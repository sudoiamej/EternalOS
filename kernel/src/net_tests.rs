use crate::net::arp::{ArpCache, ArpPacket, ARP_OP_REQUEST};
use crate::net::dns::{build_dns_query, parse_dns_response};
use crate::net::icmp::{IcmpHeader, ICMP_TYPE_ECHO_REQUEST};
use crate::net::ipv4::Ipv4Header;
use crate::net::tcp::{tcp_flags, TcpConnection, TcpHeader, TcpState};
use crate::net::udp::UdpHeader;
use crate::net::wire::{ethertype, ip_proto, EthernetAddress, EthernetHeader, Ipv4Address};
use crate::net::NetworkDevice;
use crate::{log_info, println};

/// Automated test suite for the EternalOS Networking Stack
pub fn run_net_tests() {
    log_info!("====================================================");
    log_info!("     RUNNING AUTOMATED TEST SUITE: NETWORKING STACK ");
    log_info!("====================================================");

    // 1. Wire & Ethernet Frame Tests
    test_ethernet_wire();

    // 2. ARP Packet & Cache Tests
    test_arp_wire_and_cache();

    // 3. IPv4 Header & Checksum Tests
    test_ipv4_wire_and_checksum();

    // 4. ICMP Echo Request / Reply Tests
    test_icmp_wire();

    // 5. UDP Header & Pseudo Checksum Tests
    test_udp_wire_and_checksum();

    // 6. TCP Header, Flags & State Machine Foundation Tests
    test_tcp_wire_and_state();

    // 7. DNS Query Construction & Response Parser Tests
    test_dns_wire();

    // 8. DHCP Client Serialization & Option Tests
    test_dhcp_wire();

    // 9. Live Network Device & Hardware Connectivity Tests (E1000 + QEMU slirp)
    test_live_network_connectivity();

    log_info!("====================================================");
    log_info!("  ALL NETWORKING SUBSYSTEM TESTS PASSED CLEANLY!    ");
    log_info!("====================================================");
    println!(">>> NET TEST PASS: Ethernet, ARP, IPv4, ICMP, UDP, TCP, DNS, DHCP & E1000 verified.");
}

fn test_ethernet_wire() {
    log_info!("[TEST 1/8] Testing Ethernet Framing & Serialization...");

    let src = EthernetAddress::new(0x52, 0x54, 0x00, 0x12, 0x34, 0x56);
    let dst = EthernetAddress::BROADCAST;

    assert!(dst.is_broadcast());
    assert!(!src.is_broadcast());
    assert!(src.is_unicast());

    let header = EthernetHeader {
        dst,
        src,
        ethertype: ethertype::IPV4,
    };

    let mut buf = [0u8; 14];
    let len = header.emit(&mut buf).expect("Ethernet emit failed");
    assert_eq!(len, 14);

    let (parsed, remaining) = EthernetHeader::parse(&buf).expect("Ethernet parse failed");
    assert_eq!(parsed.dst, dst);
    assert_eq!(parsed.src, src);
    assert_eq!(parsed.ethertype, ethertype::IPV4);
    assert!(remaining.is_empty());

    // Truncated buffer test
    let short_buf = [0u8; 10];
    assert!(EthernetHeader::parse(&short_buf).is_err());

    log_info!("  [PASS] Ethernet frame serialization, parsing, and boundary checks verified.");
}

fn test_arp_wire_and_cache() {
    log_info!("[TEST 2/8] Testing ARP Packet Handling & Cache...");

    let smac = EthernetAddress::new(0x52, 0x54, 0x00, 0x12, 0x34, 0x56);
    let sip = Ipv4Address::new(10, 0, 2, 15);
    let tip = Ipv4Address::new(10, 0, 2, 2);

    let req = ArpPacket::new_request(smac, sip, tip);
    let mut buf = [0u8; 28];
    let len = req.emit(&mut buf).expect("ARP emit failed");
    assert_eq!(len, 28);

    let parsed = ArpPacket::parse(&buf).expect("ARP parse failed");
    assert_eq!(parsed.opcode, ARP_OP_REQUEST);
    assert_eq!(parsed.sender_mac, smac);
    assert_eq!(parsed.sender_ip, sip);
    assert_eq!(parsed.target_ip, tip);

    // Test Bounded ARP Cache
    let mut cache = ArpCache::new();
    assert_eq!(cache.lookup(tip), None);

    let gw_mac = EthernetAddress::new(0x52, 0x54, 0x00, 0x12, 0x34, 0x02);
    cache.insert(tip, gw_mac, 100);
    assert_eq!(cache.lookup(tip), Some(gw_mac));

    // Update existing entry
    let gw_mac_updated = EthernetAddress::new(0x52, 0x54, 0x00, 0x12, 0x34, 0x03);
    cache.insert(tip, gw_mac_updated, 200);
    assert_eq!(cache.lookup(tip), Some(gw_mac_updated));

    log_info!("  [PASS] ARP packet encoding, decoding, and bounded cache verified.");
}

