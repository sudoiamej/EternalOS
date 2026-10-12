pub mod wire;
pub mod arp;
pub mod ipv4;
pub mod icmp;
pub mod udp;
pub mod tcp;
pub mod dns;
pub mod dhcp;
pub mod e1000;
pub mod stack;

use crate::net::wire::EthernetAddress;

/// Basic network interface configuration and statistics
#[derive(Debug, Clone, Copy)]
pub struct NetConfig {
    pub mac: EthernetAddress,
    pub ip: crate::net::wire::Ipv4Address,
    pub subnet_mask: crate::net::wire::Ipv4Address,
    pub gateway: crate::net::wire::Ipv4Address,
    pub dns: crate::net::wire::Ipv4Address,
}

impl NetConfig {
    /// Default QEMU User Networking configuration:
    /// QEMU slirp provides IP 10.0.2.15, Gateway 10.0.2.2, DNS 10.0.2.3
    pub const fn qemu_default(mac: EthernetAddress) -> Self {
        Self {
            mac,
            ip: crate::net::wire::Ipv4Address::new(10, 0, 2, 15),
            subnet_mask: crate::net::wire::Ipv4Address::new(255, 255, 255, 0),
            gateway: crate::net::wire::Ipv4Address::new(10, 0, 2, 2),
            dns: crate::net::wire::Ipv4Address::new(10, 0, 2, 3),
        }
    }
}

/// Packet statistics tracking
#[derive(Debug, Clone, Copy, Default)]
pub struct NetStats {
    pub rx_packets: u64,
    pub rx_bytes: u64,
    pub tx_packets: u64,
    pub tx_bytes: u64,
    pub rx_dropped: u64,
    pub tx_dropped: u64,
    pub rx_errors: u64,
    pub tx_errors: u64,
}

/// Abstract network device trait representing an Ethernet controller
pub trait NetworkDevice: Send + Sync {
    /// Get the hardware MAC address of the device
    fn mac_address(&self) -> EthernetAddress;

    /// Transmit an Ethernet frame (blocking or queued DMA)
    fn transmit(&mut self, frame: &[u8]) -> Result<(), &'static str>;

    /// Poll for a received frame into `buf`. Returns length of received frame or None if no packet is pending.
    fn receive(&mut self, buf: &mut [u8]) -> Result<Option<usize>, &'static str>;

    /// Check if hardware link is active/up
    fn is_link_up(&self) -> bool;

    /// Get reference to packet stats
    fn stats(&self) -> NetStats;
}

pub use stack::{init, poll_network, with_network_stack, NetworkStack};
