use crate::assembly::server::ServerState;
use plain_rs::events::WsEvent;
use serde_json::{Value, json};
use tokio::sync::broadcast;

pub struct NativeEventStream {
    state: ServerState,
    events: broadcast::Receiver<WsEvent>,
    client_id: Option<String>,
    next_snapshot: usize,
    notification_snapshot: Option<WsEvent>,
    scope: Option<super::graphql::NativeRequestScope>,
    frame_key: Option<[u8; 32]>,
}

pub(crate) struct Delivery {
    pub event: WsEvent,
    pub reconcile: bool,
}

#[derive(Clone, Copy)]
enum Snapshot {
    HttpServer,
    Images,
    OnlineClients,
    PeerTransport,
    Casting,
    UpdateDownload,
    Audio,
    PeerNotifications,
}

const SNAPSHOTS: &[Snapshot] = &[
    Snapshot::HttpServer,
    Snapshot::Images,
    Snapshot::OnlineClients,
    Snapshot::PeerTransport,
    Snapshot::Casting,
    Snapshot::UpdateDownload,
    Snapshot::Audio,
    Snapshot::PeerNotifications,
];

impl Delivery {
    pub(crate) fn into_json(self) -> Value {
        let event = self.event;
        if self.reconcile {
            json!({"type":event.event_type,"payload":event.payload,"reconcile":true})
        } else {
            json!({"type":event.event_type,"payload":event.payload})
        }
    }
}

impl NativeEventStream {
    pub(crate) fn new(state: ServerState, client_id: Option<String>) -> Self {
        let (events, notification_snapshot) =
            state.management.notifications.subscribe(&state.events);
        Self {
            state,
            events,
            client_id,
            next_snapshot: 0,
            notification_snapshot: Some(notification_snapshot),
            scope: None,
            frame_key: None,
        }
    }

    pub(crate) fn scoped(
        state: ServerState,
        client_id: String,
        scope: super::graphql::NativeRequestScope,
    ) -> Result<Self, String> {
        let key = plain_rs::utils::base64::base64_decode(&state.native_url_token()?);
        let key: [u8; 32] = key
            .try_into()
            .map_err(|_| "local event token unavailable".to_owned())?;
        let mut stream = Self::new(state, Some(client_id));
        stream.scope = Some(scope);
        stream.frame_key = Some(key);
        Ok(stream)
    }

    pub async fn recv(&mut self) -> Result<WsEvent, broadcast::error::RecvError> {
        self.next_delivery()
            .await
            .map(|delivery| delivery.event)
            .ok_or(broadcast::error::RecvError::Closed)
    }

    pub async fn next_frame(&mut self) -> Result<Option<Vec<u8>>, String> {
        let key = self.frame_key.ok_or("native frame encoding unavailable")?;
        let Some(event) = self.next_delivery().await.map(|delivery| delivery.event) else {
            return Ok(None);
        };
        let frame = match event.binary_payload {
            Some(payload) => plain_rs::protocols::websocket::encode_raw(event.event_type, &payload),
            None => plain_rs::protocols::websocket::encode(
                event.event_type,
                event.payload.as_bytes(),
                &key,
            ),
        };
        frame
            .map(Some)
            .ok_or_else(|| "invalid native event frame".into())
    }

    pub(crate) async fn next_delivery(&mut self) -> Option<Delivery> {
        let mut stop = self.state.stop.clone();
        let scope = self.scope.clone();
        let closed = async move {
            match scope {
                Some(scope) => scope.wait_closed().await,
                None => std::future::pending::<()>().await,
            }
        };
        tokio::pin!(closed);
        loop {
            if *stop.borrow() {
                return None;
            }
            if let Some(snapshot) = SNAPSHOTS.get(self.next_snapshot).copied() {
                if matches!(snapshot, Snapshot::HttpServer) && self.client_id.is_some() {
                    self.next_snapshot += 1;
                    continue;
                }
                let pending_snapshot = async {
                    match snapshot {
                        Snapshot::HttpServer => self.state.http_server.status().event(),
                        Snapshot::Images => self.state.image_models.snapshot().event(),
                        Snapshot::OnlineClients => self.state.main_ws.snapshot_event(),
                        Snapshot::PeerTransport => {
                            crate::services::peers::peer_transport::snapshot_event(&self.state)
                        }
                        Snapshot::Casting => self.state.cast.snapshot().event(),
                        Snapshot::UpdateDownload => {
                            self.state.update_checks.download.snapshot_event()
                        }
                        Snapshot::PeerNotifications => self
                            .notification_snapshot
                            .as_ref()
                            .expect("notification snapshot pending")
                            .clone(),
                        Snapshot::Audio => self.state.audio.playback_event().await,
                    }
                };
                let event = tokio::select! {
                    biased;
                    _ = stop.wait_for(|stopped| *stopped) => return None,
                    _ = &mut closed => return None,
                    event = pending_snapshot => event,
                };
                if *stop.borrow() {
                    return None;
                }
                self.next_snapshot += 1;
                if matches!(snapshot, Snapshot::PeerNotifications) {
                    self.notification_snapshot = None;
                }
                return Some(Delivery {
                    event,
                    reconcile: false,
                });
            }
            let received = tokio::select! {
                biased;
                _ = stop.wait_for(|stopped| *stopped) => return None,
                _ = &mut closed => return None,
                value = self.events.recv() => value,
            };
            match received {
                Ok(event) if event.is_host_event() => continue,
                Ok(event)
                    if event.target_cid.as_ref().is_some_and(|target| {
                        self.client_id
                            .as_ref()
                            .is_some_and(|client| client != target)
                    }) =>
                {
                    continue;
                }
                Ok(event) => {
                    if event.event_type == plain_rs::events::HTTP_SERVER_STATE_CHANGED
                        && self.client_id.is_some()
                    {
                        continue;
                    }
                    let event = match event.event_type {
                        plain_rs::events::HTTP_SERVER_STARTED
                        | plain_rs::events::HTTP_SERVER_FAILED
                        | plain_rs::events::HTTP_SERVER_STATE_CHANGED
                            if self.client_id.is_none() =>
                        {
                            self.state.http_server.status().event()
                        }
                        "ONLINE_CLIENTS_UPDATED" => self.state.main_ws.snapshot_event(),
                        "PEER_TRANSPORT_UPDATED" => {
                            crate::services::peers::peer_transport::snapshot_event(&self.state)
                        }
                        "APP_UPDATE_DOWNLOAD_UPDATED" => {
                            self.state.update_checks.download.snapshot_event()
                        }
                        _ => event,
                    };
                    return Some(Delivery {
                        event,
                        reconcile: false,
                    });
                }
                Err(broadcast::error::RecvError::Lagged(_)) => {
                    self.next_snapshot = usize::from(self.client_id.is_none());
                    self.notification_snapshot =
                        Some(self.state.management.notifications.snapshot_event());
                    if self.client_id.is_none() {
                        return Some(Delivery {
                            event: self.state.http_server.status().event(),
                            reconcile: true,
                        });
                    }
                    return Some(Delivery {
                        event: WsEvent::broadcast("CONTENT_CHANGED", "{}".into()),
                        reconcile: true,
                    });
                }
                Err(broadcast::error::RecvError::Closed) => return None,
            }
        }
    }
}

#[cfg(test)]
#[path = "../../tests/unit/native/events.rs"]
mod tests;
