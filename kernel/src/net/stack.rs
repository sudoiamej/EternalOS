use crate::net::arp::{ArpCache, ArpPacket, ARP_OP_REQUEST};
use crate::net::dns::{build_dns_query, parse_dns_response};
use crate::net::icmp::{IcmpHeader, ICMP_TYPE_ECHO_REQUEST};
use crate::net::e1000::{E1000Driver, E1000_DEV_82540EM, E1000_DEV_82543GC, E1000_DEV_82545EM, E1000_VENDOR_INTEL};
use crate::net::ipv4::Ipv4Header;
use crate::net::udp::UdpHeader;
use crate::net::wire::{ethertype, ip_proto, EthernetAddress, EthernetHeader, Ipv4Address};
use crate::net::{NetConfig, NetworkDevice};
use crate::pci::with_pci_registry;
use crate::sync::SpinLock;

pub const MAX_PACKET_LEN: usize = 1536;

pub struct NetworkStack {
    pub device: Option<E1000Driver>,
    pub config: NetConfig,
    pub arp_cache: ArpCache,
    pub next_ip_id: u16,
    pub next_dns_id: u16,
}

impl NetworkStack {
    pub const fn new() -> Self {
        Self {
            device: None,
            config: NetConfig {
                mac: EthernetAddress::ZERO,
                ip: Ipv4Address::new(10, 0, 2, 15),
                subnet_mask: crate::net::wire::Ipv4Address::new(255, 255, 255, 0),
                gateway: crate::net::wire::Ipv4Address::new(10, 0, 2, 2),
                dns: crate::net::wire::Ipv4Address::new(10, 0, 2, 3),
            },
            arp_cache: ArpCache::new(),
            next_ip_id: 1,
            next_dns_id: 0x1337,
        }
    }

    /// Resolve destination MAC address: checks if on local subnet, otherwise uses default gateway MAC
    pub fn resolve_mac(&mut self, target_ip: Ipv4Address) -> Option<EthernetAddress> {
        let nexthop_ip = if target_ip.is_broadcast() {
            return Some(EthernetAddress::BROADCAST);
        } else if self.config.ip.is_same_subnet(&target_ip, &self.config.subnet_mask) {
            target_ip
        } else {
            self.config.gateway
        };

        if let Some(mac) = self.arp_cache.lookup(nexthop_ip) {
            Some(mac)
        } else {
            // Send ARP request for unresolved IP
            let _ = self.send_arp_request(nexthop_ip);
            None
        }
    }

    /// Send an ARP Request broadcast for an IP
    pub fn send_arp_request(&mut self, target_ip: Ipv4Address) -> Result<(), &'static str> {
        let dev = self.device.as_mut().ok_or("No network device initialized")?;
        let arp = ArpPacket::new_request(dev.mac_address(), self.config.ip, target_ip);

        let mut frame = [0u8; 64]; // Minimum Ethernet frame is 60/64 bytes
        let eth = EthernetHeader {
            dst: EthernetAddress::BROADCAST,
            src: dev.mac_address(),
            ethertype: ethertype::ARP,
        };

        eth.emit(&mut frame[0..14])?;
        arp.emit(&mut frame[14..42])?;

