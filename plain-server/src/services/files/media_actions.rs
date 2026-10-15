use crate::host::bridge::Host;
use crate::services::audio::host::Audio;
use crate::services::images::index::ImageIndex;
use async_graphql::Result;
use plain_rs::db::Db;
use plain_rs::enums::DataType;
use plain_rs::media::actions as media_actions;
use plain_rs::media::actions::Action;
use plain_rs::media::actions::Outcome;
use plain_rs::media::audio::commands as audio_commands;
use plain_rs::media::audio::playback as audio_playback;
use plain_rs::prefs::Prefs;
use serde_json::json;
use std::sync::Arc;
pub struct MediaActions {
    #[cfg(feature = "media")]
    scans: Arc<super::media_scans::MediaScans>,
    platform: Arc<super::platform::FilePlatform>,
    duration_cache: Arc<plain_rs::media::duration_cache::DurationCache>,
    db: Arc<Db>,
    host: Arc<Host>,
    audio: Arc<Audio>,
    index: Arc<ImageIndex>,
    prefs: Arc<Prefs>,
    events: tokio::sync::broadcast::Sender<plain_rs::events::WsEvent>,
}
pub struct Completed {
    pub affected_count: i32,
    pub failed_ids: Vec<String>,
}
impl MediaActions {
    pub fn new(
        #[cfg(feature = "media")] scans: Arc<super::media_scans::MediaScans>,
        platform: Arc<super::platform::FilePlatform>,
        db: Arc<Db>,
        host: Arc<Host>,
        audio: Arc<Audio>,
        index: Arc<ImageIndex>,
        prefs: Arc<Prefs>,
        events: tokio::sync::broadcast::Sender<plain_rs::events::WsEvent>,
        duration_cache: Arc<plain_rs::media::duration_cache::DurationCache>,
    ) -> Self {
        Self {
            #[cfg(feature = "media")]
            scans,
            platform,
            duration_cache,
            db,
            host,
            audio,
            index,
            prefs,
            events,
        }
    }
    pub async fn run_query(
        &self,
        kind: DataType,
        action: Action,
        query: &str,
        dest_dir: &str,
    ) -> Result<i32> {
        let capabilities = super::media_action_io::capability_list(&self.host)
            .await
            .map_err(async_graphql::Error::new)?;
        crate::services::capabilities::require_list(
            &capabilities,
            super::media_action_io::action_capability(action),
        )
        .map_err(|error| async_graphql::Error::new(error.to_string()))?;
        let from_trash = action == Action::Restore
            || (action == Action::Delete
                && capabilities.iter().any(|value| value == "MEDIA_TRASH"));
        if !matches!(
            kind,
            DataType::Audio | DataType::Video | DataType::Image | DataType::Doc
        ) {
            return Err("unsupported media kind".into());
        }
        let ids = super::catalog::MediaCatalog::new(self.db.clone(), self.host.clone())
            .ids(kind, query, from_trash)
            .await
            .map_err(async_graphql::Error::new)?;
        let mut seen = std::collections::HashSet::new();
        let ids: Vec<_> = ids
            .into_iter()
            .filter(|id| seen.insert(id.clone()))
            .collect();
        let mut affected = 0;
        let mut failed = 0;
        for batch in ids.chunks(500) {
            let result = self
                .apply(
                    kind,
                    action,
                    batch.to_vec(),
                    from_trash,
                    dest_dir.to_owned(),
                )
                .await?;
            affected += result.affected_count;
            failed += result.failed_ids.len();
        }
        if failed != 0 {
            return Err(format!(
                "{}: {failed} media actions failed ({affected} completed)",
                format!("{action:?}").to_uppercase()
            )
            .into());
        }
        Ok(affected)
    }
    pub async fn run(
        &self,
        r#type: DataType,
        action: Action,
        requested: Vec<String>,
        from_trash: bool,
        dest_dir: String,
    ) -> Result<Completed> {
        let capabilities = super::media_action_io::capability_list(&self.host)
            .await
            .map_err(async_graphql::Error::new)?;
        crate::services::capabilities::require_list(
            &capabilities,
            super::media_action_io::action_capability(action),
        )
        .map_err(|error| async_graphql::Error::new(error.to_string()))?;
        self.apply(r#type, action, requested, from_trash, dest_dir)
            .await
    }

    async fn apply(
        &self,
        r#type: DataType,
        action: Action,
        requested: Vec<String>,
        from_trash: bool,
        dest_dir: String,
    ) -> Result<Completed> {
        let invalid = Outcome {
            successful: Vec::new(),
            failed_ids: requested.clone(),
        };
        media_actions::validate(r#type, action, &requested, &invalid)?;
        if requested.is_empty() {
            return Ok(Completed {
                affected_count: 0,
                failed_ids: Vec::new(),
            });
        }
        if action == Action::Move && dest_dir.is_empty() {
            return Err("media destination required".into());
        }
        let operation = super::media_action_io::Operation::prepare(
            &self.host,
            &self.platform,
            r#type,
            action,
            requested,
            from_trash,
            dest_dir,
            #[cfg(feature = "media")]
            self.scans.clone(),
        )
        .await
        .map_err(async_graphql::Error::new)?;
        let result = if r#type == DataType::Audio {
            let prefs = self.prefs.clone();
            self.audio
                .run(move |db, engine| {
                    let before = audio_playback::snapshot(db)?;
                    operation.complete(db, |outcome| {
                        let count =
                            media_actions::cleanup(db, r#type, action, &outcome.successful)?;
                        if action != Action::Restore
                            && !before.path.is_empty()
                            && outcome
                                .successful
                                .iter()
                                .any(|item| item.path == before.path)
                        {
                            if action == Action::Move {
                                audio_commands::clear_engine(db, &prefs, engine)?;
                            } else {
                                audio_commands::command(
                                    db,
                                    &prefs,
                                    engine,
                                    audio_commands::Action::Clear,
                                    0,
                                    1.0,
                                )?;
                            }
                        }
                        Ok((count, outcome))
                    })
                })
                .await?
        } else {
            let db = self.db.clone();
            let index = self.index.clone();
            tokio::task::spawn_blocking(move || {
                operation.complete(&db, |outcome| {
                    if outcome.successful.is_empty() {
                        return Ok((0, outcome));
                    }
                    let count = if r#type == DataType::Image && action != Action::Restore {
                        index.update_cache(|db| {
                            media_actions::cleanup(db, r#type, action, &outcome.successful)
                        })?
                    } else {
                        media_actions::cleanup(&db, r#type, action, &outcome.successful)?
                    };
                    Ok((count, outcome))
                })
            })
            .await??
        };
        let (affected, outcome) = result;
        let completed_ids: Vec<_> = outcome
            .successful
            .iter()
            .map(|item| item.id.clone())
            .collect();
        if matches!(r#type, DataType::Audio | DataType::Video) {
            self.duration_cache
                .remove(r#type.media_type_str().unwrap(), &completed_ids)?;
        }
        if !completed_ids.is_empty() {
            let _ = self.events.send(plain_rs::events::WsEvent::broadcast(
                "CONTENT_CHANGED",
                "{}".into(),
            ));
            let _ = self.events.send(plain_rs::events::WsEvent::broadcast(
                plain_rs::events::MEDIA_ACTION_FINISHED,
                json!({"type":r#type.kind(),"action":action,"ids":completed_ids}).to_string(),
            ));
        }
        Ok(Completed {
            affected_count: affected.try_into()?,
            failed_ids: outcome.failed_ids,
        })
    }
}
