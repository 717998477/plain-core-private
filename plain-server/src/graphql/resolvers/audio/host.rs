use crate::graphql::types::Long;
use crate::services::audio::host::Audio;
use async_graphql::{Context, ID, Object, SimpleObject};
use plain_rs::media::audio::queue as domain;
use std::sync::Arc;

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
