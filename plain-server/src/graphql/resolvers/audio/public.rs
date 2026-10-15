use crate::graphql::resolvers::tags::domain_types::Tag;
use crate::graphql::types::Instant;
use crate::graphql::types::Long;
use crate::host::bridge::Host;
use crate::http::auth::public_gate;
use crate::services::audio::host::Audio as AudioService;
use async_graphql::{Context, Object, SimpleObject};
use plain_rs::db::PlayHistory;
use plain_rs::db::Playlist;
use plain_rs::enums::DataType;
use plain_rs::media::audio::queue as domain;
use plain_rs::media::audio::queue::AudioTrack;
use serde_json::{Value, json};
use std::sync::Arc;

const STORAGE: &str = "WRITE_EXTERNAL_STORAGE";

pub use plain_rs::media::audio::play_mode::MediaPlayMode;

#[derive(SimpleObject, Clone, Debug)]
pub struct AudioItem {
    pub media_id: Option<async_graphql::ID>,
    pub title: String,
    pub artist: String,
    pub path: String,
    #[graphql(name = "durationMs")]
    pub duration_ms: Long,
}

impl From<AudioTrack> for AudioItem {
    fn from(track: AudioTrack) -> Self {
        item(track)
    }
}

#[derive(SimpleObject, Clone, Debug)]
#[graphql(complex)]
pub struct Audio {
    pub album_id: async_graphql::ID,
    pub id: async_graphql::ID,
    pub title: String,
    pub artist: String,
    pub path: String,
    #[graphql(name = "durationMs")]
    pub duration_ms: Long,
    pub size: Long,
    #[graphql(name = "bucketId")]
    pub bucket_id: async_graphql::ID,
    #[graphql(name = "albumFileId")]
    pub album_file_id: String,
    #[graphql(name = "createdAt")]
    pub created_at: Instant,
    #[graphql(name = "updatedAt")]
    pub updated_at: Instant,
    #[graphql(name = "isFavorite")]
    pub is_favorite: bool,
}

#[async_graphql::ComplexObject]
impl Audio {
    async fn tags(&self, ctx: &async_graphql::Context<'_>) -> async_graphql::Result<Vec<Tag>> {
        crate::graphql::resolvers::tags::loader::load(
            ctx,
            plain_rs::enums::DataType::Audio,
            self.id.as_str(),
        )
        .await
    }
}

#[derive(SimpleObject, Clone, Debug)]
pub struct AudioPlayHistory {
    pub media_id: async_graphql::ID,
    pub path: String,
    pub title: String,
    pub artist: String,
    #[graphql(name = "durationMs")]
    pub duration_ms: Long,
    #[graphql(name = "playCount")]
    pub play_count: i32,
    #[graphql(name = "playedAt")]
    pub played_at: Instant,
}

#[derive(SimpleObject, Clone, Debug)]
pub struct AudioPlaylist {
    pub id: async_graphql::ID,
    pub name: String,
    #[graphql(name = "itemCount")]
    pub item_count: i32,
    #[graphql(name = "createdAt")]
    pub created_at: Instant,
    #[graphql(name = "updatedAt")]
    pub updated_at: Instant,
}

#[derive(SimpleObject, Clone, Debug)]
pub struct AudioPlayback {
    pub media_id: Option<async_graphql::ID>,
    pub path: Option<String>,
    pub mode: MediaPlayMode,
    #[graphql(name = "isPlaying")]
    pub is_playing: bool,
    #[graphql(name = "positionMs")]
    pub position_ms: Long,
}

#[derive(Default)]
pub struct AudioQuery;

#[Object]
impl AudioQuery {
    async fn audio_count(&self, ctx: &Context<'_>, query: String) -> async_graphql::Result<i32> {
        if !public_gate::require_granted(ctx, &[STORAGE]).await? {
            return Ok(0);
        }
        crate::graphql::resolvers::media::public::media_count(ctx, "AUDIO", query).await
    }

