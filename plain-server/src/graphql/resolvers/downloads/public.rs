use crate::assembly::server::ServerState;
use crate::services::downloads::queue;
use crate::services::downloads::service;
use crate::services::downloads::service::DownloadDeps;
use async_graphql::{Context, Object};

fn deps(ctx: &Context<'_>) -> async_graphql::Result<DownloadDeps> {
    let state: &ServerState = ctx.data()?;
    Ok(DownloadDeps {
        db: state.db.clone(),
        data_dir: state.directory.clone(),
        events: state.events.clone(),
    })
}

#[derive(Default)]
pub struct DownloadMutation;

#[Object]
impl DownloadMutation {
    async fn pause_download(&self, message_id: String) -> async_graphql::Result<bool> {
        Ok(queue::pause_download(&message_id).await)
    }

    async fn download_peer_file(
        &self,
        ctx: &Context<'_>,
        message_id: String,
        peer_id: String,
    ) -> async_graphql::Result<bool> {
        Ok(service::start_download(deps(ctx)?, message_id, peer_id).await)
    }

    async fn resume_download(
        &self,
        ctx: &Context<'_>,
        message_id: String,
        peer_id: String,
    ) -> async_graphql::Result<bool> {
        Ok(service::resume_download(deps(ctx)?, message_id, peer_id).await)
    }

    async fn retry_download(
        &self,
        ctx: &Context<'_>,
        message_id: String,
        peer_id: String,
    ) -> async_graphql::Result<bool> {
        Ok(service::retry_download(deps(ctx)?, message_id, peer_id).await)
    }
}
