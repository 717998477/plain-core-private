use crate::assembly::server::ServerState;
use plain_rs::db::DPeer;
use plain_rs::transport::router::Outcome;
use plain_rs::transport::router::Router;
use plain_rs::transport::router::Ticket;
use plain_rs::transport::router::TransportType;
use serde_json::{Value, json};
use std::time::Duration;
pub(crate) fn notify(state: &ServerState) {
    let _ = state.events.send(plain_rs::events::WsEvent::broadcast(
        "PEER_TRANSPORT_UPDATED",
        "".into(),
    ));
}
pub(crate) struct Pending<'a> {
    pub(crate) router: &'a Router,
    pub(crate) state: &'a ServerState,
    pub(crate) ticket: Ticket,
}
impl Drop for Pending<'_> {
    fn drop(&mut self) {
        self.router.abort(&self.ticket);
        notify(self.state);
    }
}
pub(crate) async fn send(
    state: &ServerState,
    peer: &DPeer,
    channel_id: &str,
    key: &[u8],
    body: &str,
) -> Result<Value, String> {
    send_checked(state, peer, channel_id, key, body, || Ok(())).await
}
pub(crate) async fn send_checked(
    state: &ServerState,
    peer: &DPeer,
    channel_id: &str,
    key: &[u8],
    body: &str,
    validate: impl Fn() -> Result<(), String>,
) -> Result<Value, String> {
    if key.len() != 32 {
        return Err("Invalid peer transport key".into());
    }
    if *state.stop.borrow() {
        return Err("Core stopped".into());
    }
    let host = &state.host;
    let router = &state.transport;
    let mut available = vec![TransportType::Lan];
    if host.connected() {
        let sdk = host.call("peerTransportCapabilities", json!({})).await?;
        let sdk: Vec<TransportType> = serde_json::from_value(sdk).map_err(|e| e.to_string())?;
        available.extend(sdk.into_iter().filter(|kind| *kind != TransportType::Lan));
    }
    let mut step = router.begin(peer, &available).map_err(|e| e.to_string())?;
    let first = step.ticket.take().ok_or_else(|| {
        step.error
            .unwrap_or_else(|| "Peer transport unavailable".into())
    })?;
    let mut pending = Pending {
        state,
        router,
        ticket: first,
    };
    loop {
        notify(state);
        validate()?;
        crate::services::peers::peer_address::current(&state.db, &peer.id, peer)
            .map_err(|e| e.to_string())?;
        if pending.ticket.transport == TransportType::Lan {
            let mut stop = state.stop.clone();
            let response = tokio::select! {_=stop.changed()=>return Err("Core stopped".into()),result=crate::services::peers::peer_lan::send(state,peer,channel_id,key,body,&validate)=>result};
            match response {
                Ok(response) => {
                    validate()?;
                    crate::services::peers::peer_address::current(&state.db, &peer.id, peer)
                        .map_err(|e| e.to_string())?;
                    router
                        .finish(&pending.ticket, Outcome::Connected)
                        .map_err(|e| e.to_string())?;
                    return Ok(response);
                }
                Err(crate::services::peers::peer_lan::Failure::Fatal(error)) => {
                    return Err(error);
                }
                Err(crate::services::peers::peer_lan::Failure::Unavailable(error)) => {
                    let step = router
                        .finish(&pending.ticket, Outcome::Unavailable { error })
                        .map_err(|e| e.to_string())?;
                    match step.ticket {
                        Some(ticket) => {
                            pending.ticket = ticket;
                            continue;
                        }
                        None => {
                            return Err(step
                                .error
                                .unwrap_or_else(|| "Peer transport unavailable".into()));
                        }
                    }
                }
            }
        }
        let attempt = tokio::time::timeout(Duration::from_secs(15), async {
            match pending.ticket.transport {
                TransportType::Aware => {
                    crate::services::peers::peer_sdk::aware_send(state, peer, channel_id, key, body)
                        .await
                }
                TransportType::Ble => {
                    crate::services::peers::peer_sdk::ble_send(state, peer, channel_id, key, body)
                        .await
                }
                TransportType::Lan => unreachable!(),
            }
        })
        .await;
        let outcome = match attempt {
            Err(_) => return Err("SDK response timed out; delivery is unconfirmed".into()),
            Ok(Ok(response)) => {
                validate()?;
                crate::services::peers::peer_address::current(&state.db, &peer.id, peer)
                    .map_err(|e| e.to_string())?;
                router
                    .finish(&pending.ticket, Outcome::Connected)
                    .map_err(|e| e.to_string())?;
                return Ok(response);
            }
            Ok(Err(crate::services::peers::peer_lan::Failure::Fatal(error))) => {
                return Err(error);
            }
            Ok(Err(crate::services::peers::peer_lan::Failure::Unavailable(error))) => {
                Outcome::Unavailable { error }
            }
        };
        let step = router
            .finish(&pending.ticket, outcome)
            .map_err(|e| e.to_string())?;
        match step.ticket {
            Some(ticket) => pending.ticket = ticket,
            None => {
                return Err(step
                    .error
                    .unwrap_or_else(|| "Peer transport unavailable".into()));
            }
        }
    }
}
#[derive(Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AwareConnectionConfig {
    pub(crate) pmk: Option<String>,
    pub(crate) is_client: bool,
    pub(crate) local_port: u16,
}
pub(crate) fn aware_config_for_peer(
    state: &ServerState,
    peer: &DPeer,
) -> anyhow::Result<AwareConnectionConfig> {
    let key = if peer.key.is_empty() {
        Vec::new()
    } else {
        plain_rs::utils::base64::base64_decode_checked(&peer.key)?
    };
    anyhow::ensure!(
        key.is_empty() || key.len() == 32,
        "Invalid Aware pairing key"
    );
    let actor = state
        .prefs
        .get::<String>(plain_rs::prefs::system::keys::CLIENT_ID)?
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow::anyhow!("Missing local identity"))?;
    Ok(AwareConnectionConfig {
        pmk: if key.is_empty() {
            None
        } else {
            Some(plain_rs::utils::base64::base64_encode(&key))
        },
        is_client: actor < peer.id,
        local_port: state.prefs.get_user::<u16>("https_port")?.unwrap_or(8443),
    })
}
#[cfg(test)]
#[path = "../../../tests/unit/services/peers/peer_transport.rs"]
mod tests;
