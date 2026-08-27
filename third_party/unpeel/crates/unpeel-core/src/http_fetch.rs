//! Minimal blocking HTTP(S) GET for small manifests — the CLI update check
//! fetching `cli/latest.json` from unpeel.com. Same TLS stack as the relay
//! uplink (rustls + webpki roots), `http://` allowed for tests and local
//! dev servers. Not a general client: no redirects, no keep-alive, response
//! capped at 2 MB.

use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::Arc;
use std::time::Duration;

const MAX_RESPONSE: usize = 2 * 1024 * 1024;
const TIMEOUT: Duration = Duration::from_secs(10);

/// GET `url` and return the response body. Errors on any non-200 status.
pub fn get(url: &str) -> Result<Vec<u8>, String> {
    get_with_headers(url, &[])
}

/// `get` with extra request headers — the CLI update check uses this to carry
/// its anonymous install id (see `unpeel-tui`'s `update` module).
pub fn get_with_headers(url: &str, extra_headers: &[(&str, &str)]) -> Result<Vec<u8>, String> {
    let (secure, rest) = if let Some(rest) = url.strip_prefix("https://") {
        (true, rest)
    } else if let Some(rest) = url.strip_prefix("http://") {
        (false, rest)
    } else {
        return Err(format!("unsupported url: {url}"));
    };
    let (host_port, path) = match rest.split_once('/') {
        Some((host_port, path)) => (host_port, format!("/{path}")),
        None => (rest, "/".to_string()),
    };
    let (host, port) = match host_port.rsplit_once(':') {
        Some((host, port)) => (
            host.to_string(),
            port.parse::<u16>().map_err(|e| format!("port: {e}"))?,
        ),
        None => (host_port.to_string(), if secure { 443 } else { 80 }),
    };

    let address = (host.as_str(), port)
        .to_socket_addrs()
        .map_err(|e| format!("resolve {host}: {e}"))?
        .next()
        .ok_or_else(|| format!("resolve {host}: no addresses"))?;
    let tcp = TcpStream::connect_timeout(&address, TIMEOUT).map_err(|e| format!("connect: {e}"))?;
    tcp.set_read_timeout(Some(TIMEOUT)).ok();
    tcp.set_write_timeout(Some(TIMEOUT)).ok();

    let mut request = format!(
        "GET {path} HTTP/1.1\r\nHost: {host}\r\nUser-Agent: unpeel/{}\r\n\
         Accept: application/json\r\nConnection: close\r\n",
        env!("CARGO_PKG_VERSION")
    );
    for (name, value) in extra_headers {
        request.push_str(&format!("{name}: {value}\r\n"));
    }
    request.push_str("\r\n");

    let mut raw = Vec::new();
    if secure {
        let roots = rustls::RootCertStore {
            roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
        };
        let config = rustls::ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth();
        let name = rustls::pki_types::ServerName::try_from(host.clone())
            .map_err(|e| format!("server name: {e}"))?;
        let connection = rustls::ClientConnection::new(Arc::new(config), name)
            .map_err(|e| format!("tls: {e}"))?;
        let mut stream = rustls::StreamOwned::new(connection, tcp);
        stream
            .write_all(request.as_bytes())
            .map_err(|e| format!("write: {e}"))?;
        read_to_end_capped(&mut stream, &mut raw)?;
    } else {
        let mut stream = tcp;
        stream
            .write_all(request.as_bytes())
            .map_err(|e| format!("write: {e}"))?;
        read_to_end_capped(&mut stream, &mut raw)?;
    }

    parse_response(&raw)
}

fn read_to_end_capped(stream: &mut impl Read, out: &mut Vec<u8>) -> Result<(), String> {
    let mut chunk = [0u8; 8192];
    loop {
        match stream.read(&mut chunk) {
            Ok(0) => return Ok(()),
            Ok(n) => {
                out.extend_from_slice(&chunk[..n]);
                if out.len() > MAX_RESPONSE {
                    return Err("response too large".into());
                }
            }
            // rustls surfaces a close without close_notify as an error;
            // Connection: close servers routinely skip it — the response is
            // complete (and further validated by the header/body parse).
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(()),
            Err(e) => return Err(format!("read: {e}")),
        }
    }
}

fn parse_response(raw: &[u8]) -> Result<Vec<u8>, String> {
    let split = raw
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .ok_or("malformed response: no header terminator")?;
    let head = String::from_utf8_lossy(&raw[..split]);
    let status_line = head.lines().next().unwrap_or("");
    let status = status_line.split_whitespace().nth(1).unwrap_or("");
    if status != "200" {
        return Err(format!(
            "http status {}",
            if status.is_empty() { "?" } else { status }
        ));
    }
    let body = &raw[split + 4..];
    let chunked = head.lines().any(|line| {
        let lower = line.to_ascii_lowercase();
        lower.starts_with("transfer-encoding:") && lower.contains("chunked")
    });
    if chunked {
        dechunk(body)
    } else {
        Ok(body.to_vec())
    }
}

fn dechunk(mut body: &[u8]) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    loop {
        let line_end = body
            .windows(2)
            .position(|w| w == b"\r\n")
            .ok_or("malformed chunk: no size line")?;
        let size_text = String::from_utf8_lossy(&body[..line_end]);
        let size = usize::from_str_radix(size_text.trim().split(';').next().unwrap_or(""), 16)
            .map_err(|e| format!("malformed chunk size: {e}"))?;
        body = &body[line_end + 2..];
        if size == 0 {
            return Ok(out);
        }
        if body.len() < size + 2 {
            return Err("truncated chunk".into());
        }
        out.extend_from_slice(&body[..size]);
        body = &body[size + 2..];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_response_parses() {
        let raw = b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\r\n{\"a\":1}";
        assert_eq!(parse_response(raw).unwrap(), b"{\"a\":1}");
    }

    #[test]
    fn chunked_response_reassembles() {
        let raw = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n4\r\n{\"a\"\r\n3\r\n:1}\r\n0\r\n\r\n";
        assert_eq!(parse_response(raw).unwrap(), b"{\"a\":1}");
    }

    #[test]
    fn non_200_is_an_error() {
        let raw = b"HTTP/1.1 404 Not Found\r\n\r\nnope";
        assert!(parse_response(raw).unwrap_err().contains("404"));
    }

    #[test]
    fn get_over_local_http_works() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            let mut buffer = [0u8; 2048];
            let _ = socket.read(&mut buffer);
            socket
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 7\r\n\r\n{\"v\":2}")
                .unwrap();
        });
        let body = get(&format!(
            "http://127.0.0.1:{port}/releases/beta/cli/latest.json"
        ))
        .unwrap();
        assert_eq!(body, b"{\"v\":2}");
    }
}
