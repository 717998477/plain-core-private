use crate::host::bridge::Host;

pub(crate) async fn ipv4(host: &Host) -> Result<String, String> {
    let value = if cfg!(any(target_os = "android", target_os = "ios")) {
        host.call("systemIpv4Address", serde_json::json!({}))
            .await?
            .as_str()
            .ok_or("Invalid IPv4 reading")?
            .to_owned()
    } else {
        plain_rs::protocols::mdns::host_responder::local_ipv4_strs()
            .into_iter()
            .next()
            .ok_or("No LAN address available for casting")?
    };
    let address: std::net::Ipv4Addr = value.parse().map_err(|_| "Invalid IPv4 reading")?;
    if address.is_unspecified() || address.is_loopback() {
        return Err("No LAN address available for casting".into());
    }
    Ok(address.to_string())
}
