use crate::assembly::server::ServerState;
use crate::services::peers::status_outgoing;
use serde_json::json;

pub struct Controller {
    state: ServerState,
}

impl Controller {
    pub(crate) fn new(state: ServerState) -> Self {
        Self { state }
    }

    pub async fn execute(&self, action: Action) -> Result<(), String> {
        let mut stop = self.state.stop.clone();
        if *stop.borrow() {
            return Err("Core stopped".into());
        }
        tokio::select! {
            _ = stop.changed() => Err("Core stopped".to_string()),
            result = execute(&self.state, action) => result,
        }
    }
}

#[derive(async_graphql::Enum, Clone, Copy, PartialEq, Eq, Debug)]
#[graphql(name = "PeerStatusAction")]
pub enum Action {
    Start,
    Stop,
    Reconnect,
    EnsureAware,
}

fn emit(state: &ServerState, id: &str, online: bool) {
    if id.is_empty() {
        return;
    }
    let _ = state.events.send(plain_rs::events::WsEvent::broadcast(
        plain_rs::chat::events::WS_PEER_STATUS_UPDATED,
        json!({"id": id, "online": online}).to_string(),
    ));
    let _ = state.events.send(plain_rs::events::WsEvent::broadcast(
        "PEER_CONNECTIONS_UPDATED",
        json!({"id": id, "online": online}).to_string(),
    ));
}

async fn execute(state: &ServerState, request: Action) -> Result<(), String> {
    if matches!(request, Action::EnsureAware) {
        status_outgoing::ensure_aware(state)
            .await
            .map_err(|e| e.to_string())?;
        return Ok(());
    }
    let _control = state.peer_status.control.lock().await;
    match request {
        Action::Start => {
            if state
                .peer_status
                .public_active
                .load(std::sync::atomic::Ordering::SeqCst)
            {
                state.peer_status.outgoing.start(state.clone());
                Ok(())
            } else {
                Err("Public server inactive".to_string())
            }
        }
        Action::Stop => {
            state.peer_status.outgoing.stop().await;
            let previous = state.peer_status.connections.snapshot(&state.db);
            state.peer_status.connections.clear_hints();
            let current = state.peer_status.connections.snapshot(&state.db);
            if let Some(previous_ids) = previous["online"].as_array() {
                for id in previous_ids.iter().filter_map(|v| v.as_str()) {
                    let still_online = current["online"]
                        .as_array()
                        .is_some_and(|ids| ids.iter().any(|c| c.as_str() == Some(id)));
                    if !still_online {
                        emit(state, id, false);
                    }
                }
            }
            Ok(())
        }
        Action::Reconnect => {
            state
                .peer_status
                .outgoing
                .reconnect(state)
                .map_err(|e| e.to_string())?;
            Ok(())
        }
        Action::EnsureAware => unreachable!(),
    }
}
