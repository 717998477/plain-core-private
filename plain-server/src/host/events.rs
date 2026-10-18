use crate::assembly::server::ServerState;
use plain_rs::events::{HOST_BINARY_TYPES, HOST_TEXT_TYPES, WsEvent};
use serde_json::{Value, json};
use tokio::sync::{Mutex, mpsc};

pub struct HostEvents {
    state: ServerState,
    generation: u64,
    requests: Mutex<mpsc::Receiver<Value>>,
    events: Mutex<crate::native::events::NativeEventStream>,
}
struct ServingSession<'a>(&'a HostEvents);

impl Drop for ServingSession<'_> {
    fn drop(&mut self) {
        self.0.state.ble_advertising.close(self.0.generation);
        self.0.state.ble_scans.close_owner(self.0.generation);
        self.0.state.host.disconnect(self.0.generation);
    }
}

impl HostEvents {
    pub(crate) fn new(state: ServerState) -> Self {
        let events = crate::native::events::NativeEventStream::new(state.clone(), None);
        let (generation, requests) = state.host.connect();
        state.ble_scans.set_owner(generation);
        state.ble_advertising.connect(generation);
        crate::services::settings::update_download::Runtime::schedule_reconcile(&state);
        let connected = state.clone();
        tokio::spawn(async move {
            tokio::join!(
                crate::services::discovery::mdns_runtime::Runtime::host_connected(
                    connected.clone(),
                    generation
                ),
                crate::services::casting::receiver::lifecycle::host_connected(
                    connected, generation
                ),
            );
        });
        Self {
            state,
            generation,
            requests: Mutex::new(requests),
            events: Mutex::new(events),
        }
    }
    pub fn capabilities(&self) -> Value {
        json!({"textTypes":HOST_TEXT_TYPES,"binaryTypes":HOST_BINARY_TYPES})
    }
    pub async fn wait_disconnected(&self) {
        let mut stop = self.state.stop.clone();
        let mut connection = self.state.host.connection_changes();
        if *stop.borrow() || !self.state.host.is_current(self.generation) {
            return;
        }
        tokio::select! {
            _ = stop.changed() => {},
            _ = connection.changed() => {},
        }
    }
    pub async fn next_request(&self) -> Option<Value> {
        let mut requests = self.requests.lock().await;
        tokio::select! {
            biased;
            _ = self.wait_disconnected() => None,
            value = requests.recv() => value,
        }
    }
    pub async fn next_event(&self) -> Option<Value> {
        let mut events = self.events.lock().await;
        loop {
            let delivery = tokio::select! {
                biased;
                _ = self.wait_disconnected() => return None,
                delivery = events.next_delivery() => delivery?,
            };
            if delivery.event.binary_payload.is_none() {
                return Some(delivery.into_json());
            }
        }
    }
    pub async fn serve<Requests, Events, Pending>(
        self: std::sync::Arc<Self>,
        requests: Requests,
        events: Events,
    ) -> Result<(), String>
    where
        Requests: Fn(String, Value) -> Pending,
        Pending: std::future::Future<Output = Result<Value, String>> + Send + 'static,
        Events: Fn(Value) -> Result<(), String>,
    {
        let _session = ServingSession(&self);
        self.serve_connected(requests, events).await
    }