    async fn audios(
        &self,
        ctx: &Context<'_>,
        query: String,
        offset: i32,
        limit: i32,
        sort_by: crate::graphql::resolvers::files::types::FileSortBy,
    ) -> async_graphql::Result<Vec<Audio>> {
        public_gate::require(ctx, prefs(ctx), &[STORAGE])?;
        let items = crate::graphql::types::facts::rows(
            &crate::graphql::resolvers::media::public::media_rows(
                ctx,
                json!({
                    "dataType": "AUDIO", "query": query,
                    "offset": offset, "limit": limit, "sortBy": sort_by.as_str(),
                }),
            )
            .await?,
            |item| item.clone(),
        );
        Ok(items
            .iter()
            .map(|item| {
                let id = crate::graphql::types::facts::id(item, "id");
                Audio {
                    id: id.clone(),
                    title: crate::graphql::types::facts::text(item, "title"),
                    artist: crate::graphql::types::facts::text(item, "artist"),
                    path: crate::graphql::types::facts::text(item, "path"),
                    duration_ms: Long(crate::graphql::types::facts::integer(item, "durationMs")),
                    size: Long(crate::graphql::types::facts::integer(item, "size")),
                    bucket_id: crate::graphql::types::facts::id(item, "bucketId"),
                    album_id: crate::graphql::types::facts::text(item, "albumId").into(),
                    album_file_id: crate::graphql::types::facts::text(item, "albumFileId"),
                    created_at: crate::graphql::types::facts::instant(item, "createdAt"),
                    updated_at: crate::graphql::types::facts::instant(item, "updatedAt"),
                    is_favorite: crate::graphql::types::facts::flag(item, "isFavorite"),
                }
            })
            .collect())
    }

    async fn audio_lyrics(
        &self,
        ctx: &Context<'_>,
        media_id: async_graphql::ID,
    ) -> async_graphql::Result<Option<String>> {
        public_gate::require(ctx, prefs(ctx), &[STORAGE])?;
        let path = audio(ctx)?
            .run(move |db, lib| {
                lib.cache_ids(&[media_id.to_string()])?;
                Ok(domain::track_by_id(db, media_id.as_str())?.path)
            })
            .await?;
        let lyrics = read_lyrics(ctx, path).await?;
        Ok(Some(lyrics).filter(|value| !value.trim().is_empty()))
    }
    async fn audio_queue_items(
        &self,
        ctx: &Context<'_>,
        query: String,
        offset: i32,
        limit: i32,
    ) -> async_graphql::Result<Vec<AudioItem>> {
        public_gate::require(ctx, prefs(ctx), &[STORAGE])?;
        Ok(audio(ctx)?
            .run(move |db, _| domain::queue_page(db, offset as i64, limit as i64, &query))
            .await?
            .into_iter()
            .map(item)
            .collect())
    }

    async fn audio_queue_item_count(&self, ctx: &Context<'_>) -> async_graphql::Result<i32> {
        public_gate::require(ctx, prefs(ctx), &[STORAGE])?;
        Ok(audio(ctx)?
            .run(|db, _| domain::queue_total(db))
            .await?
            .try_into()?)
    }

    async fn audio_playlists(
        &self,
        ctx: &Context<'_>,
    ) -> async_graphql::Result<Vec<AudioPlaylist>> {
        Ok(audio(ctx)?
            .run(|db, _| domain::playlists(db))
            .await?
            .into_iter()
            .map(|(row, count)| playlist(row, count))
            .collect::<async_graphql::Result<Vec<_>>>()?)
    }

    async fn audio_playlist_items(
        &self,
        ctx: &Context<'_>,
        id: async_graphql::ID,
        query: String,
        offset: i32,
        limit: i32,
    ) -> async_graphql::Result<Vec<AudioItem>> {
        Ok(audio(ctx)?
            .run(move |db, _| {
                domain::playlist_item_records_page(
                    db,
                    id.as_str(),
                    offset as i64,
                    limit as i64,
                    &query,
                )
            })
            .await?
            .into_iter()
            .map(playlist_item)
            .collect())
    }

    async fn audio_playlist_item_count(
        &self,
        ctx: &Context<'_>,
        id: async_graphql::ID,
        query: String,
    ) -> async_graphql::Result<i32> {
        Ok(audio(ctx)?
            .run(move |db, _| domain::playlist_filtered_count(db, id.as_str(), &query))
            .await?
            .try_into()?)
    }

