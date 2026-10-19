use serde_json::{Value, json};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::{Notify, Semaphore, mpsc, oneshot, watch};

pub(crate) const REQUEST_CAPACITY: usize = 32;

type Reply = Result<Value, String>;
#[derive(Default)]
struct State {
    generation: u64,
    next_id: u64,
    peer_sockets: bool,
    sender: Option<mpsc::Sender<Value>>,
    pending: HashMap<u64, oneshot::Sender<Reply>>,
}
pub struct Host {
    state: Arc<Mutex<State>>,
    ready: Notify,
    capacity: Semaphore,
    connection: watch::Sender<u64>,
    properties: serde_json::Map<String, Value>,
}
impl Default for Host {
    fn default() -> Self {
        Self {
            state: Arc::new(Mutex::new(State::default())),
            ready: Notify::new(),
            capacity: Semaphore::new(REQUEST_CAPACITY),
            connection: watch::channel(0).0,
            properties: crate::platform::startup::properties(),
        }
    }
}
struct Pending {
    state: Arc<Mutex<State>>,
    id: u64,
}
impl Drop for Pending {
    fn drop(&mut self) {
        self.state.lock().unwrap().pending.remove(&self.id);
    }
}
fn is_startup_property(key: &str) -> bool {
    matches!(
        key,
        "cacheDir"
            | "name"
            | "deviceType"
            | "buildChannel"
            | "downloadsDir"
            | "debug"
            | "manufacturer"
            | "model"
            | "osName"
            | "osVersion"
            | "updateSource"
            | "supportedAbis"
            | "appVersion"
            | "appBuildNumber"
            | "cpuArch"
            | "socModel"
            | "wifiAwareSupported"
            | "files.recentCatalog"
            | "files.recentRoot"
            | "media.systemCatalog"
            | "android.sdkVersion"
            | "android.versionCodeName"
            | "android.securityPatch"
            | "android.bootloader"
            | "android.fingerprint"
            | "android.hardware"
            | "android.radioVersion"
            | "android.board"
            | "android.buildBrand"
            | "android.buildNumber"
            | "android.device"
            | "android.javaVmVersion"
            | "android.glEsVersion"
            | "android.buildTime"
    )
}
fn is_dynamic_property(key: &str) -> bool {
    matches!(
        key,
        "language" | "display.width" | "display.height" | "display.density" | "wifiAwareRunning"
    )
}
impl Host {
    pub fn with_startup_properties(
        properties: serde_json::Map<String, Value>,
    ) -> Result<Self, String> {
        for (key, value) in &properties {
            if !is_startup_property(key) {
                return Err(format!("System property is not fixed at startup: {key}"));
            }
            let valid = match key.as_str() {
                "cacheDir" => value
                    .as_str()
                    .is_some_and(|path| std::path::Path::new(path).is_absolute()),
                "debug" | "wifiAwareSupported" | "files.recentCatalog" | "media.systemCatalog" => {
                    value.is_boolean()
                }
                "socModel" | "files.recentRoot" => value.is_null() || value.is_string(),
                "supportedAbis" => value.as_array().is_some_and(|abis| {
                    abis.iter()
                        .all(|abi| abi.as_str().is_some_and(|abi| !abi.is_empty()))
                }),
                "updateSource" => {
                    value.is_null()
                        || serde_json::from_value::<plain_rs::updates::Source>(value.clone())
                            .is_ok()
                }
                "android.sdkVersion" => value
                    .as_i64()
                    .is_some_and(|number| number > 0 && number <= i32::MAX as i64),
                "android.buildTime" => value.as_i64().is_some(),
                "deviceType" => value.as_str().is_some_and(|name| {
                    matches!(
                        name,
                        "COMPUTER" | "PHONE" | "TABLET" | "TV" | "NAS" | "OTHER"
                    )
                }),
                "buildChannel" => value
                    .as_str()
                    .is_some_and(|name| matches!(name, "GITHUB" | "GOOGLE" | "FDROID")),
                _ => value.is_string(),
            };
            if !valid {
                return Err(format!("Invalid startup system property: {key}"));
            }
        }
        let mut host = Self::default();
        host.properties.extend(properties);
        Ok(host)
    }
    pub(crate) fn startup_property(&self, name: &str) -> Option<&Value> {
        self.properties.get(name)
    }

