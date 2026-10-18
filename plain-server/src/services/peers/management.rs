use crate::assembly::server::ServerState;
use anyhow::{Result, anyhow, ensure};
use futures_util::{SinkExt, StreamExt};
use plain_rs::{
    db::{DPeer, chat_store::peers},
    events::WsEvent,
};
use rusqlite::OptionalExtension;
use serde_json::json;
use std::{collections::HashMap, sync::Mutex, time::Duration};
use tokio::{sync::watch, task::JoinHandle};
use tokio_tungstenite::tungstenite::Message;

#[derive(Clone, PartialEq, Eq)]
struct Identity {
    url: String,
    token: String,
    ip: String,
    port: Option<u16>,
}

struct Worker {
    identity: Identity,
    cancel: watch::Sender<bool>,
    job: JoinHandle<()>,
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.cancel.send_replace(true);
        self.job.abort();
    }
}

#[derive(Default)]
pub(crate) struct Runtime {
    coordinator: Mutex<Option<(watch::Sender<bool>, JoinHandle<()>)>>,
}

impl Runtime {
    pub(crate) fn start(&self, state: ServerState) -> Result<()> {
        let actor = state.native_client_id().map_err(anyhow::Error::msg)?;
        let mut slot = self
            .coordinator
            .lock()
            .map_err(|_| anyhow!("Management runtime unavailable"))?;
        ensure!(slot.is_none(), "Management runtime already started");
        let mut changed = state.db.peer_changes();
        let mut events = state.events.subscribe();
        let (cancel, mut cancelled) = watch::channel(false);
        let coordinator = tokio::spawn(async move {
            let mut stop = state.stop.clone();
            let mut workers = HashMap::new();
            loop {
                if *stop.borrow() || *cancelled.borrow() {
                    break;
                }
                let wanted = tokio::select! {
                    biased;
                    _ = stop.changed() => break,
                    _ = cancelled.changed() => break,
                    wanted = desired(&state, &actor) => wanted,
                };
                let retry = match wanted {
                    Ok(wanted) => {
                        reconcile(&state, &mut workers, wanted).await;
                        false
                    }
                    Err(error) => {
                        log::warn!("Management peer reconciliation: {error}");
                        true
                    }
                };
                let retry_delay = tokio::time::sleep(Duration::from_secs(5));
                tokio::pin!(retry_delay);
                let again = loop {
                    tokio::select! {
                        biased;
                        _ = stop.changed() => break false,
                        _ = cancelled.changed() => break false,
                        result = changed.changed() => break result.is_ok(),
                        _ = &mut retry_delay, if retry => break true,
                        event = events.recv() => match event {
                            Ok(event) if event.event_type == "NETWORK_CHANGED" || event.event_type == "NETWORK_INTERFACES_CHANGED" => break true,
                            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => break true,
                            Err(tokio::sync::broadcast::error::RecvError::Closed) => break false,
                            _ => {},
                        },
                    }
                };
                if !again {
                    break;
                }
            }
            retire(&mut workers).await;
        });
        *slot = Some((cancel, coordinator));
        Ok(())
    }

    pub(crate) async fn shutdown(&self) {
        let coordinator = self.coordinator.lock().unwrap().take();
        if let Some((cancel, coordinator)) = coordinator {
            cancel.send_replace(true);
            let _ = coordinator.await;
        }
    }
}

fn identity(peer: &DPeer, actor: &str, base: &str) -> Option<Identity> {
    if peer.id == actor
        || peer.port.is_none_or(|port| port == 0)
        || actor.is_empty()
        || plain_rs::utils::base64::base64_decode(&peer.token).len() != 32
    {
        return None;
    }
    let mut url = reqwest::Url::parse(base).ok()?;
    if url.scheme() != "https" || url.host_str().is_none() {
        return None;
    }
    url.set_scheme("wss").ok()?;
    url.set_path("/");
    url.set_query(None);
    url.set_fragment(None);
    url.query_pairs_mut().append_pair("cid", actor);
    Some(Identity {
        url: url.to_string(),
        token: peer.token.clone(),
        ip: peer.ip.clone(),
        port: peer.port,
    })
}

async fn desired(state: &ServerState, actor: &str) -> Result<HashMap<String, Identity>> {
    let state = state.clone();
    let actor = actor.to_owned();
    tokio::task::spawn_blocking(move || -> Result<HashMap<String, Identity>> {
        let interfaces = plain_rs::transport::lan::local_interfaces();
        Ok(peers::all(&state.db)?
            .into_iter()
            .filter_map(|peer| {
                let address = super::peer_address::address_with_interfaces(&peer, &interfaces);
                identity(&peer, &actor, &address.base_url).map(|identity| (peer.id, identity))
            })
            .collect())
    })
    .await?
}

