use crate::net::wire::compute_checksum;

pub const ICMP_TYPE_ECHO_REPLY: u8 = 0;
pub const ICMP_TYPE_DEST_UNREACHABLE: u8 = 3;
pub const ICMP_TYPE_ECHO_REQUEST: u8 = 8;
pub const ICMP_MIN_HEADER_LEN: usize = 8;

#[derive(Debug, Clone, Copy)]
pub struct IcmpHeader {
    pub msg_type: u8,
    pub code: u8,
    pub checksum: u16,
    pub id: u16,
    pub sequence: u16,
}

impl IcmpHeader {
    pub fn parse(buf: &[u8]) -> Result<(Self, &[u8]), &'static str> {
        if buf.len() < ICMP_MIN_HEADER_LEN {
            return Err("ICMP buffer too short for header");
        }

        // Verify ICMP checksum over entire ICMP message
        let calculated = compute_checksum(buf);
        if calculated != 0 {
            return Err("ICMP message checksum invalid");
        }

        let msg_type = buf[0];
        let code = buf[1];
        let checksum = u16::from_be_bytes([buf[2], buf[3]]);
        let id = u16::from_be_bytes([buf[4], buf[5]]);
        let sequence = u16::from_be_bytes([buf[6], buf[7]]);

        Ok((
            Self {
                msg_type,
                code,
                checksum,
                id,
                sequence,
            },
            &buf[ICMP_MIN_HEADER_LEN..],
        ))
    }

    pub fn emit(&self, buf: &mut [u8], payload: &[u8]) -> Result<usize, &'static str> {
        let total_len = ICMP_MIN_HEADER_LEN + payload.len();
        if buf.len() < total_len {
            return Err("Buffer too small for ICMP message");
        }

        buf[0] = self.msg_type;
        buf[1] = self.code;
        buf[2..4].copy_from_slice(&[0, 0]); // Zero checksum initially
        buf[4..6].copy_from_slice(&self.id.to_be_bytes());
        buf[6..8].copy_from_slice(&self.sequence.to_be_bytes());
        buf[8..total_len].copy_from_slice(payload);

        let cksum = compute_checksum(&buf[..total_len]);
        buf[2..4].copy_from_slice(&cksum.to_be_bytes());

        Ok(total_len)
    }

    pub fn new_echo_request(id: u16, sequence: u16) -> Self {
        Self {
            msg_type: ICMP_TYPE_ECHO_REQUEST,
            code: 0,
            checksum: 0,
            id,
            sequence,
        }
    }

    pub fn new_echo_reply(id: u16, sequence: u16) -> Self {
        Self {
            msg_type: ICMP_TYPE_ECHO_REPLY,
            code: 0,
            checksum: 0,
            id,
            sequence,
        }
    }
}
