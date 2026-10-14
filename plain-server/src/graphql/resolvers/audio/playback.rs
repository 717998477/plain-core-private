use crate::graphql::types::Long;
use crate::services::audio::host::Audio;
use async_graphql::{Context, Object, SimpleObject};
use plain_rs::media::audio::playback as audio_playback;
use plain_rs::media::audio::playback::Playback;
use std::sync::Arc;

#[derive(SimpleObject)]
pub struct AudioPlayback {
    path: String,
    position_ms: Long,
    revision: Long,
}
impl From<Playback> for AudioPlayback {
    fn from(row: Playback) -> Self {
        Self {
            path: row.path,
            position_ms: Long(row.position_ms),
            revision: Long(row.revision),
        }
    }
}

#[derive(Default)]
pub struct AudioPlaybackQuery;
#[Object]
impl AudioPlaybackQuery {
    async fn audio_playback(&self, ctx: &Context<'_>) -> async_graphql::Result<AudioPlayback> {
        Ok(ctx
            .data::<Arc<Audio>>()?
            .run_playback(|db, _| audio_playback::snapshot(db))
            .await?
            .into())
    }
}
