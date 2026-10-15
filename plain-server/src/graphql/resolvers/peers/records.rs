use crate::graphql::types::Instant;
use async_graphql::SimpleObject;
use serde::Deserialize;

#[derive(SimpleObject, Deserialize)]
pub struct PeerRecord {
    pub(crate) id: async_graphql::ID,
    pub(crate) name: String,
    pub(crate) ip: String,
    #[graphql(guard = "crate::http::auth::public_gate::LocalOwner")]
    pub(crate) key: String,
    pub(crate) public_key: String,
    pub(crate) status: String,
    pub(crate) port: Option<i32>,
    pub(crate) device_type: String,
    #[graphql(guard = "crate::http::auth::public_gate::LocalOwner")]
    pub(crate) token: String,
    pub(crate) created_at: Instant,
    pub(crate) updated_at: Instant,
    #[serde(default)]
    pub(crate) address: Option<PeerRecordAddress>,
}

#[derive(SimpleObject, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PeerRecordAddress {
    best_ip: String,
    name: String,
    base_url: String,
    api_url: String,
    status_ws_url: String,
}
#[derive(async_graphql::InputObject, serde::Serialize)]
struct PeerRecordInput {
    id: async_graphql::ID,
    name: String,
    ip: String,
    key: String,
    public_key: String,
    status: String,
    port: Option<i32>,
    device_type: String,
    token: String,
    created_at: Instant,
    updated_at: Instant,
}
fn record(peer: plain_rs::db::DPeer) -> async_graphql::Result<PeerRecord> {
    Ok(serde_json::from_value(serde_json::to_value(
        crate::services::peers::peer_address::view(&peer),
    )?)?)
}
#[derive(Default)]
pub struct PeerRecordQuery;
#[async_graphql::Object]
impl PeerRecordQuery {
    async fn peer_records(
        &self,
        ctx: &async_graphql::Context<'_>,
        ids: Option<Vec<async_graphql::ID>>,
    ) -> async_graphql::Result<Vec<PeerRecord>> {
        let db = ctx
            .data::<crate::assembly::server::ServerState>()?
            .db
            .clone();
        let peers = tokio::task::spawn_blocking(move || plain_rs::db::chat_store::peers::all(&db))
            .await??;
        let ids = ids.map(|ids| {
            ids.into_iter()
                .map(|id| id.to_string())
                .collect::<std::collections::HashSet<_>>()
        });
        peers
            .into_iter()
            .filter(|peer| ids.as_ref().is_none_or(|ids| ids.contains(&peer.id)))
            .map(record)
            .collect()
    }
    async fn peer_record(
        &self,
        ctx: &async_graphql::Context<'_>,
        id: async_graphql::ID,
    ) -> async_graphql::Result<Option<PeerRecord>> {
        let db = ctx
            .data::<crate::assembly::server::ServerState>()?
            .db
            .clone();
        tokio::task::spawn_blocking(move || plain_rs::db::chat_store::peers::get(&db, &id))
            .await??
            .map(record)
            .transpose()
    }
    async fn peer_file_url(
        &self,
        ctx: &async_graphql::Context<'_>,
        id: async_graphql::ID,
        expected: PeerRecordInput,
        file_id: String,
    ) -> async_graphql::Result<String> {
        let db = ctx
            .data::<crate::assembly::server::ServerState>()?
            .db
            .clone();
        let expected = serde_json::from_value(serde_json::to_value(expected)?)?;
        tokio::task::spawn_blocking(move || {
            let peer = crate::services::peers::peer_address::current(&db, &id, &expected)?;
            crate::services::peers::peer_address::file_url(&peer, &file_id)
        })
        .await?
        .map_err(|error| async_graphql::Error::new(error.to_string()))
    }
}
#[derive(Default)]
pub struct PeerRecordMutation;
#[async_graphql::Object]
impl PeerRecordMutation {
    async fn remove_peer_record(
        &self,
        ctx: &async_graphql::Context<'_>,
        id: async_graphql::ID,
    ) -> async_graphql::Result<bool> {
        Ok(crate::graphql::resolvers::peers::actions::remove(
            ctx.data::<crate::assembly::server::ServerState>()?,
            &id,
        )
        .await?)
    }
    async fn unpair_peer_record(
        &self,
        ctx: &async_graphql::Context<'_>,
        id: async_graphql::ID,
    ) -> async_graphql::Result<bool> {
        Ok(crate::graphql::resolvers::peers::actions::unpair(
            ctx.data::<crate::assembly::server::ServerState>()?,
            &id,
        )
        .await?)
    }
}

#[cfg(all(test, feature = "http_transport"))]
#[path = "../../../../tests/unit/graphql/resolvers/peers/records.rs"]
mod tests;