        dev.transmit(&frame)
    }

    /// Send an ICMP Echo Request (Ping)
    pub fn send_icmp_ping(&mut self, target_ip: Ipv4Address, seq: u16) -> Result<(), &'static str> {
        let dst_mac = match self.resolve_mac(target_ip) {
            Some(m) => m,
            None => {
                // Sent ARP request, give backend up to 500ms (50 ticks) to reply
                let start = crate::timer::get_ticks();
                let mut resolved = None;
                while crate::timer::get_ticks().saturating_sub(start) < 50 {
                    let _ = self.poll();
                    if let Some(m) = self.arp_cache.lookup(if target_ip.is_same_subnet(&self.config.ip, &self.config.subnet_mask) { target_ip } else { self.config.gateway }) {
                        resolved = Some(m);
                        break;
                    }
                    core::hint::spin_loop();
                }

                // If still not resolved in quick poll, fallback to known QEMU default gateway MAC (52:54:00:12:34:02)
                match resolved {
                    Some(m) => m,
                    None => {
                        // In QEMU slirp, gateway MAC is 52:54:00:12:34:02
                        let default_gw_mac = EthernetAddress::new(0x52, 0x54, 0x00, 0x12, 0x34, 0x02);
                        self.arp_cache.insert(self.config.gateway, default_gw_mac, crate::timer::get_ticks());
                        default_gw_mac
                    }
                }
            }
        };

        let dev = self.device.as_mut().ok_or("No network device initialized")?;
        let payload = b"EternalOS Network Ping Payload 2026";
        let icmp = IcmpHeader::new_echo_request(0xCAFE, seq);

        let mut icmp_buf = [0u8; 128];
        let icmp_len = icmp.emit(&mut icmp_buf, payload)?;

        self.next_ip_id = self.next_ip_id.wrapping_add(1);
        let ip_hdr = Ipv4Header::new(ip_proto::ICMP, self.config.ip, target_ip, self.next_ip_id);

        let mut packet = [0u8; 256];
        let eth = EthernetHeader {
            dst: dst_mac,
            src: dev.mac_address(),
            ethertype: ethertype::IPV4,
        };

        let eth_len = eth.emit(&mut packet[0..14])?;
        let ip_len = ip_hdr.emit(&mut packet[eth_len..], icmp_len)?;
        packet[eth_len + ip_len..eth_len + ip_len + icmp_len].copy_from_slice(&icmp_buf[..icmp_len]);

        let total_frame_len = (eth_len + ip_len + icmp_len).max(60);
        dev.transmit(&packet[..total_frame_len])
    }

    /// Send a UDP datagram to target IP and port
    pub fn send_udp(&mut self, target_ip: Ipv4Address, src_port: u16, dst_port: u16, data: &[u8]) -> Result<(), &'static str> {
        let dst_mac = match self.resolve_mac(target_ip) {
            Some(m) => m,
            None => {
                for _ in 0..10_000 {
                    let _ = self.poll();
                    core::hint::spin_loop();
                }
                match self.resolve_mac(target_ip) {
                    Some(m) => m,
                    None => return Err("ARP resolution failed for destination"),
                }
            }
        };

        let dev = self.device.as_mut().ok_or("No network device initialized")?;
        let udp = UdpHeader::new(src_port, dst_port);

        let mut udp_buf = [0u8; 1024];
        let udp_len = udp.emit(self.config.ip, target_ip, &mut udp_buf, data)?;

        self.next_ip_id = self.next_ip_id.wrapping_add(1);
        let ip_hdr = Ipv4Header::new(ip_proto::UDP, self.config.ip, target_ip, self.next_ip_id);

        let mut packet = [0u8; 1200];
        let eth = EthernetHeader {
            dst: dst_mac,
            src: dev.mac_address(),
            ethertype: ethertype::IPV4,
        };

        let eth_len = eth.emit(&mut packet[0..14])?;
        let ip_len = ip_hdr.emit(&mut packet[eth_len..], udp_len)?;
        packet[eth_len + ip_len..eth_len + ip_len + udp_len].copy_from_slice(&udp_buf[..udp_len]);

        let total_frame_len = (eth_len + ip_len + udp_len).max(60);
        dev.transmit(&packet[..total_frame_len])
    }

    /// Perform a blocking DNS query to resolve a domain name into an IPv4 address
    pub fn resolve_dns(&mut self, hostname: &str, timeout_ms: u64) -> Result<Ipv4Address, &'static str> {
        let dns_server = self.config.dns;
        let trans_id = self.next_dns_id;
        self.next_dns_id = self.next_dns_id.wrapping_add(1);

        let mut query_buf = [0u8; 512];
        let query_len = build_dns_query(trans_id, hostname, &mut query_buf)?;

        // Send query to DNS server on port 53 from source port 53535
        self.send_udp(dns_server, 53535, 53, &query_buf[..query_len])?;

        // Poll for response up to timeout
        let start_tick = crate::timer::get_ticks();
        let timeout_ticks = (timeout_ms / crate::timer::MS_PER_TICK).max(1);

        let mut rx_buf = [0u8; MAX_PACKET_LEN];

        while crate::timer::get_ticks().saturating_sub(start_tick) < timeout_ticks {
            if let Some(dev) = self.device.as_mut() {
                if let Ok(Some(len)) = dev.receive(&mut rx_buf) {
                    if let Ok((eth, eth_payload)) = EthernetHeader::parse(&rx_buf[..len]) {
                        if eth.ethertype == ethertype::IPV4 {
                            if let Ok((ip_hdr, ip_payload)) = Ipv4Header::parse(eth_payload) {
                                if ip_hdr.protocol == ip_proto::UDP {
                                    if let Ok((udp_hdr, udp_payload)) = UdpHeader::parse(ip_hdr.src, ip_hdr.dst, ip_payload) {
                                        if udp_hdr.dst_port == 53535 && udp_hdr.src_port == 53 {
                                            return parse_dns_response(udp_payload, trans_id);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            core::hint::spin_loop();
        }

        Err("DNS resolution timed out")
    }

    /// Process any pending incoming frames from the hardware NIC
    pub fn poll(&mut self) -> Result<(), &'static str> {
        let dev = match self.device.as_mut() {
            Some(d) => d,
            None => return Ok(()),
        };

        let mut rx_buf = [0u8; MAX_PACKET_LEN];
        while let Ok(Some(len)) = dev.receive(&mut rx_buf) {
            let frame = &rx_buf[..len];
            let (eth, payload) = match EthernetHeader::parse(frame) {
                Ok(res) => res,
                Err(_) => continue,
            };

            match eth.ethertype {
                ethertype::ARP => {
                    if let Ok(arp) = ArpPacket::parse(payload) {
                        // Update ARP cache with sender info
                        let tick = crate::timer::get_ticks();
                        self.arp_cache.insert(arp.sender_ip, arp.sender_mac, tick);

                        // If request targeted at our IP, respond with ARP reply
                        if arp.opcode == ARP_OP_REQUEST && arp.target_ip == self.config.ip {
                            let reply = ArpPacket::new_reply(
                                self.config.mac,
                                self.config.ip,
                                arp.sender_mac,
                                arp.sender_ip,
                            );

                            let mut reply_frame = [0u8; 64];
                            let eth_resp = EthernetHeader {
                                dst: arp.sender_mac,
                                src: self.config.mac,
                                ethertype: ethertype::ARP,
                            };
                            let _ = eth_resp.emit(&mut reply_frame[0..14]);
                            let _ = reply.emit(&mut reply_frame[14..42]);

                            let _ = dev.transmit(&reply_frame);
                        }
                    }
                }
                ethertype::IPV4 => {
                    if let Ok((ip_hdr, ip_payload)) = Ipv4Header::parse(payload) {
                        // Check if addressed to us or broadcast
                        if ip_hdr.dst != self.config.ip && !ip_hdr.dst.is_broadcast() {
                            continue;
                        }

                        match ip_hdr.protocol {
                            ip_proto::ICMP => {
                                if let Ok((icmp_hdr, icmp_payload)) = IcmpHeader::parse(ip_payload) {
                                    if icmp_hdr.msg_type == ICMP_TYPE_ECHO_REQUEST {
                                        // Generate and send Echo Reply
                                        let reply_icmp = IcmpHeader::new_echo_reply(icmp_hdr.id, icmp_hdr.sequence);
                                        let mut resp_icmp_buf = [0u8; 512];
                                        if let Ok(resp_icmp_len) = reply_icmp.emit(&mut resp_icmp_buf, icmp_payload) {
                                            self.next_ip_id = self.next_ip_id.wrapping_add(1);
                                            let resp_ip = Ipv4Header::new(
                                                ip_proto::ICMP,
                                                self.config.ip,
                                                ip_hdr.src,
                                                self.next_ip_id,
                                            );

                                            let mut resp_frame = [0u8; 600];
                                            let resp_eth = EthernetHeader {
                                                dst: eth.src,
                                                src: self.config.mac,
                                                ethertype: ethertype::IPV4,
                                            };

                                            let e_len = resp_eth.emit(&mut resp_frame[0..14]).unwrap_or(14);
                                            let i_len = resp_ip.emit(&mut resp_frame[e_len..], resp_icmp_len).unwrap_or(20);
                                            resp_frame[e_len + i_len..e_len + i_len + resp_icmp_len]
                                                .copy_from_slice(&resp_icmp_buf[..resp_icmp_len]);

                                            let total_len = (e_len + i_len + resp_icmp_len).max(60);
                                            let _ = dev.transmit(&resp_frame[..total_len]);
                                        }
                                    }
                                }
                            }
                            ip_proto::UDP => {
                                // Handled via endpoint or active poll listeners
                            }
                            ip_proto::TCP => {
                                // Handled via connection endpoints
                            }
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
        }

        Ok(())
    }
}

pub static NETWORK_STACK: SpinLock<NetworkStack> = SpinLock::new(NetworkStack::new());

/// Global network subsystem initialization: searches PCI bus for E1000 and registers stack
pub fn init() {
    let mut stack = NETWORK_STACK.lock();

    let e1000_pci = with_pci_registry(|reg| {
        reg.devices().iter().find(|d| {
            d.vendor_id == E1000_VENDOR_INTEL
                && (d.device_id == E1000_DEV_82540EM
                    || d.device_id == E1000_DEV_82545EM
                    || d.device_id == E1000_DEV_82543GC)
        }).copied()
    });

    if let Some(pci_dev) = e1000_pci {
        match E1000Driver::probe_and_init(&pci_dev) {
            Ok(driver) => {
                let mac = driver.mac_address();
                stack.config = NetConfig::qemu_default(mac);
                stack.device = Some(driver);
                crate::log_info!("[NET] Intel E1000 network stack initialized. IP: 10.0.2.15/24, GW: 10.0.2.2");
            }
            Err(e) => {
                crate::log_error!("[NET] Failed to initialize E1000 driver: {}", e);
            }
        }
    } else {
        crate::log_warn!("[NET] No supported Intel E1000 PCI network device detected.");
    }
}

/// Access the global network stack under lock
pub fn with_network_stack<F, R>(f: F) -> R
where
    F: FnOnce(&mut NetworkStack) -> R,
{
    let mut stack = NETWORK_STACK.lock();
    f(&mut stack)
}

/// Poll for incoming network packets
pub fn poll_network() {
    let mut stack = NETWORK_STACK.lock();
    let _ = stack.poll();
}
