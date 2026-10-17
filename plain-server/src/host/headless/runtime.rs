use std::sync::Arc;

use crate::assembly::server::ContentServer;
use serde_json::Value;

#[cfg(target_os = "linux")]
mod linux;

pub struct Runtime {
    core: Arc<ContentServer>,
    capabilities: Vec<String>,
    multicast: std::sync::Mutex<crate::host::multicast::MulticastLeases>,
}

impl Runtime {
    pub fn new(core: Arc<ContentServer>, capabilities: Vec<String>) -> Self {
        Self {
            core,
            capabilities,
            multicast: Default::default(),
        }
    }

    async fn dispatch(&self, method: &str, params: Value) -> Result<Value, String> {
        #[cfg(target_os = "linux")]
        if linux::handles(method) {
            return linux::dispatch(&self.core, method, params).await;
        }
        match method {
            "systemCapabilities" => {
                serde_json::to_value(&self.capabilities).map_err(|error| error.to_string())
            }
            "systemPermissionFacts" => {
                crate::host::permission_facts::facts(&["WRITE_EXTERNAL_STORAGE".to_owned()], params)
            }
            method if crate::native::audio::handles(method) => {
                self.core.native_audio()?.handle(method, params).await
            }

            "mdnsMulticast" => {
                let result = self
                    .multicast
                    .lock()
                    .map_err(|_| "multicast lease state unavailable")?
                    .without_platform_lock(params)?;
                Ok(Value::Bool(result))
            }

            _ => Err(format!("Unsupported headless host method: {method}")),
        }
    }

    pub async fn run(self: Arc<Self>) -> Result<(), String> {
        let _multicast = crate::host::multicast::MulticastSession::new(&self.multicast);
        let session = Arc::new(self.core.host_events());
        let requests = self.clone();
        let result = session
            .serve(
                move |method, params| {
                    let requests = requests.clone();
                    async move { requests.dispatch(&method, params).await }
                },
                |event| {
                    let kind = event
                        .get("type")
                        .and_then(Value::as_str)
                        .ok_or("missing host event type")?;
                    let payload = event
                        .get("payload")
                        .and_then(Value::as_str)
                        .ok_or("missing host event payload")?;
                    if kind == plain_rs::events::HTTP_SERVER_FAILED {
                        return Err(format!("HTTP server failed: {payload}"));
                    }
                    log::info!("[shell] {kind}: {payload}");
                    Ok(())
                },
            )
            .await;
        result
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/host/headless/runtime.rs"]
mod tests;