    async fn serve_connected<Requests, Events, Pending>(
        &self,
        requests: Requests,
        events: Events,
    ) -> Result<(), String>
    where
        Requests: Fn(String, Value) -> Pending,
        Pending: std::future::Future<Output = Result<Value, String>> + Send + 'static,
        Events: Fn(Value) -> Result<(), String>,
    {
        let mut workers = tokio::task::JoinSet::new();
        loop {
            tokio::select! {
                biased;
                _ = self.wait_disconnected() => break,
                completed = workers.join_next(), if !workers.is_empty() => {
                    let (id, result) = completed
                        .ok_or("host worker unavailable")?
                        .map_err(|error| format!("host worker failed: {error}"))?;
                    let reply = match result {
                        Ok(result) => json!({"id": id, "result": result}),
                        Err(error) => json!({"id": id, "error": error}),
                    };
                    self.reply(reply)?;
                },
                event = self.next_event() => {
                    match event {
                        Some(event) => events(event)?,
                        None => break,
                    }
                },
                request = self.next_request(), if workers.len() < crate::host::bridge::REQUEST_CAPACITY => {
                    let Some(request) = request else { break };
                    let id = request.get("id").and_then(Value::as_u64)
                        .ok_or("invalid host request ID")?;
                    let method = request.get("method").and_then(Value::as_str)
                        .ok_or("invalid host request method")?.to_owned();
                    let params = request.get("params").cloned()
                        .ok_or("missing host request parameters")?;
                    let pending = requests(method, params);
                    workers.spawn(async move { (id, pending.await) });
                },
            }
        }
        workers.abort_all();
        while workers.join_next().await.is_some() {}
        Ok(())
    }

    pub fn reply(&self, value: Value) -> Result<(), String> {
        self.state.host.reply(self.generation, value)
    }
    pub fn attach_resource(
        &self,
        id: &str,
    ) -> Result<crate::native::resource_stream::NativeResourceStream, String> {
        self.state.bridge.attach_native(id, self.generation)
    }

    pub fn submit_thumbnail(&self, token: &str, bytes: Vec<u8>) -> Result<(), String> {
        if !self.state.host.is_current(self.generation) {
            return Err("host disconnected".into());
        }
        self.state.thumbnails.submit(token, bytes)
    }

