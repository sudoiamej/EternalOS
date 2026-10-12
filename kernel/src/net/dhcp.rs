//! Dynamic Host Configuration Protocol (DHCP) Client (RFC 2131 / RFC 2132)
//!
//! Handles DHCPDISCOVER, DHCPOFFER, DHCPREQUEST, and DHCPACK over UDP ports 68 (client) and 67 (server).

use crate::net::wire::{EthernetAddress, Ipv4Address};

pub const DHCP_SERVER_PORT: u16 = 67;
pub const DHCP_CLIENT_PORT: u16 = 68;

pub const DHCP_OP_BOOTREQUEST: u8 = 1;
pub const DHCP_OP_BOOTREPLY: u8 = 2;

pub const DHCP_MAGIC_COOKIE: [u8; 4] = [99, 130, 83, 99]; // 0x63825363

pub const DHCP_MSG_DISCOVER: u8 = 1;
pub const DHCP_MSG_OFFER: u8 = 2;
pub const DHCP_MSG_REQUEST: u8 = 3;
pub const DHCP_MSG_DECLINE: u8 = 4;
pub const DHCP_MSG_ACK: u8 = 5;
pub const DHCP_MSG_NAK: u8 = 6;

// DHCP Option Codes
pub const OPT_SUBNET_MASK: u8 = 1;
pub const OPT_ROUTER: u8 = 3;
pub const OPT_DNS_SERVER: u8 = 6;
pub const OPT_REQUESTED_IP: u8 = 50;
pub const OPT_LEASE_TIME: u8 = 51;
pub const OPT_MSG_TYPE: u8 = 53;
pub const OPT_SERVER_ID: u8 = 54;
pub const OPT_PARAM_REQUEST_LIST: u8 = 55;
pub const OPT_END: u8 = 255;

/// Leased network configuration returned from a successful DHCP transaction
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DhcpLease {
    pub assigned_ip: Ipv4Address,
    pub subnet_mask: Ipv4Address,
    pub router: Ipv4Address,
    pub dns_server: Ipv4Address,
    pub server_id: Ipv4Address,
    pub lease_time_secs: u32,
}

/// Construct a DHCPDISCOVER message into `buf`. Returns packet length.
pub fn build_dhcp_discover(
    client_mac: EthernetAddress,
    xid: u32,
    buf: &mut [u8],
) -> Result<usize, &'static str> {
    if buf.len() < 300 {
        return Err("Buffer too small for DHCPDISCOVER");
    }

    buf.fill(0);

    // Opcode, hardware type (1 = Ethernet), hardware address length (6), hops (0)
    buf[0] = DHCP_OP_BOOTREQUEST;
    buf[1] = 1; // 10Mb Ethernet
    buf[2] = 6; // MAC length
    buf[3] = 0; // Hops

    // Transaction ID (xid)
    buf[4..8].copy_from_slice(&xid.to_be_bytes());

    // Seconds elapsed (0), Flags: Broadcast flag (0x8000)
    buf[8..10].copy_from_slice(&[0, 0]);
    buf[10..12].copy_from_slice(&[0x80, 0x00]); // Broadcast response requested

    // Client IP (ciaddr), Your IP (yiaddr), Server IP (siaddr), Gateway IP (giaddr) all 0
    // (already zeroed)

    // Client hardware address (chaddr) at offset 28..44 (16 bytes, MAC in first 6)
    buf[28..34].copy_from_slice(&client_mac.0);

    // Magic cookie at offset 236..240
    buf[236..240].copy_from_slice(&DHCP_MAGIC_COOKIE);

    // Options start at offset 240
    let mut off = 240;

    // Option 53: DHCP Message Type = DHCPDISCOVER (1)
    buf[off] = OPT_MSG_TYPE;
    buf[off + 1] = 1;
    buf[off + 2] = DHCP_MSG_DISCOVER;
    off += 3;

    // Option 55: Parameter Request List (1 = Subnet Mask, 3 = Router, 6 = DNS)
    buf[off] = OPT_PARAM_REQUEST_LIST;
    buf[off + 1] = 3;
    buf[off + 2] = OPT_SUBNET_MASK;
    buf[off + 3] = OPT_ROUTER;
    buf[off + 4] = OPT_DNS_SERVER;
    off += 5;

    // Option 255: End
    buf[off] = OPT_END;
    off += 1;

    // Pad to minimum BOOTP payload length (at least 300 bytes)
    let total_len = off.max(300);
    Ok(total_len)
}