fn test_ipv4_wire_and_checksum() {
    log_info!("[TEST 3/8] Testing IPv4 Header & Checksum Generation...");

    let src = Ipv4Address::new(10, 0, 2, 15);
    let dst = Ipv4Address::new(10, 0, 2, 2);
    let hdr = Ipv4Header::new(ip_proto::UDP, src, dst, 0x42);

    let payload = b"Hello IPv4 payload";
    let mut packet = [0u8; 128];
    let hdr_len = hdr.emit(&mut packet, payload.len()).expect("IPv4 emit failed");
    assert_eq!(hdr_len, 20);
    packet[20..20 + payload.len()].copy_from_slice(payload);

    let total_len = hdr_len + payload.len();
    let (parsed_hdr, parsed_payload) = Ipv4Header::parse(&packet[..total_len]).expect("IPv4 parse failed");
    assert_eq!(parsed_hdr.version, 4);
    assert_eq!(parsed_hdr.ihl, 5);
    assert_eq!(parsed_hdr.src, src);
    assert_eq!(parsed_hdr.dst, dst);
    assert_eq!(parsed_hdr.protocol, ip_proto::UDP);
    assert_eq!(parsed_payload, payload);

    // Corrupted checksum test
    packet[10] ^= 0xFF;
    assert!(Ipv4Header::parse(&packet[..total_len]).is_err(), "Corrupted checksum must be rejected");

    log_info!("  [PASS] IPv4 header encoding, verification, and corruption rejection verified.");
}

fn test_icmp_wire() {
    log_info!("[TEST 4/8] Testing ICMP Echo Request / Reply...");

    let icmp = IcmpHeader::new_echo_request(0x1234, 1);
    let payload = b"PingTest";
    let mut buf = [0u8; 64];
    let len = icmp.emit(&mut buf, payload).expect("ICMP emit failed");
    assert_eq!(len, 8 + payload.len());

    let (parsed, parsed_payload) = IcmpHeader::parse(&buf[..len]).expect("ICMP parse failed");
    assert_eq!(parsed.msg_type, ICMP_TYPE_ECHO_REQUEST);
    assert_eq!(parsed.id, 0x1234);
    assert_eq!(parsed.sequence, 1);
    assert_eq!(parsed_payload, payload);

    // Corrupted ICMP test
    buf[2] ^= 0x55;
    assert!(IcmpHeader::parse(&buf[..len]).is_err(), "Corrupted ICMP checksum must be rejected");

    log_info!("  [PASS] ICMP Echo header encoding, checksumming, and validation verified.");
}

fn test_udp_wire_and_checksum() {
    log_info!("[TEST 5/8] Testing UDP Header & Pseudo Checksum...");

    let src_ip = Ipv4Address::new(10, 0, 2, 15);
    let dst_ip = Ipv4Address::new(10, 0, 2, 3);
    let udp = UdpHeader::new(53535, 53);
    let payload = b"DNS Query Payload";

    let mut buf = [0u8; 128];
    let len = udp.emit(src_ip, dst_ip, &mut buf, payload).expect("UDP emit failed");
    assert_eq!(len, 8 + payload.len());

    let (parsed, parsed_payload) = UdpHeader::parse(src_ip, dst_ip, &buf[..len]).expect("UDP parse failed");
    assert_eq!(parsed.src_port, 53535);
    assert_eq!(parsed.dst_port, 53);
    assert_eq!(parsed_payload, payload);

    // Checksum verification failure test
    buf[6] ^= 0xAA;
    assert!(UdpHeader::parse(src_ip, dst_ip, &buf[..len]).is_err(), "Corrupted UDP checksum must be rejected");

    log_info!("  [PASS] UDP datagram emission, pseudo-checksum, and integrity verification verified.");
}

