use crate::host::bridge::Host;

#[cfg(any(target_os = "ios", all(test, target_os = "macos")))]
mod ios;

#[derive(Clone, Copy, PartialEq, Eq)]
enum InterfaceType {
    Wireless80211,
    Ethernet,
    Tunnel,
    Ppp,
    Unknown,
}

#[derive(Clone)]
struct Interface {
    if_type: InterfaceType,
    ipv4: Vec<std::net::Ipv4Addr>,
    up: bool,
    loopback: bool,
    tunnel: bool,
}

#[cfg(not(target_os = "ios"))]
fn interfaces() -> Vec<Interface> {
    use netdev::interface::types::InterfaceType as Kind;
    netdev::get_interfaces()
        .into_iter()
        .map(|row| Interface {
            if_type: match row.if_type {
                Kind::Wireless80211 => InterfaceType::Wireless80211,
                Kind::Ethernet
                | Kind::Ethernet3Megabit
                | Kind::FastEthernetT
                | Kind::FastEthernetFx
                | Kind::GigabitEthernet => InterfaceType::Ethernet,
                Kind::Tunnel => InterfaceType::Tunnel,
                Kind::Ppp => InterfaceType::Ppp,
                _ => InterfaceType::Unknown,
            },
            ipv4: row.ipv4.iter().map(|ip| ip.addr()).collect(),
            up: row.is_up(),
            loopback: row.is_loopback(),
            tunnel: row.is_tun(),
        })
        .collect()
}

pub(crate) async fn ipv4() -> Result<String, String> {
    let value = lan_ipv4().await?;
    if value.is_empty() {
        return Err("No LAN address available for casting".into());
    }
    let address: std::net::Ipv4Addr = value.parse().map_err(|_| "Invalid IPv4 reading")?;
    if address.is_unspecified() || address.is_loopback() {
        return Err("No LAN address available for casting".into());
    }
    Ok(address.to_string())
}

#[derive(async_graphql::SimpleObject, Clone, Debug, PartialEq, Eq)]
pub(crate) struct NetworkStatus {
    pub ipv4_addresses: Vec<String>,
    pub primary_ipv4: String,
}

pub(crate) async fn snapshot() -> Result<NetworkStatus, String> {
    tokio::task::spawn_blocking(|| {
        #[cfg(not(target_os = "ios"))]
        let interfaces = interfaces();
        #[cfg(target_os = "ios")]
        let interfaces = ios::interfaces()?;
        Ok(sample(&interfaces))
    })
    .await
    .map_err(|error| error.to_string())?
}

fn sample(interfaces: &[Interface]) -> NetworkStatus {
    let addresses = lan_ipv4_addresses(interfaces);
    NetworkStatus {
        primary_ipv4: addresses.first().cloned().unwrap_or_default(),
        ipv4_addresses: addresses,
    }
}

pub(crate) async fn lan_ipv4() -> Result<String, String> {
    Ok(snapshot().await?.primary_ipv4)
}

fn lan_ipv4_addresses(interfaces: &[Interface]) -> Vec<String> {
    let mut addresses = interfaces
        .iter()
        .filter(|interface| {
            interface.up
                && !interface.loopback
                && !interface.tunnel
                && !matches!(
                    interface.if_type,
                    InterfaceType::Tunnel | InterfaceType::Ppp
                )
        })
        .flat_map(|interface| {
            let priority = match interface.if_type {
                InterfaceType::Wireless80211 => 0,
                InterfaceType::Ethernet => 1,
                _ => 2,
            };
            interface
                .ipv4
                .iter()
                .map(move |network| (priority, *network))
        })
        .filter(|(_, address)| {
            !address.is_unspecified()
                && !address.is_loopback()
                && !address.is_link_local()
                && !address.is_multicast()
                && !address.is_broadcast()
        })
        .collect::<Vec<_>>();
    addresses.sort_by_key(|(priority, _)| *priority);
    let mut seen = std::collections::HashSet::new();
    addresses
        .into_iter()
        .filter_map(|(_, address)| seen.insert(address).then(|| address.to_string()))
        .collect()
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

#[cfg(test)]
#[path = "../../tests/unit/platform/network.rs"]
mod tests;
