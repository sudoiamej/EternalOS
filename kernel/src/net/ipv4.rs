use crate::net::wire::{compute_checksum, Ipv4Address};

pub const IPV4_MIN_HEADER_LEN: usize = 20;

#[derive(Debug, Clone, Copy)]
pub struct Ipv4Header {
    pub version: u8,
    pub ihl: u8,
    pub dscp: u8,
    pub ecn: u8,
    pub total_length: u16,
    pub identification: u16,
    pub flags: u8,
    pub fragment_offset: u16,
    pub ttl: u8,
    pub protocol: u8,
    pub checksum: u16,
    pub src: Ipv4Address,
    pub dst: Ipv4Address,
}

impl Ipv4Header {
    pub fn parse(buf: &[u8]) -> Result<(Self, &[u8]), &'static str> {
        if buf.len() < IPV4_MIN_HEADER_LEN {
            return Err("IPv4 buffer smaller than minimum header");
        }

        let ver_ihl = buf[0];
        let version = ver_ihl >> 4;
        let ihl = ver_ihl & 0x0F;

        if version != 4 {
            return Err("Not an IPv4 packet");
        }

        let header_len = (ihl as usize) * 4;
        if header_len < IPV4_MIN_HEADER_LEN || buf.len() < header_len {
            return Err("Invalid IPv4 header length");
        }

        let total_length = u16::from_be_bytes([buf[2], buf[3]]);
        if (total_length as usize) < header_len || buf.len() < (total_length as usize) {
            return Err("IPv4 packet truncated according to total_length");
        }

        // Verify header checksum
        let calculated_cksum = compute_checksum(&buf[..header_len]);
        if calculated_cksum != 0 {
            return Err("IPv4 header checksum invalid");
        }

        let tos = buf[1];
        let dscp = tos >> 2;
        let ecn = tos & 0x03;
        let identification = u16::from_be_bytes([buf[4], buf[5]]);
        let flags_frag = u16::from_be_bytes([buf[6], buf[7]]);
        let flags = ((flags_frag >> 13) & 0x07) as u8;
        let fragment_offset = flags_frag & 0x1FFF;

        let ttl = buf[8];
        let protocol = buf[9];
        let checksum = u16::from_be_bytes([buf[10], buf[11]]);

        let mut src = [0u8; 4];
        src.copy_from_slice(&buf[12..16]);
        let mut dst = [0u8; 4];
        dst.copy_from_slice(&buf[16..20]);

        let payload = &buf[header_len..(total_length as usize)];

        Ok((
            Self {
                version,
                ihl,
                dscp,
                ecn,
                total_length,
                identification,
                flags,
                fragment_offset,
                ttl,
                protocol,
                checksum,
                src: Ipv4Address(src),
                dst: Ipv4Address(dst),
            },
            payload,
        ))
    }

    pub fn emit(&self, buf: &mut [u8], payload_len: usize) -> Result<usize, &'static str> {
        if buf.len() < IPV4_MIN_HEADER_LEN + payload_len {
            return Err("Buffer too small for IPv4 header and payload");
        }

        let total_len = (IPV4_MIN_HEADER_LEN + payload_len) as u16;

        buf[0] = (4 << 4) | 5; // Version 4, IHL 5 (20 bytes)
        buf[1] = (self.dscp << 2) | (self.ecn & 0x03);
        buf[2..4].copy_from_slice(&total_len.to_be_bytes());
        buf[4..6].copy_from_slice(&self.identification.to_be_bytes());

        let flags_frag = ((self.flags as u16 & 0x07) << 13) | (self.fragment_offset & 0x1FFF);
        buf[6..8].copy_from_slice(&flags_frag.to_be_bytes());

        buf[8] = self.ttl;
        buf[9] = self.protocol;
        buf[10..12].copy_from_slice(&[0, 0]); // Zero checksum before calculation

        buf[12..16].copy_from_slice(&self.src.0);
        buf[16..20].copy_from_slice(&self.dst.0);

        let cksum = compute_checksum(&buf[..IPV4_MIN_HEADER_LEN]);
        buf[10..12].copy_from_slice(&cksum.to_be_bytes());

        Ok(IPV4_MIN_HEADER_LEN)
    }

    pub fn new(protocol: u8, src: Ipv4Address, dst: Ipv4Address, id: u16) -> Self {
        Self {
            version: 4,
            ihl: 5,
            dscp: 0,
            ecn: 0,
            total_length: 20,
            identification: id,
            flags: 0x02, // Don't fragment flag
            fragment_offset: 0,
            ttl: 64,
            protocol,
            checksum: 0,
            src,
            dst,
        }
    }
}