async fn reconcile(
    state: &ServerState,
    workers: &mut HashMap<String, Worker>,
    wanted: HashMap<String, Identity>,
) {
    let removed: Vec<_> = workers
        .iter()
        .filter(|(id, worker)| {
            worker.job.is_finished()
                || wanted
                    .get(*id)
                    .is_none_or(|identity| identity != &worker.identity)
        })
        .map(|(id, _)| id.clone())
        .collect();
    for id in removed {
        if let Some(mut worker) = workers.remove(&id) {
            worker.cancel.send_replace(true);
            worker.job.abort();
            let _ = (&mut worker.job).await;
        }
    }
    for (id, identity) in wanted {
        workers.entry(id.clone()).or_insert_with(|| {
            let (cancel, cancelled) = watch::channel(false);
            let context = state.clone();
            let connection = identity.clone();
            let job = tokio::spawn(async move {
                run(context, id, connection, cancelled).await;
            });
            Worker {
                identity,
                cancel,
                job,
            }
        });
    }
}

async fn retire(workers: &mut HashMap<String, Worker>) {
    for (_, mut worker) in workers.drain() {
        worker.cancel.send_replace(true);
        worker.job.abort();
        let _ = (&mut worker.job).await;
    }
}

async fn publish(
    state: &ServerState,
    peer_id: &str,
    identity: &Identity,
    kind: String,
    data: serde_json::Value,
    cancelled: &watch::Receiver<bool>,
) -> Result<bool> {
    let state = state.clone();
    let peer_id = peer_id.to_owned();
    let identity = identity.clone();
    let cancelled = cancelled.clone();
    tokio::task::spawn_blocking(move || {
        state.db.with_conn(|connection| -> Result<bool> {
            let current = connection
                .query_row(
                    "SELECT token,ip,port FROM peers WHERE id=?1",
                    [&peer_id],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, Option<u16>>(2)?,
                        ))
                    },
                )
                .optional()?;
            if *cancelled.borrow()
                || *state.stop.borrow()
                || current.as_ref().is_none_or(|(token, ip, port)| {
                    token != &identity.token || ip != &identity.ip || port != &identity.port
                })
            {
                return Ok(false);
            }
            let _ = state.events.send(WsEvent::broadcast(
                plain_rs::events::PEER_MANAGEMENT_EVENT,
                json!({"peerId":peer_id,"type":kind,"data":data}).to_string(),
            ));
            Ok(true)
        })
    })
    .await?
}

fn next_delay(delay: Duration) -> Duration {
    (delay * 2).min(Duration::from_secs(30))
}

async fn run(
    state: ServerState,
    id: String,
    identity: Identity,
    mut cancelled: watch::Receiver<bool>,
) {
    let key = plain_rs::utils::base64::base64_decode(&identity.token);
    let mut stop = state.stop.clone();
    let mut delay = Duration::from_secs(1);
    loop {
        if *stop.borrow() || *cancelled.borrow() {
            break;
        }
        let connected = tokio::select! {
            biased;
            _ = stop.changed() => break,
            _ = cancelled.changed() => break,
            connected = tokio::time::timeout(Duration::from_secs(10), super::status_socket::connect_management(&identity.url)) => connected,
        };
        if let Ok(Ok(mut socket)) = connected {
            let publication_cancelled = cancelled.clone();
            let exchange = async {
                socket
                    .send(Message::Binary(
                        super::management_wire::registration(
                            &key,
                            chrono::Utc::now().timestamp_millis(),
                        )?
                        .into(),
                    ))
                    .await?;
                while let Some(message) = socket.next().await {
                    match message? {
                        Message::Binary(frame) => {
                            if let Some((kind, data)) = super::management_wire::event(&frame, &key)?
                            {
                                if !publish(
                                    &state,
                                    &id,
                                    &identity,
                                    kind,
                                    data,
                                    &publication_cancelled,
                                )
                                .await?
                                {
                                    break;
                                }
                                delay = Duration::from_secs(1);
                            }
                        }
                        Message::Ping(bytes) => socket.send(Message::Pong(bytes)).await?,
                        Message::Close(_) => break,
                        _ => {}
                    }
                }
                Ok::<_, anyhow::Error>(())
            };
            tokio::select! {
                biased;
                _ = stop.changed() => break,
                _ = cancelled.changed() => break,
                result = exchange => if let Err(error) = result { log::debug!("Management peer connection ended: {error}"); },
            }
        }
        tokio::select! {
            biased;
            _ = stop.changed() => break,
            _ = cancelled.changed() => break,
            _ = tokio::time::sleep(delay) => {},
        }
        delay = next_delay(delay);
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/services/peers/management.rs"]
mod tests;
