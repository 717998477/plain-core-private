use crate::assembly::server::ServerState;
use async_graphql::{Context, Object};

#[derive(Default)]
pub struct TlsQuery;
#[Object]
impl TlsQuery {
    async fn tls_certificate_signature(
        &self,
        ctx: &Context<'_>,
    ) -> async_graphql::Result<Vec<i32>> {
        crate::services::tls_identity::server_action(
            ctx.data::<ServerState>()?,
            serde_json::json!({"action":"get"}),
        )
        .await
        .map(|signature| signature.into_iter().map(i32::from).collect())
        .map_err(async_graphql::Error::new)
    }
}

#[cfg(all(test, feature = "http_transport"))]
#[path = "../../../../tests/unit/graphql/resolvers/settings/tls.rs"]
mod tests;
