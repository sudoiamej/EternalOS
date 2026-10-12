use crate::net::wire::Ipv4Address;

pub const DNS_PORT: u16 = 53;
pub const DNS_TYPE_A: u16 = 1;
pub const DNS_CLASS_IN: u16 = 1;

#[derive(Debug, Clone, Copy)]
pub struct DnsHeader {
    pub id: u16,
    pub flags: u16,
    pub qdcount: u16,
    pub ancount: u16,
    pub nscount: u16,
    pub arcount: u16,
}

impl DnsHeader {
    pub fn parse(buf: &[u8]) -> Result<Self, &'static str> {
        if buf.len() < 12 {
            return Err("DNS header too short");
        }
        let id = u16::from_be_bytes([buf[0], buf[1]]);
        let flags = u16::from_be_bytes([buf[2], buf[3]]);
        let qdcount = u16::from_be_bytes([buf[4], buf[5]]);
        let ancount = u16::from_be_bytes([buf[6], buf[7]]);
        let nscount = u16::from_be_bytes([buf[8], buf[9]]);
        let arcount = u16::from_be_bytes([buf[10], buf[11]]);

        Ok(Self {
            id,
            flags,
            qdcount,
            ancount,
            nscount,
            arcount,
        })
    }

    pub fn emit(&self, buf: &mut [u8]) -> Result<usize, &'static str> {
        if buf.len() < 12 {
            return Err("Buffer too small for DNS header");
        }
        buf[0..2].copy_from_slice(&self.id.to_be_bytes());
        buf[2..4].copy_from_slice(&self.flags.to_be_bytes());
        buf[4..6].copy_from_slice(&self.qdcount.to_be_bytes());
        buf[6..8].copy_from_slice(&self.ancount.to_be_bytes());
        buf[8..10].copy_from_slice(&self.nscount.to_be_bytes());
        buf[10..12].copy_from_slice(&self.arcount.to_be_bytes());
        Ok(12)
    }

    pub fn new_query(id: u16) -> Self {
        Self {
            id,
            flags: 0x0100, // Standard query, recursion desired (RD=1)
            qdcount: 1,
            ancount: 0,
            nscount: 0,
            arcount: 0,
        }
    }
}

/// Encode a dot-separated hostname (e.g. "google.com") into DNS label format
pub fn encode_qname(hostname: &str, buf: &mut [u8]) -> Result<usize, &'static str> {
    let mut offset = 0;
    for label in hostname.split('.') {
        if label.is_empty() {
            continue;
        }
        if label.len() > 63 {
            return Err("DNS label exceeds 63 bytes");
        }
        if offset + 1 + label.len() >= buf.len() {
            return Err("Buffer too small for DNS qname");
        }
        buf[offset] = label.len() as u8;
        offset += 1;
        buf[offset..offset + label.len()].copy_from_slice(label.as_bytes());
        offset += label.len();
    }
    if offset >= buf.len() {
        return Err("Buffer too small for DNS null terminator");
    }
    buf[offset] = 0;
    offset += 1;
    Ok(offset)
}

/// Construct a full DNS query packet for an A record into `buf`. Returns packet length.
pub fn build_dns_query(id: u16, hostname: &str, buf: &mut [u8]) -> Result<usize, &'static str> {
    if buf.len() < 12 {
        return Err("Buffer too small for DNS query");
    }
    let header = DnsHeader::new_query(id);
    let mut offset = header.emit(buf)?;

    let qname_len = encode_qname(hostname, &mut buf[offset..])?;
    offset += qname_len;

    if offset + 4 > buf.len() {
        return Err("Buffer too small for DNS query type/class");
    }
    buf[offset..offset + 2].copy_from_slice(&DNS_TYPE_A.to_be_bytes());
    offset += 2;
    buf[offset..offset + 2].copy_from_slice(&DNS_CLASS_IN.to_be_bytes());
    offset += 2;

    Ok(offset)
}

/// Skip a DNS name field in `buf` starting at `offset`, handling compression pointers (RFC 1035)
fn skip_name(buf: &[u8], mut offset: usize) -> Result<usize, &'static str> {
    let mut jumps = 0;
    while offset < buf.len() {
        let len_byte = buf[offset];
        if len_byte == 0 {
            return Ok(offset + 1);
        }
        if (len_byte & 0xC0) == 0xC0 {
            // Pointer is 2 bytes
            if offset + 2 > buf.len() {
                return Err("Truncated DNS compression pointer");
            }
            return Ok(offset + 2);
        }
        // Normal label
        let label_len = len_byte as usize;
        offset += 1 + label_len;
        jumps += 1;
        if jumps > 128 {
            return Err("DNS label loop detected");
        }
    }
    Err("DNS name overruns packet buffer")
}

/// Parse a DNS response to extract the first IPv4 (A record) address
pub fn parse_dns_response(buf: &[u8], expected_id: u16) -> Result<Ipv4Address, &'static str> {
    if buf.len() < 12 {
        return Err("DNS response too short for header");
    }
    let header = DnsHeader::parse(buf)?;
    if header.id != expected_id {
        return Err("DNS transaction ID mismatch");
    }
    // Check RCODE (bits 0..3 of flags)
    let rcode = header.flags & 0x000F;
    if rcode != 0 {
        return Err("DNS server returned error status");
    }
    if header.ancount == 0 {
        return Err("DNS response contains 0 answer records");
    }

    let mut offset = 12;

    // Skip question section
    for _ in 0..header.qdcount {
        offset = skip_name(buf, offset)?;
        if offset + 4 > buf.len() {
            return Err("Truncated DNS question section");
        }
        offset += 4; // Skip QTYPE (2) + QCLASS (2)
    }

    // Parse answers
    for _ in 0..header.ancount {
        offset = skip_name(buf, offset)?;
        if offset + 10 > buf.len() {
            return Err("Truncated DNS answer header");
        }
        let atype = u16::from_be_bytes([buf[offset], buf[offset + 1]]);
        let _aclass = u16::from_be_bytes([buf[offset + 2], buf[offset + 3]]);
        let _ttl = u32::from_be_bytes([buf[offset + 4], buf[offset + 5], buf[offset + 6], buf[offset + 7]]);
        let rdlength = u16::from_be_bytes([buf[offset + 8], buf[offset + 9]]) as usize;
        offset += 10;

        if offset + rdlength > buf.len() {
            return Err("Truncated DNS answer RDATA");
        }

        if atype == DNS_TYPE_A && rdlength == 4 {
            let mut ip = [0u8; 4];
            ip.copy_from_slice(&buf[offset..offset + 4]);
            return Ok(Ipv4Address(ip));
        }

        offset += rdlength;
    }

    Err("No IPv4 (A) record found in DNS response")
}