fn test_tcp_wire_and_state() {
    log_info!("[TEST 6/8] Testing TCP Foundation & State Machine...");

    let src_ip = Ipv4Address::new(10, 0, 2, 15);
    let dst_ip = Ipv4Address::new(10, 0, 2, 2);

    let tcp_hdr = TcpHeader {
        src_port: 12345,
        dst_port: 80,
        seq_number: 1000,
        ack_number: 0,
        data_offset: 20,
        flags: tcp_flags::SYN,
        window_size: 4096,
        checksum: 0,
        urgent_ptr: 0,
    };

    let mut buf = [0u8; 64];
    let len = tcp_hdr.emit(src_ip, dst_ip, &mut buf, &[]).expect("TCP emit failed");
    assert_eq!(len, 20);

    let (parsed, payload) = TcpHeader::parse(src_ip, dst_ip, &buf[..len]).expect("TCP parse failed");
    assert_eq!(parsed.src_port, 12345);
    assert_eq!(parsed.dst_port, 80);
    assert_eq!(parsed.flags, tcp_flags::SYN);
    assert!(payload.is_empty());

    // Test TCP State Machine: Handshake progression
    let mut conn = TcpConnection::new();
    conn.state = TcpState::SynSent;
    conn.local_port = 12345;
    conn.remote_ip = dst_ip;
    conn.remote_port = 80;

    let syn_ack_hdr = TcpHeader {
        src_port: 80,
        dst_port: 12345,
        seq_number: 5000,
        ack_number: 1001,
        data_offset: 20,
        flags: tcp_flags::SYN | tcp_flags::ACK,
        window_size: 4096,
        checksum: 0,
        urgent_ptr: 0,
    };

    conn.handle_segment(&syn_ack_hdr, 0);
    assert_eq!(conn.state, TcpState::Established);
    assert_eq!(conn.remote_seq, 5001);
    assert_eq!(conn.local_seq, 1001);

    // Teardown: FIN handling
    let fin_hdr = TcpHeader {
        src_port: 80,
        dst_port: 12345,
        seq_number: 5001,
        ack_number: 1001,
        data_offset: 20,
        flags: tcp_flags::FIN | tcp_flags::ACK,
        window_size: 4096,
        checksum: 0,
        urgent_ptr: 0,
    };

    conn.handle_segment(&fin_hdr, 0);
    assert_eq!(conn.state, TcpState::CloseWait);

    log_info!("  [PASS] TCP segment serialization, checksum verification, and handshake state machine verified.");
}

fn test_dns_wire() {
    log_info!("[TEST 7/8] Testing DNS Query Construction & Response Parser...");

    let mut query_buf = [0u8; 128];
    let qlen = build_dns_query(0x5678, "example.com", &mut query_buf).expect("DNS query build failed");
    assert!(qlen > 12);

    // Validate synthetic DNS response parsing
    let mut resp = [0u8; 64];
    resp[0..2].copy_from_slice(&0x5678u16.to_be_bytes()); // ID = 0x5678
    resp[2..4].copy_from_slice(&0x8180u16.to_be_bytes()); // Standard response, no error
    resp[4..6].copy_from_slice(&1u16.to_be_bytes());      // 1 Question
    resp[6..8].copy_from_slice(&1u16.to_be_bytes());      // 1 Answer
    resp[8..10].copy_from_slice(&0u16.to_be_bytes());     // 0 NS
    resp[10..12].copy_from_slice(&0u16.to_be_bytes());    // 0 AR

    // Question section: \x07example\x03com\x00 (13 bytes) + QTYPE (2) + QCLASS (2) = 17 bytes
    let mut off = 12;
    resp[off] = 7; off += 1;
    resp[off..off + 7].copy_from_slice(b"example"); off += 7;
    resp[off] = 3; off += 1;
    resp[off..off + 3].copy_from_slice(b"com"); off += 3;
    resp[off] = 0; off += 1;
    resp[off..off + 2].copy_from_slice(&1u16.to_be_bytes()); off += 2; // Type A
    resp[off..off + 2].copy_from_slice(&1u16.to_be_bytes()); off += 2; // Class IN

    // Answer section: Compression pointer (0xC00C) + TYPE (2) + CLASS (2) + TTL (4) + RDLEN (2) + RDATA (4) = 16 bytes
    resp[off..off + 2].copy_from_slice(&[0xC0, 0x0C]); off += 2; // Pointer to offset 12
    resp[off..off + 2].copy_from_slice(&1u16.to_be_bytes()); off += 2; // Type A
    resp[off..off + 2].copy_from_slice(&1u16.to_be_bytes()); off += 2; // Class IN
    resp[off..off + 4].copy_from_slice(&300u32.to_be_bytes()); off += 4; // TTL 300
    resp[off..off + 2].copy_from_slice(&4u16.to_be_bytes()); off += 2; // Length 4
    resp[off..off + 4].copy_from_slice(&[93, 184, 216, 34]); off += 4; // 93.184.216.34

    let resolved_ip = parse_dns_response(&resp[..off], 0x5678).expect("DNS response parse failed");
    assert_eq!(resolved_ip, Ipv4Address::new(93, 184, 216, 34));

    log_info!("  [PASS] DNS query building and compressed response resolution verified.");
}