    pub async fn publish_text(&self, kind: &str, payload: String) -> Result<(), String> {
        if !self.state.host.is_current(self.generation) {
            return Err("host disconnected".into());
        }
        if crate::services::discovery::pairing_runtime::accept_report(&self.state, kind, &payload)
            .await
            .map_err(|error| error.to_string())?
        {
            return Ok(());
        }
        if kind == "BACKGROUND_EXECUTION_GRANTED" {
            return crate::services::feeds::sync_service::schedule::grant(
                &self.state,
                serde_json::from_str(&payload).map_err(|error| error.to_string())?,
            )
            .map_err(|error| error.to_string());
        }
        if kind == "BACKGROUND_EXECUTION_REVOKED" {
            #[derive(serde::Deserialize)]
            #[serde(deny_unknown_fields)]
            struct Revoke {
                id: String,
            }
            let value: Revoke =
                serde_json::from_str(&payload).map_err(|error| error.to_string())?;
            crate::services::feeds::sync_service::schedule::revoke(&self.state, &value.id).await;
            return Ok(());
        }
        if kind == "CLIPBOARD_REPORTED" {
            return crate::services::clipboard::report(&self.state, &payload);
        }
        if kind == "NOTIFICATION_REPORTED" {
            return crate::graphql::resolvers::notifications::public::publish(
                &self.state.prefs,
                &self.state.events,
                serde_json::from_str(&payload).map_err(|error| error.to_string())?,
            );
        }
        if kind == "BLE_ADVERTISING_FACTS_REPORTED" {
            let facts = serde_json::from_str(&payload).map_err(|error| error.to_string())?;
            return self
                .state
                .ble_advertising
                .report(&self.state, self.generation, facts)
                .await
                .map_err(|error| error.to_string());
        }
        if kind == "BLE_SCAN_STATE_REPORTED" {
            #[derive(serde::Deserialize)]
            #[serde(rename_all = "camelCase", deny_unknown_fields)]
            struct ScanState {
                scan_id: String,
                active: bool,
            }
            let value: ScanState =
                serde_json::from_str(&payload).map_err(|error| error.to_string())?;
            let id = uuid::Uuid::parse_str(&value.scan_id).map_err(|error| error.to_string())?;
            return self
                .state
                .ble_scans
                .report_state(self.generation, id, value.active)
                .map_err(|error| error.to_string());
        }
        if kind == "BLE_ADVERTISEMENT_REPORTED" {
            #[derive(serde::Deserialize)]
            #[serde(rename_all = "camelCase", deny_unknown_fields)]
            struct Advertisement {
                scan_id: String,
                short_id: String,
            }
            let value: Advertisement =
                serde_json::from_str(&payload).map_err(|error| error.to_string())?;
            crate::services::discovery::ble_scan::seen(
                &self.state,
                &value.scan_id,
                &value.short_id,
            )
            .await
            .map_err(|error| error.to_string())?;
            return Ok(());
        }
        if kind == "SMS_PART_SENT_REPORTED" {
            self.state
                .sms_sends
                .report(serde_json::from_str(&payload).map_err(|error| error.to_string())?)
                .map_err(|error| error.to_string())?;
            return Ok(());
        }
        if kind == "NETWORK_INTERFACES_CHANGED" {
            serde_json::from_str::<()>(&payload).map_err(|error| error.to_string())?;
            self.state.mdns.network_changed(self.state.clone());
            let _ = self.state.events.send(WsEvent::broadcast(
                plain_rs::events::NETWORK_CHANGED,
                "{}".into(),
            ));
            return Ok(());
        }
        if crate::services::casting::receiver::reports::accept(
            &self.state,
            self.generation,
            kind,
            &payload,
        )
        .await?
        {
            return Ok(());
        }
        if kind == "IMAGE_CATALOG_CHANGED" {
            serde_json::from_str::<()>(&payload).map_err(|error| error.to_string())?;
            self.state.image_models.catalog_changed();
            return Ok(());
        }
        if kind == "PERMISSIONS_UPDATED" {
            self.state.mdns.permissions_changed(self.state.clone());
            if let Err(error) = self
                .state
                .ble_advertising
                .reconcile(&self.state, self.generation)
                .await
            {
                log::warn!("BLE advertising permission recovery failed: {error}");
            }
        }
        if kind == "MEDIA_CATALOG_CHANGED" {
            return crate::services::files::catalog_events::report(&self.state.events, &payload);
        }
        if crate::services::audio::reports::accept(&self.state, self.generation, kind, &payload)
            .await?
        {
            return Ok(());
        }
        self.publish(WsEvent::host_text(kind, payload))
    }
    pub fn publish_binary(&self, kind: &str, payload: Vec<u8>) -> Result<(), String> {
        self.publish(WsEvent::host_binary(kind, payload))
    }
    fn publish(&self, event: Option<WsEvent>) -> Result<(), String> {
        if !self.state.host.is_current(self.generation) {
            return Err("host disconnected".into());
        }
        let event = event.ok_or("host event type not allowed")?;
        let _ = self.state.events.send(event);
        Ok(())
    }
}
impl Drop for HostEvents {
    fn drop(&mut self) {
        self.state.ble_advertising.close(self.generation);
        self.state.ble_scans.close_owner(self.generation);
        self.state.host.disconnect(self.generation);
    }
}

#[cfg(test)]
#[path = "../../tests/unit/host/events.rs"]
mod tests;

#[cfg(test)]
pub(crate) struct TestHost(HostEvents);
#[cfg(test)]
impl TestHost {
    pub(crate) fn new(server: &crate::assembly::server::ContentServer) -> Self {
        Self(server.host_events())
    }
    pub(crate) async fn next(
        &mut self,
    ) -> Option<Result<tokio_tungstenite::tungstenite::Message, String>> {
        self.0.next_request().await.map(|v| {
            Ok(tokio_tungstenite::tungstenite::Message::Text(
                v.to_string().into(),
            ))
        })
    }
    pub(crate) async fn send(
        &mut self,
        value: tokio_tungstenite::tungstenite::Message,
    ) -> Result<(), String> {
        self.0.reply(
            serde_json::from_str(value.to_text().map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?,
        )
    }
}

#[cfg(test)]
#[path = "../../tests/unit/host/events_runtime.rs"]
mod runtime_tests;
