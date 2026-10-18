use super::types::{ScanProgress, ScanState};
use crate::assembly::server::ServerState;
use crate::services::files::scanning::{self, Command, State};
use async_graphql::{Context, Object, Result};

async fn state<'a>(ctx: &'a Context<'_>) -> Result<&'a ServerState> {
    let state = ctx.data::<ServerState>()?;
    crate::http::auth::public_gate::require(ctx, &state.prefs, &[super::public::STORAGE])?;
    if !crate::http::auth::public_gate::require_granted(ctx, &[super::public::STORAGE])
        .await
        .map_err(|error| async_graphql::Error::new(error.to_string()))?
    {
        return Err("no_permission".into());
    }
    Ok(state)
}
#[derive(Default)]
pub struct MediaScanQuery;
#[Object]
impl MediaScanQuery {
    async fn scan_progress(&self, ctx: &Context<'_>) -> Result<ScanProgress> {
        let progress = scanning::progress(state(ctx).await?).await?;
        Ok(ScanProgress {
            indexed: progress.indexed,
            pending: progress
                .total
                .map(|total| total.saturating_sub(progress.indexed)),
            total: progress.total,
            state: match progress.state {
                State::Idle => ScanState::IDLE,
                State::Running => ScanState::RUNNING,
                State::Paused => ScanState::PAUSED,
                State::Stopped => ScanState::STOPPED,
                State::Failed => ScanState::FAILED,
            },
            error: progress.error,
        })
    }
    async fn media_source_dirs(&self, ctx: &Context<'_>) -> Result<Vec<String>> {
        Ok(scanning::directories(state(ctx).await?).await?)
    }
}
async fn command(ctx: &Context<'_>, action: Command) -> Result<bool> {
    scanning::execute(state(ctx).await?, action).await?;
    Ok(true)
}
#[derive(Default)]
pub struct MediaScanMutation;
#[Object]
impl MediaScanMutation {
    async fn rebuild_media_index(&self, ctx: &Context<'_>, root: String) -> Result<bool> {
        command(ctx, Command::Rebuild(root)).await
    }
    async fn pause_media_scan(&self, ctx: &Context<'_>) -> Result<bool> {
        command(ctx, Command::Pause).await
    }
    async fn resume_media_scan(&self, ctx: &Context<'_>) -> Result<bool> {
        command(ctx, Command::Resume).await
    }
    async fn stop_media_scan(&self, ctx: &Context<'_>) -> Result<bool> {
        command(ctx, Command::Stop).await
    }
    async fn set_media_source_dirs(&self, ctx: &Context<'_>, dirs: Vec<String>) -> Result<bool> {
        command(ctx, Command::SetDirectories(dirs)).await
    }
}

#[cfg(test)]
#[path = "../../../../tests/unit/graphql/resolvers/media/scan.rs"]
mod tests;
