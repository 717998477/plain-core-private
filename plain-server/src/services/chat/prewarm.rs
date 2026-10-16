use crate::assembly::server::ServerState;
use crate::services::peers::peer_transport;
use crate::services::peers::peer_wire;
use anyhow::{Result, bail};
use plain_rs::chat::attachments::prewarm;
use plain_rs::chat::attachments::prewarm::Advertisement;
use plain_rs::chat::attachments::prewarm::Capabilities;
use plain_rs::chat::attachments::prewarm::Driver;
use serde_json::{Value, json};

struct Native<'a>(&'a ServerState);
impl Driver for Native<'_> {
    async fn capabilities(&self) -> Result<Capabilities> {
        Ok(serde_json::from_value(
            self.0
                .host
                .call("peerTransportPrewarmCapabilities", json!({}))
                .await
                .map_err(anyhow::Error::msg)?,
        )?)
    }
    async fn scan(&self, short_id: &str) -> Result<Option<Advertisement>> {
        Ok(serde_json::from_value(
            self.0
                .host
                .call(
                    "peerTransportPrewarmScan",
                    json!({"shortId":short_id,"timeoutMs":15000}),
                )
                .await
                .map_err(anyhow::Error::msg)?,
        )?)
    }
    async fn start_aware(&self, peer_id: &str) -> Result<bool> {
        let prepared = peer_wire::prepare(
            &self.0.db,
            &self.0.prefs,
            peer_id,
            peer_wire::Operation::StartAware,
        )?;
        if !prepared.peer.is_paired() {
            bail!("Peer no longer paired")
        }
        let response = peer_transport::send(
            self.0,
            &prepared.peer,
            &prepared.channel_id,
            &plain_rs::utils::base64::base64_decode(&prepared.key),
            &prepared.body,
        )
        .await
        .map_err(anyhow::Error::msg)?;
        started(&response)
    }
}
fn started(response: &Value) -> Result<bool> {
    if response
        .get("errors")
        .is_some_and(|v| !v.is_null() && !v.as_array().is_some_and(Vec::is_empty))
    {
        bail!("Peer startAware returned GraphQL errors");
    }
    response["data"]["startAware"]
        .as_bool()
        .ok_or_else(|| anyhow::anyhow!("Invalid peer startAware receipt"))
}
pub(crate) async fn execute(state: &ServerState, id: &str) -> Result<Option<Advertisement>> {
    let mut stop = state.stop.clone();
    if *stop.borrow() {
        bail!("Server stopped");
    }
    let native = Native(state);
    tokio::select! {
        _ = stop.changed() => Err(anyhow::anyhow!("Server stopped")),
        result = prewarm::run(&state.db, &state.prewarmer, id, &native) => result,
    }
}
#[cfg(test)]
#[path = "../../../tests/unit/services/chat/prewarm.rs"]
mod tests;
