/// MAC address (6 bytes)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct EthernetAddress(pub [u8; 6]);

impl EthernetAddress {
    pub const BROADCAST: Self = Self([0xFF; 6]);
    pub const ZERO: Self = Self([0x00; 6]);

    pub const fn new(b0: u8, b1: u8, b2: u8, b3: u8, b4: u8, b5: u8) -> Self {
        Self([b0, b1, b2, b3, b4, b5])
    }

    pub fn is_broadcast(&self) -> bool {
        self.0 == [0xFF; 6]
    }

    pub fn is_multicast(&self) -> bool {
        (self.0[0] & 0x01) != 0
    }

    pub fn is_unicast(&self) -> bool {
        !self.is_multicast()
    }
}

/// IPv4 address (4 bytes)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Ipv4Address(pub [u8; 4]);

impl Ipv4Address {
    pub const BROADCAST: Self = Self([255, 255, 255, 255]);
    pub const UNSPECIFIED: Self = Self([0, 0, 0, 0]);
    pub const LOCALHOST: Self = Self([127, 0, 0, 1]);

    pub const fn new(a: u8, b: u8, c: u8, d: u8) -> Self {
        Self([a, b, c, d])
    }

    pub fn as_u32(&self) -> u32 {
        u32::from_be_bytes(self.0)
    }

    pub fn from_u32(val: u32) -> Self {
        Self(val.to_be_bytes())
    }

    pub fn is_broadcast(&self) -> bool {
        self.0 == [255, 255, 255, 255]
    }

    pub fn is_unspecified(&self) -> bool {
        self.0 == [0, 0, 0, 0]
    }

    pub fn is_multicast(&self) -> bool {
        (self.0[0] >= 224) && (self.0[0] <= 239)
    }

    /// Check if target IP matches local network subnet
    pub fn is_same_subnet(&self, other: &Self, mask: &Self) -> bool {
        let s = self.as_u32();
        let o = other.as_u32();
        let m = mask.as_u32();
        (s & m) == (o & m)
    }
}

/// EtherType constants
pub mod ethertype {
    pub const IPV4: u16 = 0x0800;
    pub const ARP: u16 = 0x0806;
    pub const IPV6: u16 = 0x86DD;
}

/// IP Protocol numbers
pub mod ip_proto {
    pub const ICMP: u8 = 1;
    pub const TCP: u8 = 6;
    pub const UDP: u8 = 17;
}

/// 14-byte Ethernet II header
pub const ETHERNET_HEADER_LEN: usize = 14;

#[derive(Debug, Clone, Copy)]
pub struct EthernetHeader {
    pub dst: EthernetAddress,
    pub src: EthernetAddress,
    pub ethertype: u16,
}

impl EthernetHeader {
    pub fn parse(buf: &[u8]) -> Result<(Self, &[u8]), &'static str> {
        if buf.len() < ETHERNET_HEADER_LEN {
            return Err("Ethernet frame too short for header");
        }
        let mut dst = [0u8; 6];
        let mut src = [0u8; 6];
        dst.copy_from_slice(&buf[0..6]);
        src.copy_from_slice(&buf[6..12]);
        let ethertype = u16::from_be_bytes([buf[12], buf[13]]);

        Ok((
            Self {
                dst: EthernetAddress(dst),
                src: EthernetAddress(src),
                ethertype,
            },
            &buf[ETHERNET_HEADER_LEN..],
        ))
    }

    pub fn emit(&self, buf: &mut [u8]) -> Result<usize, &'static str> {
        if buf.len() < ETHERNET_HEADER_LEN {
            return Err("Buffer too small for Ethernet header");
        }
        buf[0..6].copy_from_slice(&self.dst.0);
        buf[6..12].copy_from_slice(&self.src.0);
        let et_bytes = self.ethertype.to_be_bytes();
        buf[12..14].copy_from_slice(&et_bytes);
        Ok(ETHERNET_HEADER_LEN)
    }
}

/// Standard Internet 16-bit One's Complement Checksum
pub fn compute_checksum(data: &[u8]) -> u16 {
    let mut sum: u32 = 0;
    let mut chunks = data.chunks_exact(2);
    for chunk in chunks.by_ref() {
        let word = u16::from_be_bytes([chunk[0], chunk[1]]);
        sum = sum.wrapping_add(word as u32);
    }
    let remainder = chunks.remainder();
    if !remainder.is_empty() {
        let word = (remainder[0] as u32) << 8;
        sum = sum.wrapping_add(word);
    }
    while (sum >> 16) != 0 {
        sum = (sum & 0xFFFF) + (sum >> 16);
    }
    !(sum as u16)
}

/// Compute checksum for a pseudo-header and payload (used by TCP and UDP)
pub fn compute_pseudo_checksum(src_ip: Ipv4Address, dst_ip: Ipv4Address, proto: u8, length: u16, payload: &[u8]) -> u16 {
    let mut sum: u32 = 0;

    // Pseudo header: Src (4) + Dst (4) + zero (1) + proto (1) + len (2)
    let src_u32 = src_ip.as_u32();
    let dst_u32 = dst_ip.as_u32();

    sum = sum.wrapping_add(src_u32 >> 16);
    sum = sum.wrapping_add(src_u32 & 0xFFFF);
    sum = sum.wrapping_add(dst_u32 >> 16);
    sum = sum.wrapping_add(dst_u32 & 0xFFFF);
    sum = sum.wrapping_add(proto as u32);
    sum = sum.wrapping_add(length as u32);

    let mut chunks = payload.chunks_exact(2);
    for chunk in chunks.by_ref() {
        let word = u16::from_be_bytes([chunk[0], chunk[1]]);
        sum = sum.wrapping_add(word as u32);
    }
    let remainder = chunks.remainder();
    if !remainder.is_empty() {
        let word = (remainder[0] as u32) << 8;
        sum = sum.wrapping_add(word);
    }
    while (sum >> 16) != 0 {
        sum = (sum & 0xFFFF) + (sum >> 16);
    }
    !(sum as u16)
}
