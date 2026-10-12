use crate::net::wire::{compute_pseudo_checksum, ip_proto, Ipv4Address};

pub const TCP_MIN_HEADER_LEN: usize = 20;

// TCP Flag bitmasks
pub mod tcp_flags {
    pub const FIN: u8 = 0x01;
    pub const SYN: u8 = 0x02;
    pub const RST: u8 = 0x04;
    pub const PSH: u8 = 0x08;
    pub const ACK: u8 = 0x10;
    pub const URG: u8 = 0x20;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TcpState {
    Closed,
    Listen,
    SynSent,
    SynReceived,
    Established,
    FinWait1,
    FinWait2,
    CloseWait,
    Closing,
    LastAck,
    TimeWait,
}

#[derive(Debug, Clone, Copy)]
pub struct TcpHeader {
    pub src_port: u16,
    pub dst_port: u16,
    pub seq_number: u32,
    pub ack_number: u32,
    pub data_offset: u8,
    pub flags: u8,
    pub window_size: u16,
    pub checksum: u16,
    pub urgent_ptr: u16,
}

impl TcpHeader {
    pub fn parse(src_ip: Ipv4Address, dst_ip: Ipv4Address, buf: &[u8]) -> Result<(Self, &[u8]), &'static str> {
        if buf.len() < TCP_MIN_HEADER_LEN {
            return Err("TCP buffer too short for header");
        }

        let src_port = u16::from_be_bytes([buf[0], buf[1]]);
        let dst_port = u16::from_be_bytes([buf[2], buf[3]]);
        let seq_number = u32::from_be_bytes([buf[4], buf[5], buf[6], buf[7]]);
        let ack_number = u32::from_be_bytes([buf[8], buf[9], buf[10], buf[11]]);
        let data_offset_res = buf[12];
        let data_offset = (data_offset_res >> 4) * 4;
        let flags = buf[13];
        let window_size = u16::from_be_bytes([buf[14], buf[15]]);
        let checksum = u16::from_be_bytes([buf[16], buf[17]]);
        let urgent_ptr = u16::from_be_bytes([buf[18], buf[19]]);

        if (data_offset as usize) < TCP_MIN_HEADER_LEN || buf.len() < (data_offset as usize) {
            return Err("TCP header length (data offset) invalid");
        }

        // Verify TCP checksum with pseudo-header
        let calculated = compute_pseudo_checksum(src_ip, dst_ip, ip_proto::TCP, buf.len() as u16, buf);
        if calculated != 0 {
            return Err("TCP checksum validation failed");
        }

        let payload = &buf[data_offset as usize..];

        Ok((
            Self {
                src_port,
                dst_port,
                seq_number,
                ack_number,
                data_offset,
                flags,
                window_size,
                checksum,
                urgent_ptr,
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
        let header_len = TCP_MIN_HEADER_LEN;
        let total_len = header_len + payload.len();
        if buf.len() < total_len {
            return Err("Buffer too small for TCP segment");
        }

        buf[0..2].copy_from_slice(&self.src_port.to_be_bytes());
        buf[2..4].copy_from_slice(&self.dst_port.to_be_bytes());
        buf[4..8].copy_from_slice(&self.seq_number.to_be_bytes());
        buf[8..12].copy_from_slice(&self.ack_number.to_be_bytes());
        buf[12] = 5 << 4; // Data offset: 5 * 4 = 20 bytes
        buf[13] = self.flags;
        buf[14..16].copy_from_slice(&self.window_size.to_be_bytes());
        buf[16..18].copy_from_slice(&[0, 0]); // Zero checksum for calculation
        buf[18..20].copy_from_slice(&self.urgent_ptr.to_be_bytes());
        buf[20..total_len].copy_from_slice(payload);

        let cksum = compute_pseudo_checksum(src_ip, dst_ip, ip_proto::TCP, total_len as u16, &buf[..total_len]);
        buf[16..18].copy_from_slice(&cksum.to_be_bytes());

        Ok(total_len)
    }
}

/// Lightweight TCP Connection Endpoint State
#[derive(Debug, Clone, Copy)]
pub struct TcpConnection {
    pub state: TcpState,
    pub local_port: u16,
    pub remote_ip: Ipv4Address,
    pub remote_port: u16,
    pub local_seq: u32,
    pub remote_seq: u32,
    pub window_size: u16,
}

impl TcpConnection {
    pub const fn new() -> Self {
        Self {
            state: TcpState::Closed,
            local_port: 0,
            remote_ip: Ipv4Address::UNSPECIFIED,
            remote_port: 0,
            local_seq: 1000,
            remote_seq: 0,
            window_size: 4096,
        }
    }

    /// Progress TCP state machine based on incoming segment
    pub fn handle_segment(&mut self, header: &TcpHeader, payload_len: usize) {
        match self.state {
            TcpState::SynSent => {
                // If SYN+ACK received
                if (header.flags & (tcp_flags::SYN | tcp_flags::ACK)) == (tcp_flags::SYN | tcp_flags::ACK) {
                    self.remote_seq = header.seq_number.wrapping_add(1);
                    self.local_seq = header.ack_number;
                    self.state = TcpState::Established;
                } else if (header.flags & tcp_flags::RST) != 0 {
                    self.state = TcpState::Closed;
                }
            }
            TcpState::Established => {
                if (header.flags & tcp_flags::FIN) != 0 {
                    self.remote_seq = header.seq_number.wrapping_add(1);
                    self.state = TcpState::CloseWait;
                } else if (header.flags & tcp_flags::ACK) != 0 {
                    if payload_len > 0 {
                        self.remote_seq = header.seq_number.wrapping_add(payload_len as u32);
                    }
                }
            }
            TcpState::FinWait1 => {
                if (header.flags & tcp_flags::ACK) != 0 {
                    self.state = TcpState::FinWait2;
                }
            }
            TcpState::FinWait2 => {
                if (header.flags & tcp_flags::FIN) != 0 {
                    self.remote_seq = header.seq_number.wrapping_add(1);
                    self.state = TcpState::TimeWait;
                }
            }
            TcpState::LastAck => {
                if (header.flags & tcp_flags::ACK) != 0 {
                    self.state = TcpState::Closed;
                }
            }
            _ => {}
        }
    }
}