    async fn audio_play_history(
        &self,
        ctx: &Context<'_>,
        query: String,
        offset: i32,
        limit: i32,
    ) -> async_graphql::Result<Vec<AudioPlayHistory>> {
        Ok(audio(ctx)?
            .run(move |db, _| domain::history_page(db, offset as i64, limit as i64, &query))
            .await?
            .into_iter()
            .map(history)
            .collect::<async_graphql::Result<Vec<_>>>()?)
    }
    async fn audio_play_history_count(&self, ctx: &Context<'_>) -> async_graphql::Result<i32> {
        Ok(audio(ctx)?
            .run(|db, _| Ok(plain_rs::db::audio_queue::all_history(db)?.len()))
            .await?
            .try_into()?)
    }

    async fn audio_playback(&self, ctx: &Context<'_>) -> async_graphql::Result<AudioPlayback> {
        let current = audio(ctx)?
            .run(|db, _| domain::get_audio_current(db))
            .await?;
        let mode = domain::play_mode(prefs(ctx))?;
        let player = host_call(ctx, "systemAudioPlaybackState", json!({})).await?;
        Ok(AudioPlayback {
            media_id: audio(ctx)?
                .run_playback(|db, _| Ok(db.audio_current_media_id()))
                .await?
                .map(Into::into),
            path: Some(current).filter(|path| !path.is_empty()),
            mode,
            is_playing: crate::graphql::types::facts::flag(&player, "isPlaying"),
            position_ms: Long(crate::graphql::types::facts::integer(&player, "positionMs")),
        })
    }
}

#[derive(Default)]
pub struct AudioMutation;

#[Object]
impl AudioMutation {
    async fn play_audio(
        &self,
        ctx: &Context<'_>,
        media_id: async_graphql::ID,
    ) -> async_graphql::Result<AudioItem> {
        let track = audio(ctx)?
            .run(move |db, lib| {
                lib.cache_ids(&[media_id.to_string()])?;
                let track = domain::track_by_id(db, media_id.as_str())?;
                domain::save_audio_current(db, &track.path)?;
                Ok(track)
            })
            .await?;
        Ok(item(track))
    }

    async fn play_audio_by_path(
        &self,
        ctx: &Context<'_>,
        path: String,
    ) -> async_graphql::Result<AudioItem> {
        let track = audio(ctx)?
            .run(move |db, lib| {
                use plain_rs::media::audio::commands::Engine;
                let track = lib.metadata(&path)?;
                domain::save_audio_current(db, &track.path)?;
                Ok(track)
            })
            .await?;
        Ok(item(track))
    }

    async fn update_audio_play_mode(
        &self,
        ctx: &Context<'_>,
        mode: MediaPlayMode,
    ) -> async_graphql::Result<bool> {
        crate::graphql::resolvers::settings::state::apply_user_patch(
            crate::graphql::resolvers::settings::state::UserPatchEnv::from_ctx(ctx)?,
            plain_rs::prefs::user::UserSettingsPatch {
                audio_play_mode: Some(mode.ordinal()),
                ..Default::default()
            },
        )
        .await?;
        Ok(true)
    }
    async fn clear_audio_queue(&self, ctx: &Context<'_>) -> async_graphql::Result<bool> {
        audio(ctx)?.run(|db, _| domain::clear_queue(db)).await?;
        let prefs = prefs(ctx).clone();
        audio(ctx)?
            .run(move |db, engine| {
                plain_rs::media::audio::commands::command(
                    db,
                    &prefs,
                    engine,
                    plain_rs::media::audio::commands::Action::Clear,
                    0,
                    1.0,
                )
            })
            .await?;
        notify_playback(ctx)?;
        Ok(true)
    }

    async fn remove_audio_from_queue(
        &self,
        ctx: &Context<'_>,
        media_id: async_graphql::ID,
    ) -> async_graphql::Result<bool> {
        let prefs = prefs(ctx).clone();
        audio(ctx)?
            .run(move |db, engine| {
                let current = db.audio_current_media_id().as_deref() == Some(media_id.as_str());
                domain::remove_queued(db, media_id.as_str())?;
                if current && domain::get_audio_current(db)?.is_empty() {
                    plain_rs::media::audio::commands::clear_engine(db, &prefs, engine)?;
                }
                Ok(())
            })
            .await?;
        notify_playback(ctx)?;
        Ok(true)
    }

