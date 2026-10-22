use crate::graphql::resolvers::app::types::Capability;
use crate::graphql::resolvers::device::permissions::Permission;
use crate::graphql::types::Instant;
use crate::graphql::types::KeyValuePair;
use crate::graphql::types::Long;
use crate::graphql::types::facts::flag;
use crate::graphql::types::facts::text;
use crate::host::bridge::Host;
use async_graphql::{ComplexObject, Context, Enum, Object, SimpleObject};
use serde_json::{Value, json};
use std::sync::Arc;

#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug, Default)]
pub enum DeviceType {
    #[default]
    Computer,
    Phone,
    Tablet,
    Tv,
    Nas,
    Other,
}

#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug, Default)]
pub enum DevicePlatform {
    #[default]
    Android,
    Ios,
    Macos,
    Windows,
    Linux,
}

#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug, Default)]
pub enum AppChannelType {
    #[default]
    Github,
    Google,
    Fdroid,
}

#[derive(SimpleObject, Clone, Debug)]
pub struct DisplayInfo {
    pub width: i32,
    pub height: i32,
    pub density: f64,
}

#[derive(SimpleObject, Clone, Debug)]
pub struct AndroidExtras {
    #[graphql(name = "sdkVersion")]
    pub sdk_version: i32,
    #[graphql(name = "versionCodeName")]
    pub version_code_name: String,
    #[graphql(name = "securityPatch")]
    pub security_patch: String,
    pub bootloader: String,
    pub fingerprint: String,
    pub hardware: String,
    #[graphql(name = "radioVersion")]
    pub radio_version: String,
    pub board: String,
    #[graphql(name = "buildBrand")]
    pub build_brand: String,
    #[graphql(name = "buildNumber")]
    pub build_number: String,
    pub device: String,
    #[graphql(name = "javaVmVersion")]
    pub java_vm_version: String,
    #[graphql(name = "glEsVersion")]
    pub gl_es_version: String,
    #[graphql(name = "buildTime")]
    pub build_time: Instant,
}

#[derive(SimpleObject, Clone, Debug)]
pub struct DeviceInfo {
    pub name: String,
    pub platform: DevicePlatform,
    pub manufacturer: String,
    pub model: String,
    #[graphql(name = "osName")]
    pub os_name: String,
    #[graphql(name = "osVersion")]
    pub os_version: String,
    #[graphql(name = "kernelVersion")]
    pub kernel_version: String,
    #[graphql(name = "appVersion")]
    pub app_version: String,
    #[graphql(name = "appBuildNumber")]
    pub app_build_number: String,
    pub language: String,
    #[graphql(name = "cpuArch")]
    pub cpu_arch: String,
    #[graphql(name = "cpuModel")]
    pub cpu_model: Option<String>,
    #[graphql(name = "totalMemory")]
    pub total_memory: Long,
    #[graphql(name = "totalStorage")]
    pub total_storage: Long,
    pub display: Option<DisplayInfo>,
    pub android: Option<AndroidExtras>,
}

#[derive(SimpleObject, Clone, Debug)]
pub struct Temperature {
    pub label: String,
    pub celsius: f64,
}

#[derive(SimpleObject, Clone, Debug)]
pub struct DeviceStatus {
    #[graphql(name = "uptimeSec")]
    pub uptime_sec: Long,
    #[graphql(name = "batteryLevel")]
    pub battery_level: Option<i32>,
    pub charging: bool,
    pub temperatures: Vec<Temperature>,
    #[graphql(name = "cpuUsage")]
    pub cpu_usage: f64,
    #[graphql(name = "memoryAvailable")]
    pub memory_available: Option<Long>,
    #[graphql(name = "storageAvailable")]
    pub storage_available: Long,
}

#[derive(SimpleObject, Clone)]
#[graphql(complex)]
pub struct App {
    pub client_id: String,
    /// Explicit read-only exception: authenticated clients need this existing
    /// file-access key. Generic prefs never expose it or allow writing it.
    pub url_token: String,
    pub http_port: i32,
    pub https_port: i32,
    pub device_name: String,
    pub developer_mode: bool,
    #[graphql(skip)]
    capabilities_sample: Arc<tokio::sync::OnceCell<Result<Vec<String>, String>>>,
    #[graphql(skip)]
    permissions_sample: Arc<tokio::sync::OnceCell<Result<Vec<String>, String>>>,
}

