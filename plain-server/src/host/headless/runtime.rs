use std::sync::Arc;

use crate::assembly::server::ContentServer;
use crate::host::platform::{SYSTEM_APP_FACTS, SYSTEM_APP_UPDATE_FACTS, SYSTEM_DISKS_FACTS};
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
            "systemImageFileInfo" | "systemVideoFileInfo" | "systemAudioFileInfo" => {
                let path = params["path"]
                    .as_str()
                    .ok_or("Media path required")?
                    .to_owned();
                let kind = match method {
                    "systemImageFileInfo" => plain_rs::enums::DataType::Image,
                    "systemVideoFileInfo" => plain_rs::enums::DataType::Video,
                    _ => plain_rs::enums::DataType::Audio,
                };
                tokio::task::spawn_blocking(move || super::native::media::media_info(&path, kind))
                    .await
                    .map_err(|error| error.to_string())
            }
            crate::host::platform::SYSTEM_MOUNT_FACTS => {
                tokio::task::spawn_blocking(crate::services::files::mounts::facts)
                    .await
                    .map_err(|error| error.to_string())?
            }
            "systemPermissionFacts" => {
                let app = super::native::app::app_facts(
                    self.core.data_directory(),
                    self.capabilities.clone(),
                )?;
                crate::host::permission_facts::facts(&app.permissions, params)
            }
            "fileTaskAuthorize" => {
                let allowed = tokio::task::spawn_blocking(move || {
                    crate::host::file_resource::authorize_read(params)
                })
                .await
                .map_err(|error| error.to_string())??;
                Ok(Value::Bool(allowed))
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

            "discoveryFacts" => {
                let settings: plain_rs::prefs::user::UserSettings =
                    serde_json::from_str(&self.core.user_settings_json()?)
                        .map_err(|error| error.to_string())?;
                let facts = super::native::discovery::discovery_facts(
                    settings.device_name,
                    crate::platform::version::full_version(),
                )?;
                serde_json::to_value(facts).map_err(|error| error.to_string())
            }
            SYSTEM_APP_FACTS => serde_json::to_value(super::native::app::app_facts(
                self.core.data_directory(),
                self.capabilities.clone(),
            )?)
            .map_err(|error| error.to_string()),
            SYSTEM_DISKS_FACTS => {
                let facts = tokio::task::spawn_blocking(super::native::platform::disks)
                    .await
                    .map_err(|error| error.to_string())??;
                serde_json::to_value(facts).map_err(|error| error.to_string())
            }
            SYSTEM_APP_UPDATE_FACTS => {
                let facts = tokio::task::spawn_blocking(super::native::platform::app_update)
                    .await
                    .map_err(|error| error.to_string())??;
                serde_json::to_value(facts).map_err(|error| error.to_string())
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
