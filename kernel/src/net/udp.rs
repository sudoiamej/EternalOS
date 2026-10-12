use crate::net::wire::{compute_pseudo_checksum, ip_proto, Ipv4Address};

pub const UDP_HEADER_LEN: usize = 8;

#[derive(Debug, Clone, Copy)]
pub struct UdpHeader {
    pub src_port: u16,
    pub dst_port: u16,
    pub length: u16,
    pub checksum: u16,
}

impl UdpHeader {
    pub fn parse(src_ip: Ipv4Address, dst_ip: Ipv4Address, buf: &[u8]) -> Result<(Self, &[u8]), &'static str> {
        if buf.len() < UDP_HEADER_LEN {
            return Err("UDP buffer too short for header");
        }

        let src_port = u16::from_be_bytes([buf[0], buf[1]]);
        let dst_port = u16::from_be_bytes([buf[2], buf[3]]);
        let length = u16::from_be_bytes([buf[4], buf[5]]);
        let checksum = u16::from_be_bytes([buf[6], buf[7]]);

        if (length as usize) < UDP_HEADER_LEN || buf.len() < (length as usize) {
            return Err("UDP datagram length invalid or truncated");
        }

        let datagram_bytes = &buf[..length as usize];

        // If checksum is non-zero, verify it against the IPv4 pseudo-header
        if checksum != 0 {
            let calculated = compute_pseudo_checksum(src_ip, dst_ip, ip_proto::UDP, length, datagram_bytes);
            if calculated != 0 {
                return Err("UDP checksum validation failed");
            }
        }

        let payload = &buf[UDP_HEADER_LEN..length as usize];

        Ok((
            Self {
                src_port,
                dst_port,
                length,
                checksum,
            },
            payload,
        ))
    }

    pub fn emit(
        &self,
        src_ip: Ipv4Address,
        dst_ip: Ipv4Address,
        buf: &mut [u8],
        payload: &[u8],
    ) -> Result<usize, &'static str> {
        let total_len = UDP_HEADER_LEN + payload.len();
        if buf.len() < total_len {
            return Err("Buffer too small for UDP datagram");
        }

        buf[0..2].copy_from_slice(&self.src_port.to_be_bytes());
        buf[2..4].copy_from_slice(&self.dst_port.to_be_bytes());
        buf[4..6].copy_from_slice(&(total_len as u16).to_be_bytes());
        buf[6..8].copy_from_slice(&[0, 0]); // Zero checksum for calculation
        buf[8..total_len].copy_from_slice(payload);

        let cksum = compute_pseudo_checksum(src_ip, dst_ip, ip_proto::UDP, total_len as u16, &buf[..total_len]);
        // RFC 768: If computed checksum is zero, transmit 0xFFFF
        let cksum = if cksum == 0 { 0xFFFF } else { cksum };
        buf[6..8].copy_from_slice(&cksum.to_be_bytes());

        Ok(total_len)
    }

    pub fn new(src_port: u16, dst_port: u16) -> Self {
        Self {
            src_port,
            dst_port,
            length: 8,
            checksum: 0,
        }
    }
}