fn startup_text(ctx: &Context<'_>, name: &str) -> async_graphql::Result<String> {
    ctx.data::<Arc<Host>>()?
        .startup_property(name)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| {
            async_graphql::Error::new(format!("Missing startup system property: {name}"))
        })
}

impl App {
    async fn granted_permissions(&self, ctx: &Context<'_>) -> async_graphql::Result<&Vec<String>> {
        self.permissions_sample
            .get_or_init(|| async {
                let names: Vec<&str> = <Permission as async_graphql::EnumType>::items()
                    .iter()
                    .map(|item| item.name)
                    .collect();
                let sample = host_call(ctx, "systemPermissionFacts", json!({"permissions":names}))
                    .await
                    .map_err(|error| error.message)?;
                let grants = sample["granted"]
                    .as_object()
                    .ok_or("Invalid permission sample")?;
                let mut granted = Vec::new();
                for name in names {
                    match grants.get(name).and_then(Value::as_bool) {
                        Some(true) => granted.push(name.to_owned()),
                        Some(false) => {}
                        None => return Err(format!("Missing permission sample: {name}")),
                    }
                }
                Ok(granted)
            })
            .await
            .as_ref()
            .map_err(|error| async_graphql::Error::new(error.clone()))
    }
}

#[ComplexObject]
impl App {
    async fn network(
        &self,
        ctx: &Context<'_>,
    ) -> async_graphql::Result<crate::platform::network::NetworkStatus> {
        crate::platform::network::device_snapshot(ctx.data::<Arc<Host>>()?)
            .await
            .map_err(async_graphql::Error::new)
    }

    async fn app_dir(&self, ctx: &Context<'_>) -> async_graphql::Result<String> {
        Ok(ctx
            .data::<crate::assembly::services::Services>()?
            .device
            .directory()
            .to_string_lossy()
            .into_owned())
    }
    async fn device_type(&self, ctx: &Context<'_>) -> async_graphql::Result<DeviceType> {
        Ok(device_type(&startup_text(ctx, "deviceType")?))
    }
    async fn capabilities(&self, ctx: &Context<'_>) -> async_graphql::Result<Vec<Capability>> {
        let mut names = self
            .capabilities_sample
            .get_or_init(|| async {
                crate::services::capabilities::snapshot(
                    ctx.data::<Arc<Host>>().map_err(|error| error.message)?,
                )
                .await
                .map_err(|error| error.to_string())
            })
            .await
            .as_ref()
            .map_err(|error| async_graphql::Error::new(error.clone()))?
            .clone();
        names.extend(
            crate::services::files::browser::capabilities()
                .into_iter()
                .map(str::to_owned),
        );
        let mut seen = std::collections::HashSet::new();
        Ok(names
            .into_iter()
            .filter(|name| seen.insert(name.clone()))
            .filter_map(|name| Capability::parse(&name))
            .collect())
    }
    async fn build_channel(&self, ctx: &Context<'_>) -> async_graphql::Result<AppChannelType> {
        Ok(app_channel(&startup_text(ctx, "buildChannel")?))
    }
    async fn permissions(&self, ctx: &Context<'_>) -> async_graphql::Result<Vec<Permission>> {
        let grants = self.granted_permissions(ctx).await?;
        let configured: Vec<String> = ctx
            .data::<Arc<plain_rs::prefs::Prefs>>()?
            .get_or(plain_rs::prefs::pref_keys::API_PERMISSIONS, Vec::new());
        let local = crate::http::auth::public_gate::is_local(ctx);
        let mut permissions: Vec<_> = grants
            .iter()
            .filter(|name| local || configured.contains(name))
            .filter_map(|name| Permission::parse(name))
            .collect();
        for (name, permission) in [
            ("RECORD_AUDIO", Permission::RecordAudio),
            ("ADB", Permission::Adb),
        ] {
            if grants.iter().any(|grant| grant == name) && !permissions.contains(&permission) {
                permissions.push(permission);
            }
        }
        Ok(permissions)
    }
    async fn downloads_dir(&self, ctx: &Context<'_>) -> async_graphql::Result<String> {
        startup_text(ctx, "downloadsDir")
    }
    async fn debug(&self, ctx: &Context<'_>) -> async_graphql::Result<bool> {
        ctx.data::<Arc<Host>>()?
            .startup_property("debug")
            .and_then(Value::as_bool)
            .ok_or_else(|| async_graphql::Error::new("Missing startup system property: debug"))
    }
}

