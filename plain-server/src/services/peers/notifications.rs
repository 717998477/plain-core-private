use super::management::{Identity, current_peer};
use super::metadata::AppSnapshot;
use crate::{
    assembly::server::ServerState, graphql::resolvers::notifications::types::Notification,
};
use anyhow::{Result, ensure};
use plain_rs::{chat::enums::DeviceType, db::DPeer, events::WsEvent};
use serde::Serialize;
use serde_json::json;
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    sync::Mutex,
};
use tokio::sync::{broadcast, watch};

const LIMIT: usize = 200;

#[derive(Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Item {
    #[serde(flatten)]
    notification: Notification,
    icon: String,
}

#[derive(Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct Group {
    peer_id: String,
    name: String,
    device_type: DeviceType,
    online: bool,
    loading: bool,
    loaded: bool,
    capabilities: Option<Vec<String>>,
    permissions: Vec<String>,
    items: Vec<Item>,
}

struct Peer {
    identity: Identity,
    group: Group,
    baseline: bool,
    url_token: String,
    icons: HashMap<String, String>,
}

#[derive(Default)]
struct State {
    revision: u64,
    peers: BTreeMap<String, Peer>,
}

#[derive(Default)]
pub(crate) struct Store(Mutex<State>);

fn allowed(app: &AppSnapshot) -> bool {
    app.capabilities
        .iter()
        .any(|value| value == "NOTIFICATIONS")
        && app
            .permissions
            .iter()
            .any(|value| value == "NOTIFICATION_LISTENER")
}

impl State {
    fn snapshot_event(&self) -> WsEvent {
        let groups: Vec<_> = self.peers.values().map(|peer| &peer.group).collect();
        WsEvent::broadcast(
            plain_rs::events::PEER_NOTIFICATIONS_UPDATED,
            json!({"revision":self.revision,"groups":groups}).to_string(),
        )
    }

    fn publish(
        &mut self,
        id: &str,
        alerts: Vec<Item>,
        permissions_changed: bool,
        events: &broadcast::Sender<WsEvent>,
    ) {
        self.revision += 1;
        let group = self.peers.get(id).map(|peer| &peer.group);
        let _ = events.send(WsEvent::broadcast(
            plain_rs::events::PEER_NOTIFICATIONS_UPDATED,
            json!({"revision":self.revision,"peerId":id,"group":group,"alerts":alerts,"permissionsChanged":permissions_changed}).to_string(),
        ));
    }
}

impl Store {
    pub(super) fn reconcile(
        &self,
        peers: &[DPeer],
        wanted: &HashMap<String, Identity>,
        events: &broadcast::Sender<WsEvent>,
    ) {
        let mut state = self.0.lock().unwrap();
        let removed: Vec<_> = state
            .peers
            .keys()
            .filter(|id| !wanted.contains_key(*id))
            .cloned()
            .collect();
        for id in removed {
            state.peers.remove(&id);
            state.publish(&id, vec![], false, events);
        }
        for peer in peers {
            let Some(identity) = wanted.get(&peer.id) else {
                continue;
            };
            if state
                .peers
                .get(&peer.id)
                .is_none_or(|old| old.identity.token != identity.token)
            {
                state.peers.insert(
                    peer.id.clone(),
                    Peer {
                        identity: identity.clone(),
                        group: Group {
                            peer_id: peer.id.clone(),
                            name: peer.name.clone(),
                            device_type: peer.device_type,
                            online: false,
                            loading: true,
                            loaded: false,
                            capabilities: None,
                            permissions: vec![],
                            items: vec![],
                        },
                        baseline: false,
                        url_token: String::new(),
                        icons: HashMap::new(),
                    },
                );
                state.publish(&peer.id, vec![], false, events);
            } else if let Some(old) = state.peers.get_mut(&peer.id) {
                if old.identity != *identity {
                    old.identity = identity.clone();
                    old.icons.clear();
                }
                if old.group.name != peer.name || old.group.device_type != peer.device_type {
                    old.group.name = peer.name.clone();
                    old.group.device_type = peer.device_type;
                    state.publish(&peer.id, vec![], false, events);
                }
            }
        }
    }

    pub(crate) fn snapshot_event(&self) -> WsEvent {
        self.0.lock().unwrap().snapshot_event()
    }

    pub(crate) fn subscribe(
        &self,
        events: &broadcast::Sender<WsEvent>,
    ) -> (broadcast::Receiver<WsEvent>, WsEvent) {
        let state = self.0.lock().unwrap();
        (events.subscribe(), state.snapshot_event())
    }

