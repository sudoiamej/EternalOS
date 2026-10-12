use crate::net::wire::{EthernetAddress, Ipv4Address};

pub const ARP_HTYPE_ETHERNET: u16 = 1;
pub const ARP_PTYPE_IPV4: u16 = 0x0800;
pub const ARP_OP_REQUEST: u16 = 1;
pub const ARP_OP_REPLY: u16 = 2;
pub const ARP_PACKET_LEN: usize = 28;

#[derive(Debug, Clone, Copy)]
pub struct ArpPacket {
    pub htype: u16,
    pub ptype: u16,
    pub hlen: u8,
    pub plen: u8,
    pub opcode: u16,
    pub sender_mac: EthernetAddress,
    pub sender_ip: Ipv4Address,
    pub target_mac: EthernetAddress,
    pub target_ip: Ipv4Address,
}

impl ArpPacket {
    pub fn parse(buf: &[u8]) -> Result<Self, &'static str> {
        if buf.len() < ARP_PACKET_LEN {
            return Err("ARP packet buffer too short");
        }
        let htype = u16::from_be_bytes([buf[0], buf[1]]);
        let ptype = u16::from_be_bytes([buf[2], buf[3]]);
        let hlen = buf[4];
        let plen = buf[5];
        let opcode = u16::from_be_bytes([buf[6], buf[7]]);

        if htype != ARP_HTYPE_ETHERNET || ptype != ARP_PTYPE_IPV4 || hlen != 6 || plen != 4 {
            return Err("Unsupported ARP hardware or protocol type");
        }

        let mut smac = [0u8; 6];
        smac.copy_from_slice(&buf[8..14]);
        let mut sip = [0u8; 4];
        sip.copy_from_slice(&buf[14..18]);

        let mut tmac = [0u8; 6];
        tmac.copy_from_slice(&buf[18..24]);
        let mut tip = [0u8; 4];
        tip.copy_from_slice(&buf[24..28]);

        Ok(Self {
            htype,
            ptype,
            hlen,
            plen,
            opcode,
            sender_mac: EthernetAddress(smac),
            sender_ip: Ipv4Address(sip),
            target_mac: EthernetAddress(tmac),
            target_ip: Ipv4Address(tip),
        })
    }

    pub fn emit(&self, buf: &mut [u8]) -> Result<usize, &'static str> {
        if buf.len() < ARP_PACKET_LEN {
            return Err("Buffer too small for ARP packet");
        }
        buf[0..2].copy_from_slice(&self.htype.to_be_bytes());
        buf[2..4].copy_from_slice(&self.ptype.to_be_bytes());
        buf[4] = self.hlen;
        buf[5] = self.plen;
        buf[6..8].copy_from_slice(&self.opcode.to_be_bytes());
        buf[8..14].copy_from_slice(&self.sender_mac.0);
        buf[14..18].copy_from_slice(&self.sender_ip.0);
        buf[18..24].copy_from_slice(&self.target_mac.0);
        buf[24..28].copy_from_slice(&self.target_ip.0);
        Ok(ARP_PACKET_LEN)
    }

    pub fn new_request(sender_mac: EthernetAddress, sender_ip: Ipv4Address, target_ip: Ipv4Address) -> Self {
        Self {
            htype: ARP_HTYPE_ETHERNET,
            ptype: ARP_PTYPE_IPV4,
            hlen: 6,
            plen: 4,
            opcode: ARP_OP_REQUEST,
            sender_mac,
            sender_ip,
            target_mac: EthernetAddress::ZERO,
            target_ip,
        }
    }

    pub fn new_reply(
        sender_mac: EthernetAddress,
        sender_ip: Ipv4Address,
        target_mac: EthernetAddress,
        target_ip: Ipv4Address,
    ) -> Self {
        Self {
            htype: ARP_HTYPE_ETHERNET,
            ptype: ARP_PTYPE_IPV4,
            hlen: 6,
            plen: 4,
            opcode: ARP_OP_REPLY,
            sender_mac,
            sender_ip,
            target_mac,
            target_ip,
        }
    }
}

pub const ARP_CACHE_CAPACITY: usize = 16;

#[derive(Debug, Clone, Copy)]
pub struct ArpEntry {
    pub ip: Ipv4Address,
    pub mac: EthernetAddress,
    pub updated_tick: u64,
    pub valid: bool,
}

impl ArpEntry {
    pub const fn empty() -> Self {
        Self {
            ip: Ipv4Address([0; 4]),
            mac: EthernetAddress([0; 6]),
            updated_tick: 0,
            valid: false,
        }
    }
}

/// Bounded LRU/FIFO ARP cache
pub struct ArpCache {
    pub entries: [ArpEntry; ARP_CACHE_CAPACITY],
}

impl ArpCache {
    pub const fn new() -> Self {
        Self {
            entries: [ArpEntry::empty(); ARP_CACHE_CAPACITY],
        }
    }

    pub fn lookup(&self, ip: Ipv4Address) -> Option<EthernetAddress> {
        for entry in self.entries.iter() {
            if entry.valid && entry.ip == ip {
                return Some(entry.mac);
            }
        }
        None
    }

    pub fn insert(&mut self, ip: Ipv4Address, mac: EthernetAddress, tick: u64) {
        // If already exists, update entry
        for entry in self.entries.iter_mut() {
            if entry.valid && entry.ip == ip {
                entry.mac = mac;
                entry.updated_tick = tick;
                return;
            }
        }

        // Find empty slot
        for entry in self.entries.iter_mut() {
            if !entry.valid {
                *entry = ArpEntry {
                    ip,
                    mac,
                    updated_tick: tick,
                    valid: true,
                };
                return;
            }
        }

        // Overwrite oldest entry
        let mut oldest_idx = 0;
        let mut oldest_tick = self.entries[0].updated_tick;
        for (i, entry) in self.entries.iter().enumerate() {
            if entry.updated_tick < oldest_tick {
                oldest_tick = entry.updated_tick;
                oldest_idx = i;
            }
        }

        self.entries[oldest_idx] = ArpEntry {
            ip,
            mac,
            updated_tick: tick,
            valid: true,
        };
    }
}
