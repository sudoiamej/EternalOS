use crate::net::ipv4::Ipv4Header;
use crate::net::stack::{NetworkStack, MAX_PACKET_LEN};
use crate::net::tcp::{tcp_flags, TcpHeader};
use crate::net::wire::{ethertype, ip_proto, EthernetHeader};
use crate::net::NetworkDevice;
use crate::sync::SpinLock;
use crate::timer::get_ticks;

pub const DEFAULT_HTTP_PORT: u16 = 80;

#[derive(Debug, Clone, Copy)]
pub struct HttpStats {
    pub port: u16,
    pub is_running: bool,
    pub total_requests: u64,
    pub total_bytes_sent: u64,
    pub start_tick: u64,
}

pub struct HttpServer {
    pub port: u16,
    pub is_running: bool,
    pub total_requests: u64,
    pub total_bytes_sent: u64,
    pub start_tick: u64,
}

impl HttpServer {
    pub const fn new() -> Self {
        Self {
            port: DEFAULT_HTTP_PORT,
            is_running: false,
            total_requests: 0,
            total_bytes_sent: 0,
            start_tick: 0,
        }
    }

    pub fn start(&mut self, port: u16) {
        self.port = port;
        self.is_running = true;
        self.start_tick = get_ticks();
        let _ = crate::service::start_service("web");
    }

    pub fn stop(&mut self) {
        self.is_running = false;
        let _ = crate::service::stop_service("web");
    }

    pub fn stats(&self) -> HttpStats {
        HttpStats {
            port: self.port,
            is_running: self.is_running,
            total_requests: self.total_requests,
            total_bytes_sent: self.total_bytes_sent,
            start_tick: self.start_tick,
        }
    }
}

pub static HTTP_SERVER: SpinLock<HttpServer> = SpinLock::new(HttpServer::new());

/// Dispatch an HTTP request line to the appropriate route handler
pub fn handle_http_request(request: &str, out: &mut [u8]) -> usize {
    // Parse method and path from request (e.g. "GET /health HTTP/1.1")
    let mut parts = request.split_whitespace();
    let method = parts.next().unwrap_or("GET");
    let path = parts.next().unwrap_or("/");

    if method != "GET" && method != "HEAD" {
        return build_response(405, "Method Not Allowed", "text/plain", b"405 Method Not Allowed\r\n", out);
    }

    match path {
        "/" | "/index.html" => {
            let body = b"<!DOCTYPE html>\n\
<html>\n\
<head>\n\
  <meta charset=\"utf-8\">\n\
  <title>EternalOS Server</title>\n\
  <style>\n\
    body { background-color: #110B18; color: #FFFFFF; font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; margin: 40px; }\n\
    h1 { color: #8A4FFF; margin-bottom: 8px; }\n\
    p.lead { color: #A09BA8; font-size: 16px; margin-top: 0; }\n\
    .card { background-color: #1D1328; border: 1px solid #4D2D82; border-radius: 8px; padding: 20px; margin-top: 24px; }\n\
    .badge { background-color: #8A4FFF; color: #FFFFFF; padding: 4px 10px; border-radius: 4px; font-size: 12px; font-weight: bold; }\n\
    .metric { display: inline-block; margin-right: 32px; }\n\
    .metric-val { font-size: 24px; font-weight: bold; color: #C4A1FF; }\n\
    .metric-label { font-size: 12px; color: #7B7388; text-transform: uppercase; }\n\
    a { color: #C4A1FF; text-decoration: none; font-weight: 500; }\n\
    a:hover { text-decoration: underline; }\n\
    ul { list-style-type: square; color: #8A4FFF; }\n\
    li { color: #E8E5EE; margin: 6px 0; }\n\
  </style>\n\
</head>\n\
<body>\n\
  <h1>EternalOS Server</h1>\n\
  <p class=\"lead\">Universal Modular Server Infrastructure &amp; Native Administration</p>\n\
  <div class=\"card\">\n\
    <p><span class=\"badge\">ONLINE</span> <strong>Kernel:</strong> x86-64 no_std (Limine Boot Protocol)</p>\n\
    <div style=\"margin: 20px 0;\">\n\
      <div class=\"metric\"><div class=\"metric-val\">Ring 0</div><div class=\"metric-label\">Kernel Mode</div></div>\n\
      <div class=\"metric\"><div class=\"metric-val\">E1000</div><div class=\"metric-label\">Gigabit NIC</div></div>\n\
      <div class=\"metric\"><div class=\"metric-val\">ECS</div><div class=\"metric-label\">Administration</div></div>\n\
    </div>\n\
    <p><strong>REST API Endpoints:</strong></p>\n\
    <ul>\n\
      <li><a href=\"/health\">/health</a> &mdash; System Health &amp; Liveness Probe</li>\n\
      <li><a href=\"/api/status\">/api/status</a> &mdash; Dynamic Telemetry &amp; Metrics</li>\n\
      <li><a href=\"/esys\">/esys</a> &mdash; Architecture &amp; System Overview</li>\n\
    </ul>\n\
  </div>\n\
</body>\n\
</html>\n";
            build_response(200, "OK", "text/html; charset=utf-8", body, out)
        }
        "/health" => {
            let body = b"{\"status\":\"healthy\",\"server\":\"EternalOS-Httpd/0.1\",\"service\":\"web\"}\r\n";
            build_response(200, "OK", "application/json", body, out)
        }
        "/api/status" => {
            let uptime_sec = (get_ticks() * crate::timer::MS_PER_TICK) / 1000;
            let mut buf = [0u8; 384];
            let mut cur = 0usize;
            let prefix = b"{\"system\":\"EternalOS\",\"arch\":\"x86_64\",\"status\":\"active\",\"uptime_seconds\":";
            buf[cur..cur + prefix.len()].copy_from_slice(prefix);
            cur += prefix.len();
            let mut num_buf = [0u8; 32];
            let num_str = crate::shell::format_u64(uptime_sec, &mut num_buf);
            buf[cur..cur + num_str.len()].copy_from_slice(num_str.as_bytes());
            cur += num_str.len();
            let suffix = b",\"network\":{\"ip\":\"10.0.2.15\",\"nic\":\"Intel 82540EM\"},\"workload\":\"eweb\"}\r\n";
            buf[cur..cur + suffix.len()].copy_from_slice(suffix);
            cur += suffix.len();

            build_response(200, "OK", "application/json", &buf[..cur], out)
        }
        "/esys" => {
            let body = b"{\"os\":\"EternalOS\",\"version\":\"Phase 6 Server\",\"bootloader\":\"Limine\",\"arch\":\"x86-64 Long Mode\"}\r\n";
            build_response(200, "OK", "application/json", body, out)
        }
        _ => {
            let body = b"404 Not Found: The requested endpoint does not exist on this EternalOS Server.\r\n";
            build_response(404, "Not Found", "text/plain", body, out)
        }
    }
}

