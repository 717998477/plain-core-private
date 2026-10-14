use crate::graphql::types::Long;
use async_graphql::{Context, ID, Object};
use plain_rs::media::audio::queue as audio_queue;
use plain_rs::media::audio::queue::AudioTrack;
use std::sync::Arc;

use crate::assembly::context::AppCtx;
use crate::assembly::enums::MediaPlayMode;
use crate::graphql::resolvers::audio::desktop_types::AudioItem;
use crate::graphql::resolvers::audio::desktop_types::AudioPlayHistory;
use crate::graphql::resolvers::audio::desktop_types::AudioPlayback;
use crate::graphql::resolvers::audio::desktop_types::AudioPlaylist;

fn track_to_gql(a: AudioTrack) -> AudioItem {
    AudioItem {
        media_id: a.media_id.map(Into::into),
        title: a.title,
        artist: a.artist,
        path: a.path,
        duration_ms: Long(a.duration_ms),
    }
}

fn playlist_to_gql((pl, count): (plain_rs::db::Playlist, usize)) -> AudioPlaylist {
    AudioPlaylist {
        id: pl.id.into(),
        name: pl.name,
        item_count: count.min(i32::MAX as usize) as i32,
        created_at: pl.created_at,
        updated_at: pl.updated_at,
    }
}

#[derive(Default)]
pub struct AudioQuery;

#[Object]
impl AudioQuery {
    async fn audio_queue_items(
        &self,
        ctx: &Context<'_>,
        query: String,
        offset: i32,
        limit: i32,
    ) -> async_graphql::Result<Vec<AudioItem>> {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        let text = query;
        Ok(
            audio_queue::queue_page(&c.db, offset as i64, limit as i64, &text)?
                .into_iter()
                .map(track_to_gql)
                .collect(),
        )
    }
    async fn audio_queue_item_count(&self, ctx: &Context<'_>) -> async_graphql::Result<i32> {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        Ok(audio_queue::queue_total(&c.db)?.min(i32::MAX as usize) as i32)
    }
    async fn audio_playback(&self, ctx: &Context<'_>) -> async_graphql::Result<AudioPlayback> {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        let current = audio_queue::get_audio_current(&c.db)?;
        Ok(AudioPlayback {
            media_id: c.db.audio_current_media_id().map(Into::into),
            path: (!current.is_empty()).then_some(current),
            mode: media_play_mode_of(&c.prefs)?,
            is_playing: false,
            position_ms: Long(0),
        })
    }
    async fn audio_playlists(
        &self,
        ctx: &Context<'_>,
    ) -> async_graphql::Result<Vec<AudioPlaylist>> {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        Ok(audio_queue::playlists(&c.db)?
            .into_iter()
            .map(playlist_to_gql)
            .collect())
    }
    async fn audio_playlist_items(
        &self,
        ctx: &Context<'_>,
        id: ID,
        query: String,
        offset: i32,
        limit: i32,
    ) -> async_graphql::Result<Vec<AudioItem>> {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        Ok(audio_queue::playlist_items_page(
            &c.db,
            id.as_ref(),
            offset as i64,
            limit as i64,
            &query,
        )?
        .into_iter()
        .map(track_to_gql)
        .collect())
    }

    async fn audio_playlist_item_count(
        &self,
        ctx: &Context<'_>,
        id: ID,
        query: String,
    ) -> async_graphql::Result<i32> {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        Ok(audio_queue::playlist_filtered_count(&c.db, id.as_str(), &query)?.try_into()?)
    }
    async fn audio_play_history_count(&self, ctx: &Context<'_>) -> async_graphql::Result<i32> {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        Ok(plain_rs::db::audio_queue::all_history(&c.db)?
            .len()
            .try_into()?)
    }

    async fn audio_play_history(
        &self,
        ctx: &Context<'_>,
        query: String,
        offset: i32,
        limit: i32,
    ) -> async_graphql::Result<Vec<AudioPlayHistory>> {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        Ok(
            audio_queue::history_page(&c.db, offset as i64, limit as i64, &query)?
                .into_iter()
                .map(|h| AudioPlayHistory {
                    media_id: h.media_id.into(),
                    path: h.path,
                    title: h.title,
                    artist: h.artist,
                    duration_ms: Long(h.duration_ms),
                    play_count: h.play_count.clamp(0, i32::MAX as i64) as i32,
                    played_at: h.played_at,
                })
                .collect(),
        )
    }
}

#[derive(Default)]
pub struct AudioMutation;

#[Object]
impl AudioMutation {
    async fn play_audio(
        &self,
        ctx: &Context<'_>,
        media_id: ID,
    ) -> async_graphql::Result<AudioItem> {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        let track = audio_queue::track_by_id(&c.db, media_id.as_str())?;
        audio_queue::on_playing(
            &c.db,
            &track.path,
            &track.title,
            &track.artist,
            track.duration_ms,
        )?;
        Ok(track_to_gql(track))
    }

