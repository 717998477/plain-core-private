use super::management::{Identity, current, current_peer};
use crate::assembly::server::ServerState;
use anyhow::Result;
use plain_rs::events::WsEvent;
use serde::Deserialize;
use serde_json::json;
use tokio::sync::watch;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct AppSnapshot {
    pub(super) device_name: String,
    pub(super) capabilities: Vec<String>,
    pub(super) permissions: Vec<String>,
    pub(super) url_token: String,
}

fn apply(
    connection: &rusqlite::Connection,
    state: &ServerState,
    id: &str,
    identity: &Identity,
    cancelled: &watch::Receiver<bool>,
    name: &str,
) -> Result<bool> {
    if !current(connection, state, id, identity, cancelled)? {
        return Ok(false);
    }
    if !name.is_empty() {
        let previous: String =
            connection.query_row("SELECT name FROM peers WHERE id=?1", [id], |row| row.get(0))?;
        if previous != name {
            plain_rs::db::chat_store::peers::update_name(connection, id, name)?;
            let _ = state.events.send(WsEvent::broadcast(
                plain_rs::chat::events::WS_PEER_UPDATED,
                json!({"id":id}).to_string(),
            ));
        }
    }
    Ok(true)
}

pub(super) async fn refresh(
    state: &ServerState,
    id: &str,
    identity: &Identity,
    cancelled: &watch::Receiver<bool>,
) -> Result<Option<AppSnapshot>> {
    if !current_peer(state, id, identity, cancelled).await? {
        return Ok(None);
    }
    let data = super::management_http::query(
        state,
        &identity.http_url()?,
        &state.native_client_id().map_err(anyhow::Error::msg)?,
        &plain_rs::utils::base64::base64_decode(&identity.token),
        json!({"query":"{ app { deviceName capabilities permissions urlToken } }"}),
        256 * 1024,
    )
    .await?;
    let app: AppSnapshot = serde_json::from_value(data["app"].clone())?;
    let state = state.clone();
    let id = id.to_owned();
    let identity = identity.clone();
    let cancelled = cancelled.clone();
    tokio::task::spawn_blocking(move || {
        state.db.with_conn(|connection| {
            Ok(apply(
                connection,
                &state,
                &id,
                &identity,
                &cancelled,
                &app.device_name,
            )?
            .then_some(app))
        })
    })
    .await?
}

#[cfg(test)]
#[path = "../../../tests/unit/services/peers/metadata.rs"]
mod tests;
