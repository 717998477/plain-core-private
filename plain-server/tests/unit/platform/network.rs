use super::*;

fn select_lan_ipv4(interfaces: &[Interface]) -> String {
    sample(interfaces).primary_ipv4
}

fn interface(kind: InterfaceType, addresses: &[&str]) -> Interface {
    Interface {
        if_type: kind,
        ipv4: addresses
            .iter()
            .map(|value| value.split('/').next().unwrap().parse().unwrap())
            .collect(),
        up: true,
        loopback: false,
        tunnel: false,
        default: false,
    }
}

#[test]
fn lan_selection_uses_one_shared_wireless_ethernet_fallback_order() {
    let other = interface(InterfaceType::Unknown, &["192.0.2.1/24"]);
    let ethernet = interface(InterfaceType::Ethernet, &["198.51.100.1/24"]);
    let wireless = interface(InterfaceType::Wireless80211, &["203.0.113.1/24"]);
    assert_eq!(
        select_lan_ipv4(&[other.clone(), ethernet.clone(), wireless]),
        "203.0.113.1"
    );
    assert_eq!(select_lan_ipv4(&[other.clone(), ethernet]), "198.51.100.1");
    assert_eq!(select_lan_ipv4(&[other]), "192.0.2.1");
    assert_eq!(select_lan_ipv4(&[]), "");
}

#[test]
fn lan_selection_excludes_down_loopback_tunnel_and_unusable_addresses() {
    let mut down = interface(InterfaceType::Wireless80211, &["203.0.113.1/24"]);
    down.up = false;
    let mut loopback = interface(InterfaceType::Wireless80211, &["203.0.113.2/24"]);
    loopback.loopback = true;
    let mut tunnel = interface(InterfaceType::Unknown, &["203.0.113.3/24"]);
    tunnel.tunnel = true;
    let declared_tunnel = interface(InterfaceType::Tunnel, &["203.0.113.4/24"]);
    let ppp = interface(InterfaceType::Ppp, &["203.0.113.5/24"]);
    let invalid = interface(
        InterfaceType::Wireless80211,
        &[
            "0.0.0.0/0",
            "127.0.0.1/8",
            "169.254.1.1/16",
            "224.0.0.1/4",
            "255.255.255.255/32",
        ],
    );
    let ethernet = interface(InterfaceType::Ethernet, &["198.51.100.1/24"]);
    let mut candidates = vec![down, loopback, tunnel, declared_tunnel, ppp, invalid];
    assert_eq!(select_lan_ipv4(&candidates), "");
    candidates.push(ethernet);
    assert_eq!(select_lan_ipv4(&candidates), "198.51.100.1");
}

#[test]
fn lan_selection_retains_sample_order_between_equal_priority_interfaces() {
    let first = interface(
        InterfaceType::Wireless80211,
        &["203.0.113.1/24", "203.0.113.2/24"],
    );
    let second = interface(InterfaceType::Wireless80211, &["198.51.100.1/24"]);
    assert_eq!(
        select_lan_ipv4(&[first.clone(), second.clone()]),
        "203.0.113.1"
    );
    assert_eq!(select_lan_ipv4(&[second, first]), "198.51.100.1");
}

#[test]
fn network_sample_deduplicates_addresses_and_requires_an_active_default_tunnel() {
    let wireless = interface(
        InterfaceType::Wireless80211,
        &["203.0.113.1/24", "203.0.113.1/24"],
    );
    let mut tunnel = interface(InterfaceType::Tunnel, &["198.51.100.1/24"]);
    assert!(!sample(&[wireless.clone(), tunnel.clone()]).vpn_active);
    tunnel.default = true;
    let status = sample(&[wireless, tunnel.clone()]);
    assert!(status.vpn_active);
    assert_eq!(status.ipv4_addresses, ["203.0.113.1"]);
    tunnel.up = false;
    assert!(!sample(&[tunnel]).vpn_active);
}
