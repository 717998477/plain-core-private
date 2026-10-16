use crate::assembly::server::ServerState;
use anyhow::Result;
use plain_rs::discovery::advertisement;
use plain_rs::discovery::advertisement::Facts;
use serde_json::{Value, json};
pub(crate) enum Request {
    Reply {},
    Mdns {},
    Ble {},
}
pub(crate) async fn execute(state: &ServerState, request: Request) -> Result<Value> {
    let id = state
        .prefs
        .get::<String>(plain_rs::prefs::system::keys::CLIENT_ID)?
        .unwrap_or_default();
    if matches!(request, Request::Ble {}) {
        let (supported, running) = aware_flags(state).await?;
        return Ok(json!(plain_rs::utils::base64::base64_encode(
            &advertisement::ble(&id, supported, running)?
        )));
    }
    let settings = plain_rs::prefs::user::snapshot(&state.prefs)?;
    let name = settings.device_name;
    let port = state.public_ports().await.map(|ports| ports.1).unwrap_or(0);
    let reply = advertisement::reply(&id, &name, port, facts(state).await?)?;
    match request {
        Request::Reply {} => Ok(serde_json::to_value(reply)?),
        Request::Mdns {} => Ok(serde_json::to_value(advertisement::mdns(
            &reply,
            &plain_rs::prefs::ensure_mdns_hostname(&state.prefs),
        )?)?),
        Request::Ble {} => unreachable!(),
    }
}
pub(crate) async fn device_type(state: &ServerState) -> Result<plain_rs::chat::enums::DeviceType> {
    let value = state
        .host
        .startup_property("deviceType")
        .ok_or_else(|| anyhow::anyhow!("Missing startup device type"))?;
    Ok(serde_json::from_value(value.clone())?)
}

async fn aware_flags(state: &ServerState) -> Result<(bool, bool)> {
    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        let value = state
            .host
            .call(
                "systemProperties",
                json!({"names":["wifiAwareSupported", "wifiAwareRunning"]}),
            )
            .await
            .map_err(anyhow::Error::msg)?;
        let supported = value["wifiAwareSupported"]
            .as_bool()
            .ok_or_else(|| anyhow::anyhow!("Invalid Wi-Fi Aware support reading"))?;
        let running = value["wifiAwareRunning"]
            .as_bool()
            .ok_or_else(|| anyhow::anyhow!("Invalid Wi-Fi Aware state reading"))?;
        Ok((supported, running))
    }
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        let _ = state;
        Ok((false, false))
    }
}
async fn facts(state: &ServerState) -> Result<Facts> {
    let name = crate::services::device::info::default_device_name(&state.host)
        .await
        .map_err(anyhow::Error::msg)?;
    let device_type = device_type(state).await?;
    let version = state
        .host
        .startup_property("appVersion")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("Missing startup app version"))?
        .to_owned();
    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        let value = state
            .host
            .call(
                "systemProperties",
                json!({"names":["ipv4Addresses", "wifiAwareSupported", "wifiAwareRunning"]}),
            )
            .await
            .map_err(anyhow::Error::msg)?;
        #[derive(serde::Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Properties {
            ipv4_addresses: Vec<String>,
            wifi_aware_supported: bool,
            wifi_aware_running: bool,
        }
        let value: Properties = serde_json::from_value(value)?;
        Ok(Facts {
            name,
            device_type,
            version,
            platform: std::env::consts::OS.to_uppercase(),
            ips: value.ipv4_addresses,
            aware_supported: value.wifi_aware_supported,
            aware_running: value.wifi_aware_running,
        })
    }
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        Ok(Facts {
            name,
            device_type,
            version,
            platform: std::env::consts::OS.to_uppercase(),
            ips: plain_rs::protocols::mdns::host_responder::local_ipv4_strs(),
            aware_supported: false,
            aware_running: false,
        })
    }
}
#[cfg(test)]
#[path = "../../../tests/unit/services/discovery/discovery_advertisement.rs"]
mod tests;