fn test_dhcp_wire() {
    log_info!("[TEST 8/9] Testing DHCP Discovery & Request Packet Building...");

    let mac = EthernetAddress::new(0x52, 0x54, 0x00, 0x12, 0x34, 0x56);
    let xid = 0x12345678;

    let mut disc_buf = [0u8; 320];
    let disc_len = crate::net::dhcp::build_dhcp_discover(mac, xid, &mut disc_buf)
        .expect("build_dhcp_discover failed");
    assert!(disc_len >= 300);
    assert_eq!(disc_buf[0], crate::net::dhcp::DHCP_OP_BOOTREQUEST);
    assert_eq!(&disc_buf[4..8], &xid.to_be_bytes());
    assert_eq!(&disc_buf[28..34], &mac.0);
    assert_eq!(&disc_buf[236..240], &crate::net::dhcp::DHCP_MAGIC_COOKIE);

    let req_ip = Ipv4Address::new(10, 0, 2, 15);
    let srv_id = Ipv4Address::new(10, 0, 2, 2);
    let mut req_buf = [0u8; 320];
    let req_len = crate::net::dhcp::build_dhcp_request(mac, xid, req_ip, srv_id, &mut req_buf)
        .expect("build_dhcp_request failed");
    assert!(req_len >= 300);
    assert_eq!(req_buf[0], crate::net::dhcp::DHCP_OP_BOOTREQUEST);

    log_info!("  [PASS] DHCP discovery and request serialization verified.");
}

fn test_live_network_connectivity() {
    log_info!("[TEST 9/9] Testing Live Network Device & Hardware Connectivity (E1000)...");

    let has_device = crate::net::with_network_stack(|stack| stack.device.is_some());
    assert!(has_device, "Intel E1000 NIC must be detected and initialized on PCI bus");

    crate::net::with_network_stack(|stack| {
        let dev = stack.device.as_ref().unwrap();
        let mac = dev.mac_address();
        log_info!(
            "  Detected NIC MAC: {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}, Link: {}",
            mac.0[0], mac.0[1], mac.0[2], mac.0[3], mac.0[4], mac.0[5],
            if dev.is_link_up() { "UP" } else { "DOWN" }
        );
        assert_ne!(mac, EthernetAddress::ZERO);
    });

    // Send ARP request to QEMU gateway (10.0.2.2)
    let gw_ip = Ipv4Address::new(10, 0, 2, 2);
    let arp_res = crate::net::with_network_stack(|stack| stack.send_arp_request(gw_ip));
    assert!(arp_res.is_ok(), "ARP request transmission failed");
    log_info!("  [PASS] ARP request successfully transmitted over E1000 DMA ring.");

    // Send ICMP Echo Request to QEMU gateway (10.0.2.2)
    let ping_res = crate::net::with_network_stack(|stack| stack.send_icmp_ping(gw_ip, 1));
    assert!(ping_res.is_ok(), "ICMP ping transmission failed");
    log_info!("  [PASS] ICMP Echo Request successfully transmitted to Gateway (10.0.2.2).");

    // Inspect packet counters
    crate::net::with_network_stack(|stack| {
        let stats = stack.device.as_ref().unwrap().stats();
        log_info!("  Device Packet Statistics: TX Packets={}, TX Bytes={}", stats.tx_packets, stats.tx_bytes);
        assert!(stats.tx_packets >= 2, "At least 2 packets must have been transmitted");
    });

    log_info!("  [PASS] Live hardware device DMA transmission and network counters verified.");
}
