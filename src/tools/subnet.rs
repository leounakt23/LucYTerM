//! Offline IPv4 CIDR calculations.

use std::net::Ipv4Addr;

/// Calculated IPv4 network information.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubnetInfo {
    pub network: Ipv4Addr,
    pub broadcast: Ipv4Addr,
    pub first_host: Ipv4Addr,
    pub last_host: Ipv4Addr,
    pub prefix: u8,
    pub host_count: u64,
}

/// Calculate an IPv4 CIDR range.
pub fn calculate(cidr: &str) -> Result<SubnetInfo, String> {
    let (address, prefix) = cidr.split_once('/').ok_or("CIDR must contain '/'")?;
    let address: Ipv4Addr = address.parse().map_err(|_| "invalid IPv4 address")?;
    let prefix: u8 = prefix.parse().map_err(|_| "invalid prefix length")?;
    if prefix > 32 {
        return Err("prefix length must be 0..=32".into());
    }
    let mask = if prefix == 0 {
        0
    } else {
        u32::MAX << (32 - prefix)
    };
    let network = u32::from(address) & mask;
    let broadcast = network | !mask;
    let (first_host, last_host, host_count) = match prefix {
        32 => (network, network, 1),
        31 => (network, broadcast, 2),
        _ => (
            network + 1,
            broadcast - 1,
            u64::from(broadcast - network - 1),
        ),
    };
    Ok(SubnetInfo {
        network: network.into(),
        broadcast: broadcast.into(),
        first_host: first_host.into(),
        last_host: last_host.into(),
        prefix,
        host_count,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn calculates_range() {
        let info = calculate("192.168.1.20/24").unwrap();
        assert_eq!(info.network, Ipv4Addr::new(192, 168, 1, 0));
        assert_eq!(info.broadcast, Ipv4Addr::new(192, 168, 1, 255));
        assert_eq!(info.host_count, 254);
    }
}
