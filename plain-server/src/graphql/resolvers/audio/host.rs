use crate::graphql::resolvers::audio::public::AudioItem;
use crate::graphql::types::Long;
use crate::services::audio::host::Audio;
use async_graphql::{Context, ID, InputObject, Object, SimpleObject};
use plain_rs::media::audio::queue as domain;
use plain_rs::media::audio::queue::AudioTrack;
use std::sync::Arc;

#[derive(InputObject)]
pub struct AudioHostTrackInput {
    pub(crate) path: String,
    media_id: Option<ID>,
    title: String,
    artist: String,
    album_id: ID,
    duration_ms: Long,
}
impl From<AudioHostTrackInput> for AudioTrack {
    fn from(row: AudioHostTrackInput) -> Self {
        Self {
            media_id: row.media_id.map(|id| id.to_string()),
            path: row.path,
            title: row.title,
            artist: row.artist,
            album_id: row.album_id.to_string(),
            duration_ms: row.duration_ms.0,
        }
    }
}

#[derive(SimpleObject)]
struct AudioArtistPlayCount {
    artist: String,
    count: Long,
}
fn audio<'a>(ctx: &'a Context<'_>) -> async_graphql::Result<&'a Arc<Audio>> {
    ctx.data::<Arc<Audio>>()
}

#[derive(Default)]
pub struct AudioHostQuery;
#[Object]
impl AudioHostQuery {
    async fn audio_queued_media_ids(&self, ctx: &Context<'_>) -> async_graphql::Result<Vec<ID>> {
        audio(ctx)?
            .run(|db, _| {
                Ok(plain_rs::db::audio_queue::all_queue_items(db)?
                    .into_iter()
                    .map(|row| row.media_id.into())
                    .collect())
            })
            .await
    }

    async fn audio_artist_play_counts(
        &self,
        ctx: &Context<'_>,
    ) -> async_graphql::Result<Vec<AudioArtistPlayCount>> {
        Ok(audio(ctx)?
            .run(|db, _| domain::artist_play_counts(db))
            .await?
            .into_iter()
            .map(|(artist, count)| AudioArtistPlayCount {
                artist,
                count: Long(count),
            })
            .collect())
    }
}

#[derive(Default)]
pub struct AudioHostMutation;
#[Object]
impl AudioHostMutation {
    async fn audio_host_enqueue(
        &self,
        ctx: &Context<'_>,
        media_ids: Vec<ID>,
    ) -> async_graphql::Result<bool> {
        audio(ctx)?
            .run(move |db, lib| {
                let ids = media_ids
                    .into_iter()
                    .map(|id| id.to_string())
                    .collect::<Vec<_>>();
                lib.cache_ids(&ids)?;
                let tracks = ids
                    .iter()
                    .map(|id| domain::track_by_id(db, id))
                    .collect::<plain_rs::error::DomainResult<Vec<_>>>()?;
                domain::enqueue(db, &tracks, false)
            })
            .await?;
        Ok(true)
    }

    async fn audio_host_remove_paths(
        &self,
        ctx: &Context<'_>,
        paths: Vec<String>,
    ) -> async_graphql::Result<bool> {
        audio(ctx)?
            .run(move |db, _| domain::remove_paths(db, &paths))
            .await?;
        Ok(true)
    }
    async fn audio_host_set_current(
        &self,
        ctx: &Context<'_>,
        media_id: Option<ID>,
    ) -> async_graphql::Result<bool> {
        audio(ctx)?
            .run(move |db, lib| {
                let path = if let Some(id) = media_id {
                    lib.cache_ids(&[id.to_string()])?;
                    domain::track_by_id(db, id.as_str())?.path
                } else {
                    String::new()
                };
                domain::save_audio_current(db, &path)
            })
            .await?;
        Ok(true)
    }

    async fn audio_host_resolve_next(
        &self,
        ctx: &Context<'_>,
        is_next: bool,
        shuffle: bool,
    ) -> async_graphql::Result<Option<AudioItem>> {
        Ok(audio(ctx)?
            .run(move |db, _| domain::select_next(db, is_next, shuffle))
            .await?
            .map(Into::into))
    }
    async fn audio_host_add_playlist_items(
        &self,
        ctx: &Context<'_>,
        id: ID,
        media_ids: Vec<ID>,
    ) -> async_graphql::Result<i32> {
        Ok(audio(ctx)?
            .run(move |db, lib| {
                let ids = media_ids
                    .into_iter()
                    .map(|id| id.to_string())
                    .collect::<Vec<_>>();
                lib.cache_ids(&ids)?;
                let tracks = ids
                    .iter()
                    .map(|id| domain::track_by_id(db, id))
                    .collect::<plain_rs::error::DomainResult<Vec<_>>>()?;
                domain::add_playlist_items(db, id.as_str(), &tracks)
            })
            .await?
            .try_into()?)
    }
}