    pub(crate) fn uses_system_media_catalog(&self) -> bool {
        self.startup_property("media.systemCatalog")
            .and_then(Value::as_bool)
            .unwrap_or(false)
    }
    pub(crate) fn needs_socket_cleanup(&self) -> bool {
        self.state.lock().unwrap().peer_sockets
    }
    pub(crate) fn connection_changes(&self) -> watch::Receiver<u64> {
        self.connection.subscribe()
    }
    pub(crate) fn generation(&self) -> Option<u64> {
        let state = self.state.lock().unwrap();
        state.sender.as_ref().map(|_| state.generation)
    }
    pub(crate) fn connected(&self) -> bool {
        self.state.lock().unwrap().sender.is_some()
    }
    pub fn connect(&self) -> (u64, mpsc::Receiver<Value>) {
        let (sender, receiver) = mpsc::channel(REQUEST_CAPACITY);
        let mut state = self.state.lock().unwrap();
        for (_, reply) in state.pending.drain() {
            let _ = reply.send(Err("host connection replaced".into()));
        }
        state.generation += 1;
        state.sender = Some(sender);
        state.peer_sockets = false;
        let generation = state.generation;
        self.connection.send_replace(generation);
        drop(state);
        self.ready.notify_waiters();
        (generation, receiver)
    }
    pub fn is_current(&self, generation: u64) -> bool {
        let state = self.state.lock().unwrap();
        state.generation == generation && state.sender.is_some()
    }
    pub fn disconnect(&self, generation: u64) {
        let mut state = self.state.lock().unwrap();
        if state.generation != generation {
            return;
        }
        state.sender = None;
        self.connection.send_replace(generation);
        for (_, reply) in state.pending.drain() {
            let _ = reply.send(Err("host disconnected".into()));
        }
    }
    pub fn reply(&self, generation: u64, message: Value) -> Result<(), String> {
        let id = message
            .get("id")
            .and_then(Value::as_u64)
            .ok_or("invalid host reply ID")?;
        let mut state = self.state.lock().unwrap();
        if state.generation != generation {
            return Ok(());
        }
        if let Some(reply) = state.pending.remove(&id) {
            let result = match message.get("error") {
                Some(Value::String(error)) => Err(error.clone()),
                Some(_) => Err("invalid host error".into()),
                None => message
                    .get("result")
                    .cloned()
                    .ok_or_else(|| "missing host result".into()),
            };
            let _ = reply.send(result);
        }
        Ok(())
    }
    pub async fn call(&self, method: &str, params: Value) -> Reply {
        self.call_inner(method, params, Some(Duration::from_secs(25)))
            .await
    }
    pub(crate) async fn call_wait(&self, method: &str, params: Value) -> Reply {
        self.call_inner(method, params, None).await
    }
    pub(crate) async fn call_current(&self, generation: u64, method: &str, params: Value) -> Reply {
        self.request_inner(
            method,
            params,
            Some(Duration::from_secs(25)),
            Some(generation),
        )
        .await
    }
    async fn call_inner(&self, method: &str, params: Value, timeout: Option<Duration>) -> Reply {
        if method == "systemProperties" {
            let names = params["names"]
                .as_array()
                .ok_or("System property names required")?;
            let mut values = serde_json::Map::new();
            let mut dynamic = Vec::new();
            for name in names {
                let name = name.as_str().ok_or("Invalid system property name")?;
                if let Some(value) = self.properties.get(name) {
                    values.insert(name.to_owned(), value.clone());
                } else if is_startup_property(name) {
                    return Err(format!("Missing startup system property: {name}"));
                } else if is_dynamic_property(name) {
                    if !dynamic.iter().any(|existing| existing == name) {
                        dynamic.push(name.to_owned());
                    }
                } else {
                    return Err(format!("Unsupported system property: {name}"));
                }
            }
            if !dynamic.is_empty() {
                let mut params = params;
                params["names"] = json!(dynamic);
                let sampled = self.request(method, params, timeout).await?;
                let sampled = sampled
                    .as_object()
                    .ok_or("Invalid system property sample")?;
                for name in dynamic {
                    values.insert(
                        name.clone(),
                        sampled
                            .get(&name)
                            .ok_or_else(|| format!("Missing system property: {name}"))?
                            .clone(),
                    );
                }
            }
            return Ok(Value::Object(values));
        }
        self.request(method, params, timeout).await
    }
    async fn request(&self, method: &str, params: Value, timeout: Option<Duration>) -> Reply {
        self.request_inner(method, params, timeout, None).await
    }
    async fn request_inner(
        &self,
        method: &str,
        params: Value,
        timeout: Option<Duration>,
        generation: Option<u64>,
    ) -> Reply {
        let _permit = self
            .capacity
            .try_acquire()
            .map_err(|_| "host request capacity exceeded")?;
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        let (id, receiver) = loop {
            let notified = self.ready.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            let request = {
                let mut state = self.state.lock().unwrap();
                if generation.is_some_and(|generation| {
                    state.generation != generation || state.sender.is_none()
                }) {
                    return Err("host connection replaced".into());
                }
                if let Some(sender) = state.sender.clone() {
                    if method == "peerTransportSocketOpen" {
                        state.peer_sockets = true;
                    }
                    state.next_id += 1;
                    let id = state.next_id;
                    let (reply, receiver) = oneshot::channel();
                    state.pending.insert(id, reply);
                    if sender
                        .try_send(json!({"id":id,"method":method,"params":params}))
                        .is_err()
                    {
                        state.pending.remove(&id);
                        return Err("host request queue unavailable".into());
                    }
                    Some((id, receiver))
                } else {
                    None
                }
            };
            if let Some(request) = request {
                break request;
            }
            tokio::time::timeout_at(deadline, notified)
                .await
                .map_err(|_| "host unavailable")?;
        };
        let _pending = Pending {
            state: self.state.clone(),
            id,
        };
        match timeout {
            Some(timeout) => tokio::time::timeout(timeout, receiver)
                .await
                .map_err(|_| "host request timed out")?,
            None => receiver.await,
        }
        .map_err(|_| "host reply canceled")?
    }
}

#[cfg(test)]
#[path = "../../tests/unit/host/bridge.rs"]
mod tests;