fn build_response(code: u16, status_text: &str, content_type: &str, body: &[u8], out: &mut [u8]) -> usize {
    let mut header_buf = [0u8; 256];
    let mut hlen = 0;

    let write_h = |dest: &mut [u8], len: &mut usize, s: &str| {
        let b = s.as_bytes();
        let rem = dest.len() - *len;
        let c = b.len().min(rem);
        dest[*len..*len + c].copy_from_slice(&b[..c]);
        *len += c;
    };

    let mut num_b = [0u8; 32];
    write_h(&mut header_buf, &mut hlen, "HTTP/1.1 ");
    write_h(&mut header_buf, &mut hlen, crate::shell::format_u64(code as u64, &mut num_b));
    write_h(&mut header_buf, &mut hlen, " ");
    write_h(&mut header_buf, &mut hlen, status_text);
    write_h(&mut header_buf, &mut hlen, "\r\nServer: EternalOS-Httpd/0.1\r\nConnection: close\r\nContent-Type: ");
    write_h(&mut header_buf, &mut hlen, content_type);
    write_h(&mut header_buf, &mut hlen, "\r\nContent-Length: ");
    write_h(&mut header_buf, &mut hlen, crate::shell::format_u64(body.len() as u64, &mut num_b));
    write_h(&mut header_buf, &mut hlen, "\r\n\r\n");

    let total = hlen + body.len();
    if out.len() >= total {
        out[..hlen].copy_from_slice(&header_buf[..hlen]);
        out[hlen..total].copy_from_slice(body);
        total
    } else {
        0
    }
}

