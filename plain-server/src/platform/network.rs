use crate::host::bridge::Host;

pub(crate) async fn ipv4(host: &Host) -> Result<String, String> {
    let value = lan_ipv4(host).await?;
    if value.is_empty() {
        return Err("No LAN address available for casting".into());
    }
    let address: std::net::Ipv4Addr = value.parse().map_err(|_| "Invalid IPv4 reading")?;
    if address.is_unspecified() || address.is_loopback() {
        return Err("No LAN address available for casting".into());
    }
    Ok(address.to_string())
}

pub(crate) async fn lan_ipv4(host: &Host) -> Result<String, String> {
    if cfg!(any(target_os = "android", target_os = "ios")) {
        Ok(host
            .call("systemIpv4Address", serde_json::json!({}))
            .await?
            .as_str()
            .ok_or("Invalid IPv4 reading")?
            .to_owned())
    } else {
        Ok(plain_rs::protocols::mdns::host_responder::local_ipv4_strs()
            .into_iter()
            .next()
            .unwrap_or_default())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub(crate) enum NetworkType {
    Wifi,
    Cellular,
    Ethernet,
    None,
    Other,
}

pub(crate) async fn network_type(host: &Host) -> Result<NetworkType, String> {
    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        serde_json::from_value(
            host.call("systemNetworkType", serde_json::json!({}))
                .await?,
        )
        .map_err(|error| error.to_string())
    }
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        let _ = host;
        tokio::task::spawn_blocking(|| {
            use netdev::interface::types::InterfaceType;
            let Ok(interface) = netdev::get_default_interface() else {
                return NetworkType::None;
            };
            match interface.if_type {
                InterfaceType::Wireless80211 => NetworkType::Wifi,
                InterfaceType::Wwan | InterfaceType::Wwanpp | InterfaceType::Wwanpp2 => {
                    NetworkType::Cellular
                }
                InterfaceType::Ethernet
                | InterfaceType::GigabitEthernet
                | InterfaceType::FastEthernetT
                | InterfaceType::FastEthernetFx => NetworkType::Ethernet,
                _ => NetworkType::Other,
            }
        })
        .await
        .map_err(|error| error.to_string())
    }
}
