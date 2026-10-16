use crate::{assembly::server::ServerState, services::http_server::HttpServerState};
use async_graphql::{Context, Object, Result};

#[derive(Default)]
pub struct HttpServerQuery;
#[Object]
impl HttpServerQuery {
    #[graphql(guard = "crate::http::auth::public_gate::LocalOwner")]
    async fn http_server(&self, ctx: &Context<'_>) -> Result<HttpServerState> {
        Ok(ctx.data::<ServerState>()?.http_server.snapshot())
    }
}

#[derive(Default)]
pub struct HttpServerMutation;
#[Object]
impl HttpServerMutation {
    #[graphql(guard = "crate::http::auth::public_gate::LocalOwner")]
    async fn restart_http_server(
        &self,
        ctx: &Context<'_>,
        http_port: Option<u16>,
        https_port: Option<u16>,
    ) -> Result<HttpServerState> {
        crate::services::http_server::restart(ctx.data::<ServerState>()?, http_port, https_port)
            .map_err(async_graphql::Error::new)
    }
}