impl DeviceType {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            DeviceType::Computer => "COMPUTER",
            DeviceType::Phone => "PHONE",
            DeviceType::Tablet => "TABLET",
            DeviceType::Tv => "TV",
            DeviceType::Nas => "NAS",
            DeviceType::Other => "OTHER",
        }
    }
}

#[derive(Default)]
pub struct DeviceQuery;

#[Object]
impl DeviceQuery {
    async fn device_status(&self, ctx: &Context<'_>) -> async_graphql::Result<DeviceStatus> {
        let fields = ctx.look_ahead();
        ctx.data::<crate::assembly::services::Services>()?
            .device
            .collect(
                ctx.data::<Arc<Host>>()?,
                crate::services::device::Selection {
                    uptime: fields.field("uptimeSec").exists(),
                    battery: fields.field("batteryLevel").exists()
                        || fields.field("charging").exists(),
                    temperatures: fields.field("temperatures").exists(),
                    cpu: fields.field("cpuUsage").exists(),
                    memory: fields.field("memoryAvailable").exists(),
                    storage: fields.field("storageAvailable").exists(),
                },
            )
            .await
            .map_err(async_graphql::Error::new)
    }

    async fn device_info(&self, ctx: &Context<'_>) -> async_graphql::Result<DeviceInfo> {
        let look = ctx.look_ahead();
        let mut fields: Vec<String> = crate::services::device::info::FIELDS
            .iter()
            .filter(|name| look.field(name).exists())
            .map(|name| (*name).to_owned())
            .collect();
        for (name, children) in [
            ("display", crate::services::device::info::DISPLAY_FIELDS),
            ("android", crate::services::device::info::ANDROID_FIELDS),
        ] {
            if look.field(name).exists() {
                fields.push(name.into());
            }
            for child in children {
                if look.field(name).field(child).exists() {
                    fields.push(format!("{name}.{child}"));
                }
            }
        }
        crate::services::device::info::collect(
            ctx.data::<Arc<Host>>()?,
            ctx.data::<crate::assembly::services::Services>()?
                .device
                .directory()
                .to_path_buf(),
            fields,
            ctx.data::<crate::assembly::services::Services>()?
                .device
                .fixed_info()
                .map_err(async_graphql::Error::new)?,
        )
        .await
        .map_err(async_graphql::Error::new)
    }

    /// Everything a client needs to reach this device: its identity, the
    /// port it listens on and the token that authorises it.
    async fn app(&self, ctx: &Context<'_>) -> async_graphql::Result<App> {
        let prefs = ctx.data::<Arc<plain_rs::prefs::Prefs>>()?;
        let user = plain_rs::prefs::settings::snapshot(prefs)?;
        let system = plain_rs::prefs::security::snapshot(prefs)?;
        let (http_port, https_port) = match ctx.data_opt::<crate::assembly::server::ServerState>() {
            Some(state) => state
                .public_ports()
                .await
                .map(|(http, https)| (i32::from(http), i32::from(https)))
                .unwrap_or((0, 0)),
            None => (user.http_port, user.https_port),
        };
        Ok(App {
            client_id: system.client_id,
            url_token: system.url_token,
            http_port,
            https_port,
            device_name: user.device_name,
            developer_mode: user.developer_mode,
            capabilities_sample: Arc::new(tokio::sync::OnceCell::new()),
            permissions_sample: Arc::new(tokio::sync::OnceCell::new()),
        })
    }

    /// Log lines, newest first. A blank `query` pages the buffer directly;
    /// a non-blank one filters the whole buffer and then pages, so a
    /// search cannot be cut short by the page size.
    async fn app_logs(
        &self,
        ctx: &Context<'_>,
        query: String,
        offset: i32,
        limit: i32,
    ) -> async_graphql::Result<Vec<String>> {
        ctx.data::<crate::assembly::services::Services>()?
            .logs
            .read(text_of(&query), offset, limit)
            .await
            .map_err(async_graphql::Error::new)
    }

    async fn app_log_files_size(&self, ctx: &Context<'_>) -> async_graphql::Result<Long> {
        let size = ctx
            .data::<crate::assembly::services::Services>()?
            .logs
            .size()
            .await
            .map_err(async_graphql::Error::new)?;
        Ok(Long(i64::try_from(size)?))
    }

    async fn app_log_path(&self, ctx: &Context<'_>) -> async_graphql::Result<String> {
        Ok(ctx
            .data::<crate::assembly::services::Services>()?
            .logs
            .path()
            .map_err(async_graphql::Error::new)?
            .to_string_lossy()
            .into_owned())
    }