    async fn add_audio_to_queue(
        &self,
        ctx: &Context<'_>,
        query: String,
    ) -> async_graphql::Result<bool> {
        let tracks = search_tracks(ctx, &query).await?;
        audio(ctx)?
            .run(move |db, lib| {
                lib.cache_ids(
                    &tracks
                        .iter()
                        .filter_map(|track| track.media_id.clone())
                        .collect::<Vec<_>>(),
                )?;
                domain::enqueue(db, &tracks, false)
            })
            .await?;
        notify_playback(ctx)?;
        Ok(true)
    }

    async fn reorder_audio_queue(
        &self,
        ctx: &Context<'_>,
        media_ids: Vec<async_graphql::ID>,
    ) -> async_graphql::Result<bool> {
        audio(ctx)?
            .run(move |db, _| {
                domain::reorder_queued(
                    db,
                    &media_ids
                        .into_iter()
                        .map(|id| id.to_string())
                        .collect::<Vec<_>>(),
                )
            })
            .await?;
        Ok(true)
    }

    async fn create_audio_playlist(
        &self,
        ctx: &Context<'_>,
        name: String,
    ) -> async_graphql::Result<AudioPlaylist> {
        let row = audio(ctx)?
            .run(move |db, _| domain::create_playlist(db, &name))
            .await?;
        playlist(row, 0)
    }

    async fn update_audio_playlist(
        &self,
        ctx: &Context<'_>,
        id: async_graphql::ID,
        name: String,
    ) -> async_graphql::Result<AudioPlaylist> {
        let key = id.to_string();
        audio(ctx)?
            .run({
                let key = key.clone();
                move |db, _| domain::rename_playlist(db, &key, &name)
            })
            .await?;
        let row = audio(ctx)?
            .run({
                let key = key.clone();
                move |db, _| domain::playlist_by_id(db, &key)
            })
            .await?
            .ok_or_else(|| {
                async_graphql::Error::new(format!("Playlist {key} not found after update"))
            })?;
        let count = audio(ctx)?
            .run(move |db, _| domain::playlist_item_count(db, &key))
            .await?;
        playlist(row, count)
    }

    async fn delete_audio_playlist(
        &self,
        ctx: &Context<'_>,
        id: async_graphql::ID,
    ) -> async_graphql::Result<bool> {
        audio(ctx)?
            .run(move |db, _| domain::delete_playlist(db, id.as_str()))
            .await?;
        Ok(true)
    }

    async fn add_audio_playlist_items(
        &self,
        ctx: &Context<'_>,
        id: async_graphql::ID,
        query: String,
    ) -> async_graphql::Result<bool> {
        let tracks = search_tracks(ctx, &query).await?;
        audio(ctx)?
            .run(move |db, lib| {
                lib.cache_ids(
                    &tracks
                        .iter()
                        .filter_map(|track| track.media_id.clone())
                        .collect::<Vec<_>>(),
                )?;
                domain::add_playlist_items(db, id.as_str(), &tracks)
            })
            .await?;
        Ok(true)
    }

    async fn remove_audio_playlist_item(
        &self,
        ctx: &Context<'_>,
        id: async_graphql::ID,
        query: String,
    ) -> async_graphql::Result<crate::graphql::types::ActionResult> {
        let count = audio(ctx)?
            .run(move |db, _| domain::remove_playlist_query(db, id.as_str(), &query))
            .await?;
        Ok(crate::graphql::types::ActionResult {
            affected_count: count.try_into()?,
        })
    }
}

fn notify_playback(ctx: &Context<'_>) -> async_graphql::Result<()> {
    let _ = ctx
        .data::<tokio::sync::broadcast::Sender<plain_rs::events::WsEvent>>()?
        .send(plain_rs::events::WsEvent::broadcast(
            "CONTENT_CHANGED",
            "{}".into(),
        ));
    Ok(())
}

