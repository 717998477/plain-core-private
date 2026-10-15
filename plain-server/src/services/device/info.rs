use crate::graphql::resolvers::device::public::{
    AndroidExtras, DeviceInfo, DevicePlatform, DisplayInfo,
};
use crate::graphql::types::{Instant, Long};
use crate::host::bridge::Host;
use serde_json::{Map, Value};

pub(crate) const FIELDS: &[&str] = &[
    "name",
    "platform",
    "manufacturer",
    "model",
    "osName",
    "osVersion",
    "kernelVersion",
    "appVersion",
    "appBuildNumber",
    "language",
    "cpuArch",
    "cpuModel",
    "totalMemory",
    "totalStorage",
];
pub(crate) const DISPLAY_FIELDS: &[&str] = &["width", "height", "density"];
pub(crate) const ANDROID_FIELDS: &[&str] = &[
    "sdkVersion",
    "versionCodeName",
    "securityPatch",
    "bootloader",
    "fingerprint",
    "hardware",
    "radioVersion",
    "board",
    "buildBrand",
    "buildNumber",
    "device",
    "javaVmVersion",
    "glEsVersion",
    "buildTime",
];

pub(crate) fn sdk_properties(fields: &[String]) -> Vec<String> {
    fields
        .iter()
        .filter_map(|field| {
            #[cfg(target_os = "android")]
            match field.as_str() {
                "name" | "manufacturer" | "model" | "osVersion" | "appVersion"
                | "appBuildNumber" | "language" | "cpuArch" => return Some(field.clone()),
                "cpuModel" => return Some("socModel".into()),
                name if name.starts_with("display.") || name.starts_with("android.") => {
                    return Some(field.clone());
                }
                _ => {}
            }
            #[cfg(target_os = "ios")]
            match field.as_str() {
                "name" | "osName" | "osVersion" | "appVersion" | "appBuildNumber" | "language" => {
                    return Some(field.clone());
                }
                name if name.starts_with("display.") => return Some(field.clone()),
                _ => {}
            }
            let _ = field;
            None
        })
        .collect()
}

pub(crate) async fn collect(
    host: &Host,
    directory: std::path::PathBuf,
    fields: Vec<String>,
) -> Result<DeviceInfo, String> {
    let names = sdk_properties(&fields);
    let display = cfg!(any(target_os = "android", target_os = "ios"))
        && fields.iter().any(|name| name == "display");
    let android = cfg!(target_os = "android") && fields.iter().any(|name| name == "android");
    let sdk = read_properties(host, names);
    let direct = tokio::task::spawn_blocking(move || direct(&fields, &directory));
    let (sdk, direct) = tokio::join!(sdk, direct);
    apply_sdk(
        direct.map_err(|error| error.to_string())??,
        sdk?,
        display,
        android,
    )
}

pub(crate) async fn default_device_name(host: &Host) -> Result<String, String> {
    if cfg!(any(target_os = "android", target_os = "ios")) {
        let values = read_properties(host, vec!["name".into()]).await?;
        return values
            .get("name")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
            .ok_or("Missing device name".into());
    }
    Ok(plain_rs::prefs::identity::default_device_name())
}

async fn read_properties(host: &Host, names: Vec<String>) -> Result<Map<String, Value>, String> {
    if names.is_empty() {
        return Ok(Map::new());
    }
    let values = host
        .call("systemProperties", serde_json::json!({"names":names}))
        .await?;
    let values = values.as_object().ok_or("invalid system properties")?;
    for name in &names {
        let value = values
            .get(name)
            .ok_or_else(|| format!("missing system property: {name}"))?;
        let valid = match name.as_str() {
            "socModel" => value.is_null() || value.is_string(),
            "display.width" | "display.height" | "android.sdkVersion" | "android.buildTime" => {
                value.as_i64().is_some()
            }
            "display.density" => value
                .as_f64()
                .is_some_and(|value| value.is_finite() && value > 0.0),
            _ => value.is_string(),
        };
        if !valid {
            return Err(format!("invalid system property: {name}"));
        }
    }
    Ok(values.clone())
}

fn apply_sdk(
    mut info: DeviceInfo,
    sdk: Map<String, Value>,
    display: bool,
    android: bool,
) -> Result<DeviceInfo, String> {
    let string = |key: &str| {
        sdk.get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned()
    };
    for (key, target) in [
        ("name", &mut info.name),
        ("manufacturer", &mut info.manufacturer),
        ("model", &mut info.model),
        ("osName", &mut info.os_name),
        ("osVersion", &mut info.os_version),
        ("appVersion", &mut info.app_version),
        ("appBuildNumber", &mut info.app_build_number),
        ("language", &mut info.language),
        ("cpuArch", &mut info.cpu_arch),
    ] {
        if sdk.contains_key(key) {
            *target = string(key);
        }
    }
    if let Some(model) = sdk
        .get("socModel")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty() && *value != "unknown")
    {
        info.cpu_model = Some(model.to_owned());
    }
    if display || sdk.keys().any(|key| key.starts_with("display.")) {
        info.display = Some(DisplayInfo {
            width: sdk
                .get("display.width")
                .and_then(Value::as_i64)
                .unwrap_or_default() as i32,
            height: sdk
                .get("display.height")
                .and_then(Value::as_i64)
                .unwrap_or_default() as i32,
            density: sdk
                .get("display.density")
                .and_then(Value::as_f64)
                .unwrap_or_default(),
        });
    }
    if android || sdk.keys().any(|key| key.starts_with("android.")) {
        info.android = Some(AndroidExtras {
            sdk_version: sdk
                .get("android.sdkVersion")
                .and_then(Value::as_i64)
                .unwrap_or_default() as i32,
            version_code_name: string("android.versionCodeName"),
            security_patch: string("android.securityPatch"),
            bootloader: string("android.bootloader"),
            fingerprint: string("android.fingerprint"),
            hardware: string("android.hardware"),
            radio_version: string("android.radioVersion"),
            board: string("android.board"),
            build_brand: string("android.buildBrand"),
            build_number: string("android.buildNumber"),
            device: string("android.device"),
            java_vm_version: string("android.javaVmVersion"),
            gl_es_version: string("android.glEsVersion"),
            build_time: Instant(
                chrono::DateTime::from_timestamp_millis(
                    sdk.get("android.buildTime")
                        .and_then(Value::as_i64)
                        .unwrap_or_default(),
                )
                .ok_or("invalid Android build timestamp")?,
            ),
        });
    }
    Ok(info)
}

