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
#[derive(SimpleObject)]
struct LoggedInPeer {
    peer: PeerRecord,
    online: bool,
}
#[derive(async_graphql::InputObject)]
struct PeerLoginInput {
    id: async_graphql::ID,
    name: String,
    host: String,
    device_type: crate::graphql::resolvers::device::public::DeviceType,
    token: String,
    signature_public_key: String,
    chat_key: Option<String>,
}
#[derive(Default)]
pub struct PeerRecordQuery;
#[async_graphql::Object]
impl PeerRecordQuery {
    #[graphql(guard = "crate::http::auth::public_gate::LocalOwner")]
    async fn logged_in_peers(
        &self,
        ctx: &async_graphql::Context<'_>,
    ) -> async_graphql::Result<Vec<LoggedInPeer>> {
        let state = ctx.data::<crate::assembly::server::ServerState>()?;
        let db = state.db.clone();
        let mut rows =
            tokio::task::spawn_blocking(move || plain_rs::db::chat_store::peers::all(&db))
                .await??;
        rows.retain(|row| !row.token.is_empty());
        rows.sort_by(|a, b| b.created_at.cmp(&a.created_at).then(a.id.cmp(&b.id)));
        let connections = state.peer_status.connections.snapshot(&state.db);
        let online = connections["online"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(serde_json::Value::as_str)
            .collect::<std::collections::HashSet<_>>();
        rows.into_iter()
            .map(|row| {
                let is_online = online.contains(row.id.as_str());
                Ok(LoggedInPeer {
                    peer: record(row)?,
                    online: is_online,
                })
            })
            .collect()
    }

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
    #[graphql(guard = "crate::http::auth::public_gate::LocalOwner")]
    async fn login_peer(
        &self,
        ctx: &async_graphql::Context<'_>,
        input: PeerLoginInput,
    ) -> async_graphql::Result<bool> {
        let db = ctx
            .data::<crate::assembly::server::ServerState>()?
            .db
            .clone();
        let (ip, port) = plain_rs::utils::http_url::split_peer_host(&input.host);
        let device_type = input
            .device_type
            .as_str()
            .parse::<plain_rs::chat::enums::DeviceType>()?;
        tokio::task::spawn_blocking(move || {
            db.login_peer(
                &input.id,
                &input.name,
                &ip,
                port,
                device_type,
                &input.token,
                &input.signature_public_key,
                &input.chat_key.unwrap_or_default(),
            )
        })
        .await?
        .map_err(async_graphql::Error::new)?;
        Ok(true)
    }
    #[graphql(guard = "crate::http::auth::public_gate::LocalOwner")]
    async fn logout_peer(
        &self,
        ctx: &async_graphql::Context<'_>,
        id: async_graphql::ID,
    ) -> async_graphql::Result<bool> {
        let db = ctx
            .data::<crate::assembly::server::ServerState>()?
            .db
            .clone();
        tokio::task::spawn_blocking(move || db.logout_peer(&id))
            .await?
            .map_err(async_graphql::Error::new)?;
        Ok(true)
    }
    #[graphql(guard = "crate::http::auth::public_gate::LocalOwner")]
    async fn update_peer_name(
        &self,
        ctx: &async_graphql::Context<'_>,
        id: async_graphql::ID,
        name: String,
    ) -> async_graphql::Result<bool> {
        let db = ctx
            .data::<crate::assembly::server::ServerState>()?
            .db
            .clone();
        tokio::task::spawn_blocking(move || db.update_peer_name(&id, &name))
            .await?
            .map_err(async_graphql::Error::new)?;
        Ok(true)
    }

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