pub(crate) async fn search_tracks(
    ctx: &Context<'_>,
    query: &str,
) -> async_graphql::Result<Vec<AudioTrack>> {
    if query.trim().is_empty() {
        return Err(async_graphql::Error::new("bulk_query_required"));
    }
    if let Some(id) = plain_rs::utils::search_dsl::field_value(query, "playlist_id") {
        let query = query.to_owned();
        return audio(ctx)?
            .run(move |db, _| domain::playlist_items_page(db, &id, 0, i64::MAX, &query))
            .await;
    }
    let sort_by = library_sort(ctx)?;
    let mut tracks = Vec::new();
    let mut offset = 0;
    loop {
        let page = crate::graphql::resolvers::media::public::library_rows(
            ctx,
            DataType::Audio,
            query.to_owned(),
            sort_by,
            offset,
            500,
        )
        .await?;
        let count = page.len();
        tracks.extend(page.into_iter().map(|row| AudioTrack {
            media_id: row["id"].as_str().map(str::to_owned),
            path: crate::graphql::types::facts::text(&row, "path"),
            title: crate::graphql::types::facts::text(&row, "title"),
            artist: crate::graphql::types::facts::text(&row, "artist"),
            album_id: String::new(),
            duration_ms: crate::graphql::types::facts::integer(&row, "durationMs"),
        }));
        if count < 500 {
            break;
        }
        offset += count as u32;
    }
    Ok(tracks)
}

async fn read_lyrics(ctx: &Context<'_>, path: String) -> async_graphql::Result<String> {
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        let _ = ctx;
        tokio::task::spawn_blocking(move || {
            plain_rs::media::audio::lyrics::extract_lyrics_from_path(&path)
        })
        .await
        .map_err(|error| async_graphql::Error::new(error.to_string()))
    }
    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        Ok(host_call(ctx, "systemAudioLyrics", json!({"path":path}))
            .await?
            .as_str()
            .unwrap_or_default()
            .to_owned())
    }
}

fn item(track: AudioTrack) -> AudioItem {
    AudioItem {
        media_id: track.media_id.map(Into::into),
        title: track.title,
        artist: track.artist,
        path: track.path,
        duration_ms: Long(track.duration_ms),
    }
}

fn playlist_item(row: plain_rs::db::PlaylistItem) -> AudioItem {
    AudioItem {
        media_id: Some(row.media_id.into()),
        title: row.title,
        artist: row.artist,
        path: row.audio_path,
        duration_ms: Long(row.duration_ms),
    }
}

fn playlist(row: Playlist, count: usize) -> async_graphql::Result<AudioPlaylist> {
    Ok(AudioPlaylist {
        id: row.id.into(),
        name: row.name,
        item_count: count.try_into()?,
        created_at: crate::graphql::resolvers::common::content_common::instant(&row.created_at)?,
        updated_at: crate::graphql::resolvers::common::content_common::instant(&row.updated_at)?,
    })
}

fn history(row: PlayHistory) -> async_graphql::Result<AudioPlayHistory> {
    Ok(AudioPlayHistory {
        media_id: row.media_id.into(),
        path: row.path,
        title: row.title,
        artist: row.artist,
        duration_ms: Long(row.duration_ms),
        play_count: row.play_count.clamp(0, i64::from(i32::MAX)) as i32,
        played_at: crate::graphql::resolvers::common::content_common::instant(&row.played_at)?,
    })
}

fn audio<'a>(ctx: &'a Context<'_>) -> async_graphql::Result<&'a Arc<AudioService>> {
    ctx.data::<Arc<AudioService>>()
}

fn library_sort(
    ctx: &Context<'_>,
) -> async_graphql::Result<crate::graphql::resolvers::files::types::FileSortBy> {
    crate::graphql::resolvers::files::types::FileSortBy::from_ordinal(
        plain_rs::prefs::user::snapshot(prefs(ctx))?.audio_sort_by,
    )
}

fn prefs<'a>(ctx: &'a Context<'_>) -> &'a Arc<plain_rs::prefs::Prefs> {
    ctx.data_unchecked::<Arc<plain_rs::prefs::Prefs>>()
}

async fn host_call(ctx: &Context<'_>, method: &str, params: Value) -> async_graphql::Result<Value> {
    ctx.data_unchecked::<Arc<Host>>()
        .call(method, params)
        .await
        .map_err(async_graphql::Error::new)
}

#[cfg(test)]
#[path = "../../../../tests/unit/graphql/resolvers/audio/public.rs"]
mod tests;
