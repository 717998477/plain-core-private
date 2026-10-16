use super::*;

#[test]
fn apple_interface_names_preserve_wireless_wired_and_tunnel_facts() {
    assert!(matches!(kind("en0"), InterfaceType::Wireless80211));
    for name in ["en1", "bridge0"] {
        assert!(matches!(kind(name), InterfaceType::Ethernet));
    }
    for name in ["utun0", "ipsec3"] {
        assert!(matches!(kind(name), InterfaceType::Tunnel));
    }
    assert!(matches!(kind("ppp0"), InterfaceType::Ppp));
    for name in ["pdp_ip0", "lo0"] {
        assert!(matches!(kind(name), InterfaceType::Unknown));
    }
}