    async fn play_audio_by_path(
        &self,
        ctx: &Context<'_>,
        path: String,
    ) -> async_graphql::Result<AudioItem> {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        let track =
            c.db.with_conn(|conn| audio_queue::track_by_path_conn(conn, &path))?;
        let track = track.unwrap_or_else(|| AudioTrack::from_path_stem(&path));
        audio_queue::on_playing(
            &c.db,
            &track.path,
            &track.title,
            &track.artist,
            track.duration_ms,
        )?;
        Ok(track_to_gql(track))
    }
    async fn update_audio_play_mode(
        &self,
        ctx: &Context<'_>,
        mode: MediaPlayMode,
    ) -> async_graphql::Result<bool> {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        let mode_str = match mode {
            MediaPlayMode::Repeat => "REPEAT",
            MediaPlayMode::RepeatOne => "REPEAT_ONE",
            MediaPlayMode::Shuffle => "SHUFFLE",
        };
        audio_queue::save_audio_mode(&c.prefs, mode_str)
            .map(|_| true)
            .map_err(|error| async_graphql::Error::new(error.to_string()))
    }
    async fn clear_audio_queue(&self, ctx: &Context<'_>) -> async_graphql::Result<bool> {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        audio_queue::clear_queue(&c.db)?;
        Ok(true)
    }

    async fn remove_audio_from_queue(
        &self,
        ctx: &Context<'_>,
        media_id: ID,
    ) -> async_graphql::Result<bool> {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        audio_queue::remove_queued(&c.db, media_id.as_str())?;
        Ok(true)
    }

    async fn add_audio_to_queue(
        &self,
        ctx: &Context<'_>,
        query: String,
    ) -> async_graphql::Result<bool> {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        let tracks = desktop_tracks(ctx, &query).await?;
        audio_queue::enqueue(&c.db, &tracks, false)?;
        Ok(true)
    }

    async fn reorder_audio_queue(
        &self,
        ctx: &Context<'_>,
        media_ids: Vec<ID>,
    ) -> async_graphql::Result<bool> {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        audio_queue::reorder_queued(
            &c.db,
            &media_ids
                .into_iter()
                .map(|id| id.to_string())
                .collect::<Vec<_>>(),
        )?;
        Ok(true)
    }
    async fn create_audio_playlist(
        &self,
        ctx: &Context<'_>,
        name: String,
    ) -> async_graphql::Result<AudioPlaylist> {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        Ok(playlist_to_gql((
            audio_queue::create_playlist(&c.db, &name)?,
            0,
        )))
    }
    async fn update_audio_playlist(
        &self,
        ctx: &Context<'_>,
        id: ID,
        name: String,
    ) -> async_graphql::Result<AudioPlaylist> {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        audio_queue::rename_playlist(&c.db, id.as_ref(), &name)?;
        let pl = audio_queue::playlist_by_id(&c.db, id.as_ref())?
            .ok_or_else(|| async_graphql::Error::new(format!("Playlist {} not found", id.0)))?;
        let count = audio_queue::playlist_item_count(&c.db, id.as_ref())?;
        Ok(playlist_to_gql((pl, count)))
    }
    async fn delete_audio_playlist(
        &self,
        ctx: &Context<'_>,
        id: ID,
    ) -> async_graphql::Result<bool> {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        audio_queue::delete_playlist(&c.db, id.as_ref())?;
        Ok(true)
    }

    async fn add_audio_playlist_items(
        &self,
        ctx: &Context<'_>,
        id: ID,
        query: String,
    ) -> async_graphql::Result<bool> {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        let tracks = desktop_tracks(ctx, &query).await?;
        audio_queue::add_playlist_items(&c.db, id.as_str(), &tracks)?;
        Ok(true)
    }

    async fn remove_audio_playlist_item(
        &self,
        ctx: &Context<'_>,
        id: ID,
        query: String,
    ) -> async_graphql::Result<crate::graphql::types::ActionResult> {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        Ok(crate::graphql::types::ActionResult {
            affected_count: audio_queue::remove_playlist_query(&c.db, id.as_str(), &query)?
                .try_into()?,
        })
    }
}

async fn desktop_tracks(ctx: &Context<'_>, query: &str) -> async_graphql::Result<Vec<AudioTrack>> {
    if query.trim().is_empty() {
        return Err(async_graphql::Error::new("bulk_query_required"));
    }
    let c = ctx.data_unchecked::<Arc<AppCtx>>();
    if let Some(id) = plain_rs::utils::search_dsl::field_value(query, "playlist_id") {
        return Ok(audio_queue::playlist_items_page(
            &c.db,
            &id,
            0,
            i64::MAX,
            query,
        )?);
    }
    let sort = match plain_rs::prefs::user::snapshot(&c.prefs)?.audio_sort_by {
        0 => plain_media::index::images::MediaSort::DateAsc,
        1 | 6 => plain_media::index::images::MediaSort::DateDesc,
        2 => plain_media::index::images::MediaSort::SizeAsc,
        3 => plain_media::index::images::MediaSort::SizeDesc,
        4 => plain_media::index::images::MediaSort::NameAsc,
        5 => plain_media::index::images::MediaSort::NameDesc,
        _ => return Err(async_graphql::Error::new("invalid audio sort order")),
    };
    let mut tracks = Vec::new();
    let mut offset = 0;
    loop {
        let rows = plain_media::catalog::search_page(
            query,
            Some("audio"),
            sort,
            offset,
            500,
            Some(c.media.db.clone()),
            true,
        )
        .await?;
        let count = rows.len();
        for row in rows {
            if let Some(track) =
                c.db.with_conn(|conn| audio_queue::track_by_path_conn(conn, &row.path))?
            {
                tracks.push(track);
            }
        }
        if count < 500 {
            break;
        }
        offset += count as i32;
    }
    Ok(tracks)
}

fn media_play_mode_of(prefs: &plain_rs::prefs::Prefs) -> async_graphql::Result<MediaPlayMode> {
    Ok(audio_queue::play_mode(prefs)?)
}

#[cfg(test)]
#[path = "../../../../tests/unit/graphql/resolvers/audio/desktop.rs"]
mod tests;
