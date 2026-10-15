use super::client::{
    SharedDownloadKind, SharedEntry, SharedEntryInput, SharedLink, SharedLinkInput, domain,
};
use crate::assembly::server::ServerState;
use crate::graphql::types::Long;
use async_graphql::{Context, Enum, InputObject, Object, SimpleObject};
use serde::{Deserialize, Serialize};

#[derive(InputObject, Serialize)]
struct SharedDownloadInput {
    message_id: async_graphql::ID,
    kind: SharedDownloadKind,
    link: SharedLinkInput,
    url_token: String,
    entries: Vec<SharedEntryInput>,
    target_dir: String,
    downloads_base: String,
    zip_name: String,
}
#[derive(Enum, Copy, Clone, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
enum SharedBatchStatus {
    Pending,
    Downloading,
    Paused,
    Completed,
    Partial,
    Failed,
    Canceled,
}
#[derive(SimpleObject, Deserialize)]
struct SharedDownloadFailure {
    path: String,
    error: String,
}
#[derive(SimpleObject, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SharedDownloadTask {
    id: async_graphql::ID,
    message_id: async_graphql::ID,
    r#type: SharedDownloadKind,
    title: String,
    target_dir: String,
    link: SharedLink,
    url_token: String,
    entries: Vec<SharedEntry>,
    zip_name: String,
    generation: String,
    status: SharedBatchStatus,
    error: String,
    downloaded_size: Long,
    total_size: Long,
    download_speed: Long,
    total_files: i32,
    done_files: i32,
    failed_files: i32,
    current_file: String,
    packing: bool,
    failures: Vec<SharedDownloadFailure>,
}
#[derive(SimpleObject, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SharedDownloadQueue {
    runtime_id: String,
    revision: Long,
    tasks: Vec<SharedDownloadTask>,
}
#[derive(Default)]
pub struct SharedDownloadQuery;
#[Object]
impl SharedDownloadQuery {
    async fn shared_download_queue(
        &self,
        ctx: &Context<'_>,
    ) -> async_graphql::Result<SharedDownloadQueue> {
        let runtime = ctx.data::<ServerState>()?.shared_batches.clone();
        Ok(serde_json::from_value(
            tokio::task::spawn_blocking(move || runtime.queue.snapshot()).await?,
        )?)
    }
}
#[derive(Default)]
pub struct SharedDownloadMutation;
#[Object]
impl SharedDownloadMutation {
    async fn enqueue_shared_download(
        &self,
        ctx: &Context<'_>,
        input: SharedDownloadInput,
    ) -> async_graphql::Result<async_graphql::ID> {
        let runtime = ctx.data::<ServerState>()?.shared_batches.clone();
        let intent = domain(input)?;
        tokio::task::spawn_blocking(move || runtime.queue.enqueue(intent))
            .await?
            .map(async_graphql::ID)
            .map_err(|error| async_graphql::Error::new(error.to_string()))
    }
    async fn pause_shared_download(
        &self,
        ctx: &Context<'_>,
        id: async_graphql::ID,
    ) -> async_graphql::Result<bool> {
        crate::services::peers::shared_batch::control(ctx.data::<ServerState>()?, &id, "pause")
            .await
            .map_err(|error| async_graphql::Error::new(error.to_string()))
    }
    async fn resume_shared_download(
        &self,
        ctx: &Context<'_>,
        id: async_graphql::ID,
    ) -> async_graphql::Result<bool> {
        crate::services::peers::shared_batch::control(ctx.data::<ServerState>()?, &id, "resume")
            .await
            .map_err(|error| async_graphql::Error::new(error.to_string()))
    }
    async fn retry_shared_download(
        &self,
        ctx: &Context<'_>,
        id: async_graphql::ID,
    ) -> async_graphql::Result<bool> {
        crate::services::peers::shared_batch::control(ctx.data::<ServerState>()?, &id, "retry")
            .await
            .map_err(|error| async_graphql::Error::new(error.to_string()))
    }
    async fn cancel_shared_download(
        &self,
        ctx: &Context<'_>,
        id: async_graphql::ID,
    ) -> async_graphql::Result<bool> {
        crate::services::peers::shared_batch::control(ctx.data::<ServerState>()?, &id, "cancel")
            .await
            .map_err(|error| async_graphql::Error::new(error.to_string()))
    }
    async fn remove_shared_download(
        &self,
        ctx: &Context<'_>,
        id: async_graphql::ID,
    ) -> async_graphql::Result<bool> {
        crate::services::peers::shared_batch::control(ctx.data::<ServerState>()?, &id, "remove")
            .await
            .map_err(|error| async_graphql::Error::new(error.to_string()))
    }
}
