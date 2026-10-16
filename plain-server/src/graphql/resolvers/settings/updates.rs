use crate::{
    assembly::server::ServerState,
    services::settings::updates::{self, AppUpdateStatus},
};
use async_graphql::{Context, Object, Result};

#[derive(Default)]
pub struct AppUpdateMutation;
#[Object]
impl AppUpdateMutation {
    async fn check_app_update(
        &self,
        ctx: &Context<'_>,
        #[graphql(default)] automatic: bool,
    ) -> Result<AppUpdateStatus> {
        Ok(updates::check(ctx.data::<ServerState>()?, automatic).await?)
    }
}
