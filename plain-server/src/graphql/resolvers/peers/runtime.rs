use async_graphql::{Context, Object};

#[derive(Default)]
pub struct PeerRuntimeMutation;
#[Object]
impl PeerRuntimeMutation {
    #[graphql(guard = "crate::http::auth::public_gate::LocalOwner")]
    async fn control_peer_status(
        &self,
        ctx: &Context<'_>,
        action: crate::services::peers::peer_status_controller::Action,
    ) -> async_graphql::Result<bool> {
        crate::services::peers::peer_status_controller::Controller::new(
            ctx.data::<crate::assembly::server::ServerState>()?.clone(),
        )
        .execute(action)
        .await
        .map_err(async_graphql::Error::new)?;
        Ok(true)
    }
}

#[cfg(test)]
#[path = "../../../../tests/unit/graphql/resolvers/peers/runtime.rs"]
mod tests;
