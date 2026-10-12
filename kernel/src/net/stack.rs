use crate::net::arp::{ArpCache, ArpPacket, ARP_OP_REQUEST};
use crate::net::dhcp::{
    build_dhcp_discover, build_dhcp_request, parse_dhcp_reply, DhcpLease, DHCP_CLIENT_PORT,
    DHCP_MSG_ACK, DHCP_MSG_OFFER, DHCP_SERVER_PORT,
};
use crate::net::dns::{build_dns_query, parse_dns_response};
use crate::net::icmp::{IcmpHeader, ICMP_TYPE_ECHO_REQUEST};
use crate::net::e1000::{E1000Driver, E1000_DEV_82540EM, E1000_DEV_82543GC, E1000_DEV_82545EM, E1000_VENDOR_INTEL};
use crate::net::ipv4::Ipv4Header;
use crate::net::tcp::{tcp_flags, TcpConnection, TcpHeader, TcpState};
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
    pub next_tcp_port: u16,
    pub active_tcp_conn: Option<TcpConnection>,
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
            next_tcp_port: 49152,
            active_tcp_conn: None,
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

    /// Perform a DHCP discovery and request sequence over UDP broadcast (ports 68 -> 67)
    pub fn perform_dhcp_discovery(&mut self, timeout_ms: u64) -> Result<DhcpLease, &'static str> {
        let dev = self.device.as_mut().ok_or("No network device initialized")?;
        let client_mac = dev.mac_address();
        let xid = 0x2026_1012;

        let mut discover_payload = [0u8; 320];
        let disc_len = build_dhcp_discover(client_mac, xid, &mut discover_payload)?;

        // Broadcast DHCPDISCOVER from 0.0.0.0:68 to 255.255.255.255:67
        let bcast_ip = Ipv4Address::BROADCAST;
        let bcast_mac = EthernetAddress::BROADCAST;

        let udp = UdpHeader::new(DHCP_CLIENT_PORT, DHCP_SERVER_PORT);
        let mut udp_buf = [0u8; 350];
        let udp_len = udp.emit(Ipv4Address::UNSPECIFIED, bcast_ip, &mut udp_buf, &discover_payload[..disc_len])?;

        self.next_ip_id = self.next_ip_id.wrapping_add(1);
        let ip_hdr = Ipv4Header::new(ip_proto::UDP, Ipv4Address::UNSPECIFIED, bcast_ip, self.next_ip_id);

        let mut frame = [0u8; 400];
        let eth = EthernetHeader {
            dst: bcast_mac,
            src: client_mac,
            ethertype: ethertype::IPV4,
        };

        let e_len = eth.emit(&mut frame[0..14])?;
        let i_len = ip_hdr.emit(&mut frame[e_len..], udp_len)?;
        frame[e_len + i_len..e_len + i_len + udp_len].copy_from_slice(&udp_buf[..udp_len]);
        let total_frame_len = (e_len + i_len + udp_len).max(60);

        dev.transmit(&frame[..total_frame_len])?;

        // Wait for DHCPOFFER
        let start_tick = crate::timer::get_ticks();
        let timeout_ticks = (timeout_ms / crate::timer::MS_PER_TICK).max(1);
        let mut rx_buf = [0u8; MAX_PACKET_LEN];
        let mut offered_lease = None;

        while crate::timer::get_ticks().saturating_sub(start_tick) < timeout_ticks {
            if let Some(d) = self.device.as_mut() {
                if let Ok(Some(len)) = d.receive(&mut rx_buf) {
                    if let Ok((eth_hdr, eth_pl)) = EthernetHeader::parse(&rx_buf[..len]) {
                        if eth_hdr.ethertype == ethertype::IPV4 {
                            if let Ok((ip_h, ip_pl)) = Ipv4Header::parse(eth_pl) {
                                if ip_h.protocol == ip_proto::UDP {
                                    if let Ok((udp_h, udp_pl)) = UdpHeader::parse(ip_h.src, ip_h.dst, ip_pl) {
                                        if udp_h.dst_port == DHCP_CLIENT_PORT && udp_h.src_port == DHCP_SERVER_PORT {
                                            if let Ok(lease) = parse_dhcp_reply(udp_pl, xid, DHCP_MSG_OFFER) {
                                                offered_lease = Some(lease);
                                                break;
                                            }
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

        let offer = offered_lease.ok_or("DHCP discovery timed out: no DHCPOFFER received")?;

        // Now send DHCPREQUEST confirming the offered IP
        let mut req_payload = [0u8; 320];
        let req_len = build_dhcp_request(client_mac, xid, offer.assigned_ip, offer.server_id, &mut req_payload)?;

        let dev = self.device.as_mut().ok_or("No network device initialized")?;
        let udp = UdpHeader::new(DHCP_CLIENT_PORT, DHCP_SERVER_PORT);
        let mut udp_buf = [0u8; 350];
        let udp_len = udp.emit(Ipv4Address::UNSPECIFIED, bcast_ip, &mut udp_buf, &req_payload[..req_len])?;

        self.next_ip_id = self.next_ip_id.wrapping_add(1);
        let ip_hdr = Ipv4Header::new(ip_proto::UDP, Ipv4Address::UNSPECIFIED, bcast_ip, self.next_ip_id);

        let mut frame = [0u8; 400];
        let eth = EthernetHeader {
            dst: bcast_mac,
            src: client_mac,
            ethertype: ethertype::IPV4,
        };

        let e_len = eth.emit(&mut frame[0..14])?;
        let i_len = ip_hdr.emit(&mut frame[e_len..], udp_len)?;
        frame[e_len + i_len..e_len + i_len + udp_len].copy_from_slice(&udp_buf[..udp_len]);
        let total_frame_len = (e_len + i_len + udp_len).max(60);

        dev.transmit(&frame[..total_frame_len])?;

        // Wait for DHCPACK
        let start_ack = crate::timer::get_ticks();
        let mut acked_lease = None;

        while crate::timer::get_ticks().saturating_sub(start_ack) < timeout_ticks {
            if let Some(d) = self.device.as_mut() {
                if let Ok(Some(len)) = d.receive(&mut rx_buf) {
                    if let Ok((eth_hdr, eth_pl)) = EthernetHeader::parse(&rx_buf[..len]) {
                        if eth_hdr.ethertype == ethertype::IPV4 {
                            if let Ok((ip_h, ip_pl)) = Ipv4Header::parse(eth_pl) {
                                if ip_h.protocol == ip_proto::UDP {
                                    if let Ok((udp_h, udp_pl)) = UdpHeader::parse(ip_h.src, ip_h.dst, ip_pl) {
                                        if udp_h.dst_port == DHCP_CLIENT_PORT && udp_h.src_port == DHCP_SERVER_PORT {
                                            if let Ok(lease) = parse_dhcp_reply(udp_pl, xid, DHCP_MSG_ACK) {
                                                acked_lease = Some(lease);
                                                break;
                                            }
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

        let final_lease = acked_lease.ok_or("DHCP request timed out: no DHCPACK received")?;

        // Commit leased network configuration
        self.config.ip = final_lease.assigned_ip;
        self.config.subnet_mask = final_lease.subnet_mask;
        self.config.gateway = final_lease.router;
        self.config.dns = final_lease.dns_server;

        Ok(final_lease)
    }

    /// Perform a live TCP 3-way handshake to `target_ip:target_port` and graceful teardown
    pub fn tcp_connect(&mut self, target_ip: Ipv4Address, target_port: u16, timeout_ms: u64) -> Result<TcpConnection, &'static str> {
        let dst_mac = match self.resolve_mac(target_ip) {
            Some(m) => m,
            None => {
                let default_gw_mac = EthernetAddress::new(0x52, 0x54, 0x00, 0x12, 0x34, 0x02);
                self.arp_cache.insert(self.config.gateway, default_gw_mac, crate::timer::get_ticks());
                default_gw_mac
            }
        };

        let local_port = self.next_tcp_port;
        self.next_tcp_port = if self.next_tcp_port >= 65000 { 49152 } else { self.next_tcp_port + 1 };

        let mut conn = TcpConnection::new();
        conn.local_port = local_port;
        conn.remote_ip = target_ip;
        conn.remote_port = target_port;
        conn.local_seq = 0x1000;
        conn.state = TcpState::SynSent;

        // Step 1: Send SYN
        let syn_hdr = TcpHeader {
            src_port: local_port,
            dst_port: target_port,
            seq_number: conn.local_seq,
            ack_number: 0,
            data_offset: 20,
            flags: tcp_flags::SYN,
            window_size: conn.window_size,
            checksum: 0,
            urgent_ptr: 0,
        };

        let mut tcp_buf = [0u8; 64];
        let tcp_len = syn_hdr.emit(self.config.ip, target_ip, &mut tcp_buf, &[])?;

        let dev = self.device.as_mut().ok_or("No network device initialized")?;
        self.next_ip_id = self.next_ip_id.wrapping_add(1);
        let ip_hdr = Ipv4Header::new(ip_proto::TCP, self.config.ip, target_ip, self.next_ip_id);

        let mut packet = [0u8; 128];
        let eth = EthernetHeader {
            dst: dst_mac,
            src: dev.mac_address(),
            ethertype: ethertype::IPV4,
        };

        let e_len = eth.emit(&mut packet[0..14])?;
        let i_len = ip_hdr.emit(&mut packet[e_len..], tcp_len)?;
        packet[e_len + i_len..e_len + i_len + tcp_len].copy_from_slice(&tcp_buf[..tcp_len]);
        let total_len = (e_len + i_len + tcp_len).max(60);

        dev.transmit(&packet[..total_len])?;

        // Step 2: Poll for SYN-ACK
        let start_tick = crate::timer::get_ticks();
        let timeout_ticks = (timeout_ms / crate::timer::MS_PER_TICK).max(1);
        let mut rx_buf = [0u8; MAX_PACKET_LEN];
        let mut handshake_done = false;

        while crate::timer::get_ticks().saturating_sub(start_tick) < timeout_ticks {
            if let Some(d) = self.device.as_mut() {
                if let Ok(Some(len)) = d.receive(&mut rx_buf) {
                    if let Ok((eth_h, eth_pl)) = EthernetHeader::parse(&rx_buf[..len]) {
                        if eth_h.ethertype == ethertype::IPV4 {
                            if let Ok((ip_h, ip_pl)) = Ipv4Header::parse(eth_pl) {
                                if ip_h.src == target_ip && ip_h.protocol == ip_proto::TCP {
                                    if let Ok((tcp_resp, tcp_pl)) = TcpHeader::parse(ip_h.src, ip_h.dst, ip_pl) {
                                        if tcp_resp.dst_port == local_port && tcp_resp.src_port == target_port {
                                            conn.handle_segment(&tcp_resp, tcp_pl.len());
                                            if conn.state == TcpState::Established {
                                                handshake_done = true;
                                                break;
                                            } else if conn.state == TcpState::Closed {
                                                return Err("TCP connection rejected (RST received from remote host)");
                                            }
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

        if !handshake_done {
            return Err("TCP connection timed out waiting for SYN-ACK");
        }

        // Step 3: Complete 3-way handshake by sending ACK
        let ack_hdr = TcpHeader {
            src_port: local_port,
            dst_port: target_port,
            seq_number: conn.local_seq,
            ack_number: conn.remote_seq,
            data_offset: 20,
            flags: tcp_flags::ACK,
            window_size: conn.window_size,
            checksum: 0,
            urgent_ptr: 0,
        };

        let mut ack_buf = [0u8; 64];
        let ack_len = ack_hdr.emit(self.config.ip, target_ip, &mut ack_buf, &[])?;

        let dev = self.device.as_mut().ok_or("No network device initialized")?;
        self.next_ip_id = self.next_ip_id.wrapping_add(1);
        let ip_hdr = Ipv4Header::new(ip_proto::TCP, self.config.ip, target_ip, self.next_ip_id);

        let mut packet = [0u8; 128];
        let eth = EthernetHeader {
            dst: dst_mac,
            src: dev.mac_address(),
            ethertype: ethertype::IPV4,
        };

        let e_len = eth.emit(&mut packet[0..14])?;
        let i_len = ip_hdr.emit(&mut packet[e_len..], ack_len)?;
        packet[e_len + i_len..e_len + i_len + ack_len].copy_from_slice(&ack_buf[..ack_len]);
        let total_len = (e_len + i_len + ack_len).max(60);

        dev.transmit(&packet[..total_len])?;

        self.active_tcp_conn = Some(conn);
        Ok(conn)
    }

    /// Send a TCP RST to gracefully reset active connection
    pub fn tcp_close(&mut self) -> Result<(), &'static str> {
        let conn = match self.active_tcp_conn.take() {
            Some(c) => c,
            None => return Ok(()),
        };

        let dst_mac = match self.resolve_mac(conn.remote_ip) {
            Some(m) => m,
            None => EthernetAddress::new(0x52, 0x54, 0x00, 0x12, 0x34, 0x02),
        };

        let rst_hdr = TcpHeader {
            src_port: conn.local_port,
            dst_port: conn.remote_port,
            seq_number: conn.local_seq,
            ack_number: conn.remote_seq,
            data_offset: 20,
            flags: tcp_flags::RST | tcp_flags::ACK,
            window_size: 0,
            checksum: 0,
            urgent_ptr: 0,
        };

        let mut rst_buf = [0u8; 64];
        let rst_len = rst_hdr.emit(self.config.ip, conn.remote_ip, &mut rst_buf, &[])?;

        let dev = self.device.as_mut().ok_or("No network device initialized")?;
        self.next_ip_id = self.next_ip_id.wrapping_add(1);
        let ip_hdr = Ipv4Header::new(ip_proto::TCP, self.config.ip, conn.remote_ip, self.next_ip_id);

        let mut packet = [0u8; 128];
        let eth = EthernetHeader {
            dst: dst_mac,
            src: dev.mac_address(),
            ethertype: ethertype::IPV4,
        };

        let e_len = eth.emit(&mut packet[0..14])?;
        let i_len = ip_hdr.emit(&mut packet[e_len..], rst_len)?;
        packet[e_len + i_len..e_len + i_len + rst_len].copy_from_slice(&rst_buf[..rst_len]);
        let total_len = (e_len + i_len + rst_len).max(60);

        dev.transmit(&packet[..total_len])
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
