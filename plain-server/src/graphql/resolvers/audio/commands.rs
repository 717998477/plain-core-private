use crate::graphql::resolvers::audio::public::AudioItem;
use crate::graphql::types::Long;
use crate::services::audio::host::Audio;
use async_graphql::{Context, ID, Object, Result, SimpleObject};
use plain_rs::media::audio::{commands, playback, queue};
use plain_rs::prefs::Prefs;
use std::sync::Arc;

#[derive(SimpleObject)]
pub struct AudioPlaybackState {
    path: String,
    position_ms: Long,
    revision: Long,
}
impl From<playback::Playback> for AudioPlaybackState {
    fn from(value: playback::Playback) -> Self {
        Self {
            path: value.path,
            position_ms: Long(value.position_ms),
            revision: Long(value.revision),
        }
    }
}

#[derive(Default)]
pub struct AudioCommandsMutation;

#[Object]
impl AudioCommandsMutation {
    async fn audio_command(
        &self,
        ctx: &Context<'_>,
        action: commands::Action,
        position_ms: Long,
        speed: f32,
    ) -> Result<AudioPlaybackState> {
        let prefs = ctx.data::<Arc<Prefs>>()?.clone();
        Ok(ctx
            .data::<Arc<Audio>>()?
            .run(move |db, engine| {
                commands::command(db, &prefs, engine, action, position_ms.0, speed)
            })
            .await?
            .into())
    }
    async fn audio_play_track(
        &self,
        ctx: &Context<'_>,
        media_id: ID,
    ) -> Result<AudioPlaybackState> {
        let prefs = ctx.data::<Arc<Prefs>>()?.clone();
        Ok(ctx
            .data::<Arc<Audio>>()?
            .run(move |db, engine| {
                engine.cache_ids(&[media_id.to_string()])?;
                let track = queue::track_by_id(db, media_id.as_str())?;
                commands::play_track(db, &prefs, engine, track)
            })
            .await?
            .into())
    }
    async fn audio_play_path(&self, ctx: &Context<'_>, path: String) -> Result<AudioPlaybackState> {
        let prefs = ctx.data::<Arc<Prefs>>()?.clone();
        Ok(ctx
            .data::<Arc<Audio>>()?
            .run(move |db, engine| commands::play_path(db, &prefs, engine, &path))
            .await?
            .into())
    }
    async fn invalidate_audio_engine(&self, ctx: &Context<'_>) -> Result<AudioPlaybackState> {
        Ok(ctx
            .data::<Arc<Audio>>()?
            .run_playback(|db, _| playback::invalidate(db))
            .await?
            .into())
    }
    async fn enqueue_audios(&self, ctx: &Context<'_>, media_ids: Vec<ID>) -> Result<bool> {
        ctx.data::<Arc<Audio>>()?
            .run(move |db, engine| {
                let ids = media_ids
                    .into_iter()
                    .map(|id| id.to_string())
                    .collect::<Vec<_>>();
                engine.cache_ids(&ids)?;
                let tracks = ids
                    .iter()
                    .map(|id| queue::track_by_id(db, id))
                    .collect::<plain_rs::error::DomainResult<Vec<_>>>()?;
                queue::enqueue(db, &tracks, false)
            })
            .await?;
        Ok(true)
    }
    async fn select_next_audio(
        &self,
        ctx: &Context<'_>,
        is_next: bool,
        shuffle: bool,
    ) -> Result<Option<AudioItem>> {
        Ok(ctx
            .data::<Arc<Audio>>()?
            .run(move |db, _| queue::select_next(db, is_next, shuffle))
            .await?
            .map(Into::into))
    }
    async fn select_audio(&self, ctx: &Context<'_>, media_id: Option<ID>) -> Result<bool> {
        ctx.data::<Arc<Audio>>()?
            .run(move |db, engine| {
                let path = match media_id {
                    Some(id) => {
                        engine.cache_ids(&[id.to_string()])?;
                        queue::track_by_id(db, id.as_str())?.path
                    }
                    None => String::new(),
                };
                queue::save_audio_current(db, &path)
            })
            .await?;
        Ok(true)
    }
    async fn add_audio_playlist_media(
        &self,
        ctx: &Context<'_>,
        id: ID,
        media_ids: Vec<ID>,
    ) -> Result<i32> {
        let count = ctx
            .data::<Arc<Audio>>()?
            .run(move |db, engine| {
                let ids = media_ids
                    .into_iter()
                    .map(|id| id.to_string())
                    .collect::<Vec<_>>();
                engine.cache_ids(&ids)?;
                let tracks = ids
                    .iter()
                    .map(|id| queue::track_by_id(db, id))
                    .collect::<plain_rs::error::DomainResult<Vec<_>>>()?;
                queue::add_playlist_items(db, id.as_str(), &tracks)
            })
            .await?;
        Ok(count.try_into()?)
    }
}