fn direct(fields: &[String], directory: &std::path::Path) -> Result<DeviceInfo, String> {
    let selected = |name: &str| fields.iter().any(|field| field == name);
    let mut info = DeviceInfo {
        name: String::new(),
        platform: platform(),
        manufacturer: String::new(),
        model: String::new(),
        os_name: String::new(),
        os_version: String::new(),
        kernel_version: String::new(),
        app_version: String::new(),
        app_build_number: String::new(),
        language: String::new(),
        cpu_arch: String::new(),
        cpu_model: None,
        total_memory: Long(0),
        total_storage: Long(0),
        display: None,
        android: None,
    };
    if selected("kernelVersion") {
        #[cfg(target_os = "ios")]
        {
            info.kernel_version = super::apple::kernel_version()?;
        }
        #[cfg(not(target_os = "ios"))]
        {
            info.kernel_version = sysinfo::System::kernel_version().unwrap_or_default();
        }
    }
    if selected("totalMemory") {
        #[cfg(target_os = "ios")]
        {
            info.total_memory = Long(super::apple::total_memory()?);
        }
        #[cfg(not(target_os = "ios"))]
        {
            let mut system = sysinfo::System::new();
            system.refresh_memory();
            info.total_memory = Long(system.total_memory().min(i64::MAX as u64) as i64);
        }
    }
    if selected("totalStorage") {
        info.total_storage = Long(super::storage_bytes(directory, true)?);
    }
    #[cfg(target_os = "android")]
    {
        info.os_name = "Android".into();
    }
    #[cfg(target_os = "ios")]
    {
        info.manufacturer = "Apple".into();
        if selected("model") || selected("cpuArch") {
            let machine = machine()?;
            info.model = machine.clone();
            info.cpu_arch = machine;
        }
    }
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        if selected("name") {
            info.name = sysinfo::System::host_name().unwrap_or_default();
        }
        if selected("cpuArch") {
            info.cpu_arch = std::env::consts::ARCH.into();
        }
        if selected("osName") {
            #[cfg(target_os = "linux")]
            {
                info.os_name = plain_rs::platform::linux::device_info::os_release();
            }
            #[cfg(not(target_os = "linux"))]
            {
                info.os_name = sysinfo::System::name().unwrap_or_default();
            }
        }
        if selected("osVersion") {
            #[cfg(target_os = "linux")]
            {
                info.os_version = plain_rs::platform::linux::device_info::os_release();
            }
            #[cfg(not(target_os = "linux"))]
            {
                info.os_version = sysinfo::System::os_version().unwrap_or_default();
            }
        }
        if selected("appVersion") {
            info.app_version = crate::platform::version::version().into();
        }
        if selected("appBuildNumber") {
            info.app_build_number = crate::platform::version::full_version();
        }
        #[cfg(target_os = "linux")]
        {
            if selected("manufacturer") {
                info.manufacturer = plain_rs::platform::linux::device_info::dmi_manufacturer();
            }
            if selected("model") {
                info.model = plain_rs::platform::linux::device_info::dmi_model();
                if info.model.is_empty() {
                    info.model = std::env::var("PLAINAPP_MODEL").unwrap_or_default();
                }
            }
        }
    }
    #[cfg(not(target_os = "ios"))]
    if selected("cpuModel") {
        #[cfg(target_os = "android")]
        {
            info.cpu_model = std::fs::read_to_string("/proc/cpuinfo")
                .ok()
                .and_then(|text| cpu_model(&text));
        }
        #[cfg(not(target_os = "android"))]
        {
            let mut system = sysinfo::System::new();
            system.refresh_cpu_all();
            info.cpu_model = system
                .cpus()
                .first()
                .map(|cpu| cpu.brand().to_owned())
                .filter(|value| !value.is_empty());
        }
    }
    Ok(info)
}

fn platform() -> DevicePlatform {
    match std::env::consts::OS {
        "android" => DevicePlatform::Android,
        "ios" => DevicePlatform::Ios,
        "macos" => DevicePlatform::Macos,
        "windows" => DevicePlatform::Windows,
        _ => DevicePlatform::Linux,
    }
}

fn cpu_model(text: &str) -> Option<String> {
    text.lines()
        .filter_map(|line| line.split_once(':'))
        .find(|(key, _)| key.trim() == "model name")
        .map(|(_, value)| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

#[cfg(target_os = "ios")]
fn machine() -> Result<String, String> {
    let mut name = std::mem::MaybeUninit::<libc::utsname>::uninit();
    if unsafe { libc::uname(name.as_mut_ptr()) } != 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    let name = unsafe { name.assume_init() };
    Ok(unsafe { std::ffi::CStr::from_ptr(name.machine.as_ptr()) }
        .to_string_lossy()
        .into_owned())
}

#[cfg(test)]
#[path = "../../../tests/unit/services/device/info.rs"]
mod tests;
