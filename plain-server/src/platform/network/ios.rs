use super::{Interface, InterfaceType};
use std::{
    ffi::CStr,
    net::{IpAddr, Ipv4Addr, Ipv6Addr},
};

struct Addresses(*mut libc::ifaddrs);
impl Drop for Addresses {
    fn drop(&mut self) {
        unsafe {
            libc::freeifaddrs(self.0);
        }
    }
}

fn kind(name: &str) -> InterfaceType {
    if name == "en0" {
        InterfaceType::Wireless80211
    } else if name.starts_with("en") || name.starts_with("bridge") {
        InterfaceType::Ethernet
    } else if name.starts_with("utun") || name.starts_with("ipsec") {
        InterfaceType::Tunnel
    } else if name.starts_with("ppp") {
        InterfaceType::Ppp
    } else {
        InterfaceType::Unknown
    }
}

pub(super) fn interfaces() -> Result<Vec<Interface>, String> {
    let mut head = std::ptr::null_mut();
    if unsafe { libc::getifaddrs(&mut head) } != 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    let _addresses = Addresses(head);
    let mut result = Vec::new();
    let mut current = head;
    while !current.is_null() {
        let row = unsafe { &*current };
        current = row.ifa_next;
        if row.ifa_addr.is_null() || row.ifa_name.is_null() {
            continue;
        }
        let address = unsafe {
            match i32::from((*row.ifa_addr).sa_family) {
                libc::AF_INET => Some(IpAddr::V4(Ipv4Addr::from(
                    (*(row.ifa_addr.cast::<libc::sockaddr_in>()))
                        .sin_addr
                        .s_addr
                        .to_ne_bytes(),
                ))),
                libc::AF_INET6 => Some(IpAddr::V6(Ipv6Addr::from(
                    (*(row.ifa_addr.cast::<libc::sockaddr_in6>()))
                        .sin6_addr
                        .s6_addr,
                ))),
                _ => None,
            }
        };
        let Some(address) = address else {
            continue;
        };
        let name = unsafe { CStr::from_ptr(row.ifa_name) }.to_string_lossy();
        result.push(Interface {
            if_type: kind(&name),
            ipv4: match address {
                IpAddr::V4(ip) => vec![ip],
                _ => vec![],
            },
            up: row.ifa_flags & libc::IFF_UP as u32 != 0,
            loopback: row.ifa_flags & libc::IFF_LOOPBACK as u32 != 0,
            tunnel: row.ifa_flags & libc::IFF_POINTOPOINT as u32 != 0,
        });
    }
    Ok(result)
}

#[cfg(test)]
#[path = "../../../tests/unit/platform/network/ios.rs"]
mod tests;
