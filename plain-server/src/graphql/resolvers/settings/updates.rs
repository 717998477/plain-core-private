use crate::services::settings::update_download::{self, AppUpdateDownload, Command};
use crate::{
    assembly::server::ServerState,
    services::settings::updates::{self, AppUpdateStatus},
};
use async_graphql::{Context, Object, Result};

#[derive(Default)]
pub struct AppUpdateMutation;
#[Object]
impl AppUpdateMutation {
    #[graphql(guard = "crate::http::auth::public_gate::LocalOwner")]
    async fn start_app_update_download(&self, ctx: &Context<'_>) -> Result<AppUpdateDownload> {
        Ok(update_download::execute(ctx.data::<ServerState>()?, Command::Start).await?)
    }
    #[graphql(guard = "crate::http::auth::public_gate::LocalOwner")]
    async fn cancel_app_update_download(&self, ctx: &Context<'_>) -> Result<AppUpdateDownload> {
        Ok(update_download::execute(ctx.data::<ServerState>()?, Command::Cancel).await?)
    }

    async fn check_app_update(
        &self,
        ctx: &Context<'_>,
        #[graphql(default)] automatic: bool,
    ) -> Result<AppUpdateStatus> {
        Ok(updates::check(ctx.data::<ServerState>()?, automatic).await?)
    }
}