/// Process incoming network packets and handle HTTP transactions when the server is active
pub fn poll_http_service(stack: &mut NetworkStack) -> bool {
    let (is_running, server_port) = {
        let s = HTTP_SERVER.lock();
        (s.is_running, s.port)
    };

    if !is_running {
        return false;
    }

    let dev = match stack.device.as_mut() {
        Some(d) => d,
        None => return false,
    };

    let mut rx_buf = [0u8; MAX_PACKET_LEN];
    let len = match dev.receive(&mut rx_buf) {
        Ok(Some(l)) => l,
        _ => return false,
    };

    let frame = &rx_buf[..len];
    let (eth, eth_pl) = match EthernetHeader::parse(frame) {
        Ok(res) => res,
        Err(_) => return false,
    };

    if eth.ethertype != ethertype::IPV4 {
        return false;
    }

    let (ip_hdr, ip_pl) = match Ipv4Header::parse(eth_pl) {
        Ok(res) => res,
        Err(_) => return false,
    };

    if ip_hdr.protocol != ip_proto::TCP || ip_hdr.dst != stack.config.ip {
        return false;
    }

    let (tcp_hdr, tcp_pl) = match TcpHeader::parse(ip_hdr.src, ip_hdr.dst, ip_pl) {
        Ok(res) => res,
        Err(_) => return false,
    };

    if tcp_hdr.dst_port != server_port {
        return false;
    }

    let client_ip = ip_hdr.src;
    let client_port = tcp_hdr.src_port;
    let client_mac = eth.src;
    let my_mac = dev.mac_address();

    // Check if SYN packet (Connection initiation)
    if (tcp_hdr.flags & tcp_flags::SYN) != 0 && (tcp_hdr.flags & tcp_flags::ACK) == 0 {
        // Send SYN-ACK reply
        let syn_ack = TcpHeader {
            src_port: server_port,
            dst_port: client_port,
            seq_number: 0x2000,
            ack_number: tcp_hdr.seq_number.wrapping_add(1),
            data_offset: 20,
            flags: tcp_flags::SYN | tcp_flags::ACK,
            window_size: 4096,
            checksum: 0,
            urgent_ptr: 0,
        };

        let mut tcp_buf = [0u8; 64];
        if let Ok(tlen) = syn_ack.emit(stack.config.ip, client_ip, &mut tcp_buf, &[]) {
            stack.next_ip_id = stack.next_ip_id.wrapping_add(1);
            let resp_ip = Ipv4Header::new(ip_proto::TCP, stack.config.ip, client_ip, stack.next_ip_id);
            let mut out_frame = [0u8; 128];
            let resp_eth = EthernetHeader {
                dst: client_mac,
                src: my_mac,
                ethertype: ethertype::IPV4,
            };
            let elen = resp_eth.emit(&mut out_frame[0..14]).unwrap_or(14);
            let ilen = resp_ip.emit(&mut out_frame[elen..], tlen).unwrap_or(20);
            out_frame[elen + ilen..elen + ilen + tlen].copy_from_slice(&tcp_buf[..tlen]);
            let total = (elen + ilen + tlen).max(60);
            let _ = dev.transmit(&out_frame[..total]);
        }
        return true;
    }

    // Check if packet carries an HTTP request payload
    if !tcp_pl.is_empty() {
        if let Ok(req_str) = core::str::from_utf8(tcp_pl) {
            let mut resp_payload = [0u8; 1400];
            let resp_len = handle_http_request(req_str, &mut resp_payload);

            if resp_len > 0 {
                // Send HTTP response with PSH + ACK + FIN to close cleanly
                let resp_tcp = TcpHeader {
                    src_port: server_port,
                    dst_port: client_port,
                    seq_number: 0x2001,
                    ack_number: tcp_hdr.seq_number.wrapping_add(tcp_pl.len() as u32),
                    data_offset: 20,
                    flags: tcp_flags::PSH | tcp_flags::ACK | tcp_flags::FIN,
                    window_size: 4096,
                    checksum: 0,
                    urgent_ptr: 0,
                };

                let mut tcp_buf = [0u8; 1460];
                if let Ok(tlen) = resp_tcp.emit(stack.config.ip, client_ip, &mut tcp_buf, &resp_payload[..resp_len]) {
                    stack.next_ip_id = stack.next_ip_id.wrapping_add(1);
                    let resp_ip = Ipv4Header::new(ip_proto::TCP, stack.config.ip, client_ip, stack.next_ip_id);
                    let mut out_frame = [0u8; 1514];
                    let resp_eth = EthernetHeader {
                        dst: client_mac,
                        src: my_mac,
                        ethertype: ethertype::IPV4,
                    };
                    let elen = resp_eth.emit(&mut out_frame[0..14]).unwrap_or(14);
                    let ilen = resp_ip.emit(&mut out_frame[elen..], tlen).unwrap_or(20);
                    out_frame[elen + ilen..elen + ilen + tlen].copy_from_slice(&tcp_buf[..tlen]);
                    let total = (elen + ilen + tlen).max(60);
                    let _ = dev.transmit(&out_frame[..total]);

                    let mut s = HTTP_SERVER.lock();
                    s.total_requests += 1;
                    s.total_bytes_sent += resp_len as u64;
                    crate::service::log_service("web", "Served HTTP request");
                }
            }
        }
        return true;
    }

    false
}