    fn apply(
        &self,
        id: &str,
        identity: &Identity,
        result: Result<(AppSnapshot, Vec<Notification>, reqwest::Url)>,
        events: &broadcast::Sender<WsEvent>,
    ) -> Result<()> {
        let mut state = self.0.lock().unwrap();
        let Some(peer) = state
            .peers
            .get_mut(id)
            .filter(|peer| peer.identity == *identity)
        else {
            return Ok(());
        };
        let before = peer.group.clone();
        let mut alerts = vec![];
        match result {
            Ok((app, items, base)) => {
                validate_snapshot(&app, &items)?;
                let is_allowed = allowed(&app);
                if peer.url_token != app.url_token {
                    peer.icons.clear();
                    peer.url_token = app.url_token;
                }
                let mut projected = Vec::with_capacity(items.len());
                if is_allowed {
                    let previous: HashSet<_> = peer
                        .group
                        .items
                        .iter()
                        .map(|item| item.notification.id.as_str().to_owned())
                        .collect();
                    for notification in items {
                        let icon = match peer.icons.get(notification.app_id.as_str()) {
                            Some(icon) => icon.clone(),
                            None => {
                                let file_id = plain_rs::chat::messages::content::make_file_id(
                                    &format!("pkgicon://{}", notification.app_id.as_str()),
                                    &peer.url_token,
                                );
                                ensure!(!file_id.is_empty(), "Invalid notification icon key");
                                let mut url = base.join("/fs")?;
                                url.query_pairs_mut().append_pair("id", &file_id);
                                let icon = url.to_string();
                                peer.icons
                                    .insert(notification.app_id.to_string(), icon.clone());
                                icon
                            }
                        };
                        let fresh = !previous.contains(notification.id.as_str());
                        let item = Item { notification, icon };
                        if peer.baseline && fresh && !item.notification.silent {
                            alerts.push(item.clone());
                        }
                        projected.push(item);
                    }
                }
                peer.baseline = is_allowed;
                peer.group.items = projected;
                peer.group.capabilities = Some(app.capabilities);
                peer.group.permissions = app.permissions;
                peer.group.online = true;
            }
            Err(error)
                if matches!(
                    error.downcast_ref::<super::management_http::AuthorizationError>(),
                    Some(super::management_http::AuthorizationError::Permission)
                ) =>
            {
                peer.group.items.clear();
                peer.group
                    .permissions
                    .retain(|name| name != "NOTIFICATION_LISTENER");
                peer.baseline = false;
                peer.group.online = true;
            }
            Err(error)
                if matches!(
                    error.downcast_ref::<super::management_http::AuthorizationError>(),
                    Some(super::management_http::AuthorizationError::Session)
                ) =>
            {
                peer.group.items.clear();
                peer.baseline = false;
                peer.group.online = false;
            }
            Err(error) => {
                log::debug!("Peer notification refresh failed: {error}");
                peer.group.online = false;
            }
        }
        peer.group.loading = false;
        peer.group.loaded = true;
        if peer.group != before || !alerts.is_empty() {
            let permissions_changed = peer.group.permissions != before.permissions;
            state.publish(id, alerts, permissions_changed, events);
        }
        Ok(())
    }
}

fn validate_snapshot(app: &AppSnapshot, items: &[Notification]) -> Result<()> {
    ensure!(items.len() <= LIMIT, "Invalid notification snapshot");
    let mut ids = HashSet::new();
    ensure!(
        items
            .iter()
            .all(|item| !item.id.as_str().is_empty() && ids.insert(item.id.as_str())),
        "Invalid notification identities"
    );
    ensure!(
        !allowed(app)
            || items.is_empty()
            || plain_rs::utils::base64::base64_decode(&app.url_token).len() == 32,
        "Invalid notification icon key"
    );
    Ok(())
}

pub(super) async fn refresh_from_app(
    state: &ServerState,
    id: &str,
    identity: &Identity,
    cancelled: &watch::Receiver<bool>,
    app: Result<AppSnapshot>,
) -> Result<()> {
    if !current_peer(state, id, identity, cancelled).await? {
        return Ok(());
    }
    let base = identity.http_url()?;
    let client_id = state.native_client_id().map_err(anyhow::Error::msg)?;
    let key = plain_rs::utils::base64::base64_decode(&identity.token);
    let result = async {
        let app = app?;
        let items: Vec<Notification> = if allowed(&app) {
            let data = super::management_http::query(state, &base, &client_id, &key,
                json!({"query":"{ notifications(query:\"\",offset:0,limit:200) { id onlyOnce isClearable appId appName postedAt silent title body actions replyActions } }"}), 4 * 1024 * 1024).await?;
            serde_json::from_value(data["notifications"].clone())?
        } else { vec![] };
        validate_snapshot(&app, &items)?;
        Ok((app, items, base))
    }.await;
    let state = state.clone();
    let id = id.to_owned();
    let identity = identity.clone();
    let cancelled = cancelled.clone();
    tokio::task::spawn_blocking(move || {
        state.db.with_conn(|connection| {
            if super::management::current(connection, &state, &id, &identity, &cancelled)? {
                state
                    .management
                    .notifications
                    .apply(&id, &identity, result, &state.events)?;
            }
            Ok(())
        })
    })
    .await?
}

#[cfg(test)]
#[path = "../../../tests/unit/services/peers/notifications.rs"]
mod tests;