/// Construct a DHCPREQUEST message for offered IP. Returns packet length.
pub fn build_dhcp_request(
    client_mac: EthernetAddress,
    xid: u32,
    requested_ip: Ipv4Address,
    server_id: Ipv4Address,
    buf: &mut [u8],
) -> Result<usize, &'static str> {
    if buf.len() < 300 {
        return Err("Buffer too small for DHCPREQUEST");
    }

    buf.fill(0);

    buf[0] = DHCP_OP_BOOTREQUEST;
    buf[1] = 1;
    buf[2] = 6;
    buf[3] = 0;

    buf[4..8].copy_from_slice(&xid.to_be_bytes());
    buf[10..12].copy_from_slice(&[0x80, 0x00]); // Broadcast response requested

    buf[28..34].copy_from_slice(&client_mac.0);
    buf[236..240].copy_from_slice(&DHCP_MAGIC_COOKIE);

    let mut off = 240;

    // Option 53: DHCP Message Type = DHCPREQUEST (3)
    buf[off] = OPT_MSG_TYPE;
    buf[off + 1] = 1;
    buf[off + 2] = DHCP_MSG_REQUEST;
    off += 3;

    // Option 50: Requested IP
    buf[off] = OPT_REQUESTED_IP;
    buf[off + 1] = 4;
    buf[off + 2..off + 6].copy_from_slice(&requested_ip.0);
    off += 6;

    // Option 54: Server Identifier
    buf[off] = OPT_SERVER_ID;
    buf[off + 1] = 4;
    buf[off + 2..off + 6].copy_from_slice(&server_id.0);
    off += 6;

    // Option 55: Parameter Request List
    buf[off] = OPT_PARAM_REQUEST_LIST;
    buf[off + 1] = 3;
    buf[off + 2] = OPT_SUBNET_MASK;
    buf[off + 3] = OPT_ROUTER;
    buf[off + 4] = OPT_DNS_SERVER;
    off += 5;

    // Option 255: End
    buf[off] = OPT_END;
    off += 1;

    let total_len = off.max(300);
    Ok(total_len)
}

/// Parse a DHCP reply message (DHCPOFFER or DHCPACK)
pub fn parse_dhcp_reply(
    buf: &[u8],
    expected_xid: u32,
    expected_msg_type: u8,
) -> Result<DhcpLease, &'static str> {
    if buf.len() < 240 {
        return Err("DHCP response packet too short");
    }

    if buf[0] != DHCP_OP_BOOTREPLY {
        return Err("Packet is not a BOOTREPLY");
    }

    let xid = u32::from_be_bytes([buf[4], buf[5], buf[6], buf[7]]);
    if xid != expected_xid {
        return Err("DHCP transaction ID mismatch");
    }

    // Your (Client) IP Address (yiaddr) at offset 16..20
    let yiaddr = Ipv4Address::new(buf[16], buf[17], buf[18], buf[19]);

    // Check magic cookie
    if buf[236..240] != DHCP_MAGIC_COOKIE {
        return Err("Invalid DHCP magic cookie in response");
    }

    let mut msg_type = None;
    let mut subnet_mask = None;
    let mut router = None;
    let mut dns_server = None;
    let mut server_id = None;
    let mut lease_time = 3600u32; // Default 1 hour

    let mut off = 240;
    while off < buf.len() {
        let opt = buf[off];
        if opt == OPT_END {
            break;
        }
        if opt == 0 {
            // Padding option
            off += 1;
            continue;
        }

        if off + 1 >= buf.len() {
            return Err("Truncated DHCP option header");
        }
        let len = buf[off + 1] as usize;
        off += 2;

        if off + len > buf.len() {
            return Err("Truncated DHCP option payload");
        }

        let opt_data = &buf[off..off + len];
        match opt {
            OPT_MSG_TYPE => {
                if len >= 1 {
                    msg_type = Some(opt_data[0]);
                }
            }
            OPT_SUBNET_MASK => {
                if len >= 4 {
                    subnet_mask = Some(Ipv4Address::new(opt_data[0], opt_data[1], opt_data[2], opt_data[3]));
                }
            }
            OPT_ROUTER => {
                if len >= 4 {
                    router = Some(Ipv4Address::new(opt_data[0], opt_data[1], opt_data[2], opt_data[3]));
                }
            }
            OPT_DNS_SERVER => {
                if len >= 4 {
                    dns_server = Some(Ipv4Address::new(opt_data[0], opt_data[1], opt_data[2], opt_data[3]));
                }
            }
            OPT_SERVER_ID => {
                if len >= 4 {
                    server_id = Some(Ipv4Address::new(opt_data[0], opt_data[1], opt_data[2], opt_data[3]));
                }
            }
            OPT_LEASE_TIME => {
                if len >= 4 {
                    lease_time = u32::from_be_bytes([opt_data[0], opt_data[1], opt_data[2], opt_data[3]]);
                }
            }
            _ => {}
        }

        off += len;
    }

    if msg_type != Some(expected_msg_type) {
        return Err("DHCP message type does not match expected offer/ack");
    }

    if yiaddr.is_unspecified() {
        return Err("DHCP server offered unspecified (0.0.0.0) IP address");
    }

    Ok(DhcpLease {
        assigned_ip: yiaddr,
        subnet_mask: subnet_mask.unwrap_or(Ipv4Address::new(255, 255, 255, 0)),
        router: router.unwrap_or(Ipv4Address::new(10, 0, 2, 2)),
        dns_server: dns_server.unwrap_or(Ipv4Address::new(10, 0, 2, 3)),
        server_id: server_id.unwrap_or(Ipv4Address::new(10, 0, 2, 2)),
        lease_time_secs: lease_time,
    })
}
