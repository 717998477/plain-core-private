use crate::assembly::server::ServerState;
use async_graphql::{Context, Object};
use serde_json::{Value, json};

async fn execute(ctx: &Context<'_>, command: Value) -> async_graphql::Result<Vec<i32>> {
    let identity = ctx
        .data::<ServerState>()?
        .tls_identity
        .read()
        .map_err(|error| async_graphql::Error::new(error.to_string()))?
        .clone();
    let value = tokio::task::spawn_blocking(move || identity.action(&command.to_string()))
        .await?
        .map_err(async_graphql::Error::new)?;
    let value: Value = serde_json::from_str(&value)?;
    Ok(
        serde_json::from_value::<Vec<u8>>(value["signature"].clone())?
            .into_iter()
            .map(i32::from)
            .collect(),
    )
}
#[derive(Default)]
pub struct TlsQuery;
#[Object]
impl TlsQuery {
    async fn tls_certificate_signature(
        &self,
        ctx: &Context<'_>,
    ) -> async_graphql::Result<Vec<i32>> {
        execute(ctx, json!({"action":"get"})).await
    }
}
#[derive(Default)]
pub struct TlsMutation;
#[Object]
impl TlsMutation {
    async fn regenerate_tls_certificate(
        &self,
        ctx: &Context<'_>,
    ) -> async_graphql::Result<Vec<i32>> {
        execute(ctx, json!({"action":"generate"})).await
    }
    async fn import_tls_certificate_pem(
        &self,
        ctx: &Context<'_>,
        certificate_pem: String,
        private_key_pem: String,
    ) -> async_graphql::Result<Vec<i32>> {
        execute(ctx,json!({"action":"import","certificatePem":certificate_pem,"privateKeyPem":private_key_pem})).await
    }
    async fn import_tls_certificate_pkcs12(
        &self,
        ctx: &Context<'_>,
        data: String,
        password: String,
    ) -> async_graphql::Result<Vec<i32>> {
        execute(
            ctx,
            json!({"action":"importPkcs12","data":data,"password":password}),
        )
        .await
    }
}

#[cfg(all(test, feature = "http_transport"))]
#[path = "../../../../tests/unit/graphql/resolvers/settings/tls.rs"]
mod tests;