    /// Whether the mDNS advertiser is currently scanning. Starting and
    /// stopping are commands, not state, so they report success rather
    /// than a snapshot the caller would have to re-read.
    async fn is_discovering(&self, ctx: &Context<'_>) -> async_graphql::Result<bool> {
        let facts = ctx
            .data::<crate::assembly::server::ServerState>()?
            .mdns
            .snapshot();
        Ok(flag(&facts, "scanning"))
    }
}

#[derive(Default)]
pub struct DeviceMutation;

#[Object]
impl DeviceMutation {
    async fn clear_app_logs(&self, ctx: &Context<'_>) -> async_graphql::Result<bool> {
        ctx.data::<crate::assembly::services::Services>()?
            .logs
            .clear()
            .await
            .map_err(async_graphql::Error::new)?;
        Ok(true)
    }

    async fn export_app_logs(&self, ctx: &Context<'_>) -> async_graphql::Result<Option<String>> {
        let state = ctx.data::<crate::assembly::server::ServerState>()?;
        ctx.data::<crate::assembly::services::Services>()?
            .logs
            .export(&state.directory, &state.cache, state.stop.clone())
            .await
            .map_err(async_graphql::Error::new)
    }

    async fn set_temp_value(
        &self,
        ctx: &Context<'_>,
        key: String,
        value: String,
    ) -> async_graphql::Result<KeyValuePair> {
        ctx.data::<crate::services::temp_values::TempValues>()?
            .set(key.clone(), value.clone());
        Ok(KeyValuePair { key, value })
    }

    /// The app restarts itself; the response only acknowledges that the
    /// request was accepted, since the connection dies with the process.
    async fn relaunch_app(&self, ctx: &Context<'_>) -> async_graphql::Result<bool> {
        crate::services::capabilities::require(ctx.data::<Arc<Host>>()?, "APP_RELAUNCH")
            .await
            .map_err(|error| async_graphql::Error::new(error.to_string()))?;
        host_call(ctx, "systemRelaunchApp", json!({})).await?;
        Ok(true)
    }

    /// Renaming the device also re-publishes the mDNS advertisement, so a
    /// client that waits for the new name to appear does see it.
    async fn update_device_name(
        &self,
        ctx: &Context<'_>,
        name: String,
    ) -> async_graphql::Result<bool> {
        crate::graphql::resolvers::settings::state::update(
            ctx,
            plain_rs::prefs::settings::SettingsPatch {
                device_name: Some(name),
                ..Default::default()
            },
        )
        .await?;
        Ok(true)
    }

    async fn start_discovery(&self, ctx: &Context<'_>) -> async_graphql::Result<bool> {
        crate::services::discovery::mdns_runtime::control(
            ctx.data::<crate::assembly::server::ServerState>()?,
            crate::services::discovery::mdns_runtime::Request::Start {},
        )
        .await?;
        Ok(true)
    }

    async fn stop_discovery(&self, ctx: &Context<'_>) -> async_graphql::Result<bool> {
        crate::services::discovery::mdns_runtime::control(
            ctx.data::<crate::assembly::server::ServerState>()?,
            crate::services::discovery::mdns_runtime::Request::Stop {},
        )
        .await?;
        Ok(true)
    }
}

/// The `text` field of the search DSL, which is what the log view filters
/// on. A query with other fields but no `text` still filters on nothing.
fn text_of(query: &str) -> String {
    plain_rs::utils::search_dsl::parse(query)
        .into_iter()
        .find(|field| field.name == "text")
        .map(|field| field.value)
        .unwrap_or_default()
}

pub(crate) fn device_type(value: &str) -> DeviceType {
    match value {
        "PHONE" => DeviceType::Phone,
        "TABLET" => DeviceType::Tablet,
        "TV" => DeviceType::Tv,
        "NAS" => DeviceType::Nas,
        "OTHER" => DeviceType::Other,
        _ => DeviceType::Computer,
    }
}

fn app_channel(value: &str) -> AppChannelType {
    match value {
        "GOOGLE" => AppChannelType::Google,
        "FDROID" => AppChannelType::Fdroid,
        _ => AppChannelType::Github,
    }
}

async fn host_call(ctx: &Context<'_>, method: &str, params: Value) -> async_graphql::Result<Value> {
    ctx.data_unchecked::<Arc<Host>>()
        .call(method, params)
        .await
        .map_err(async_graphql::Error::new)
}

#[cfg(test)]
#[path = "../../../../tests/unit/graphql/resolvers/device/public.rs"]
mod tests;
