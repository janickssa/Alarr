//! Quorum agent for the WireGuard container.
//!
//! Runs `wg show all dump` and serves the peers as JSON on `GET /peers`, protected by a
//! shared token (`Authorization: Bearer <token>`). `GET /version` is public. Private and
//! preshared keys from the dump are never sent.
//!
//! Configuration (environment):
//! - `QUORUM_AGENT_BIND`: listen address, default `0.0.0.0:9587`.
//! - `QUORUM_AGENT_TOKEN_FILE`: file holding the token (at least 32 characters), default
//!   `/etc/quorum-agent/token`.
//! - `QUORUM_AGENT_NAMES`: optional file of `<public key> <name>` lines, default
//!   `/etc/quorum-agent/names`.
//! - `QUORUM_AGENT_ALLOW`: optional comma-separated client IPs allowed to connect
//!   (e.g. the Quorum server's address). Empty allows any.
//! - `QUORUM_AGENT_WG`: path of the `wg` binary, default `wg`.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{IpAddr, TcpListener, TcpStream};
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

const VERSION: &str = env!("CARGO_PKG_VERSION");
const MAX_HEADER_BYTES: usize = 8 * 1024;
const MAX_CONNECTIONS: usize = 16;
const MIN_TOKEN_LEN: usize = 32;

struct Config {
    bind: String,
    token: String,
    names_file: String,
    allow: Vec<IpAddr>,
    wg: String,
}

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key)
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| default.to_owned())
}

fn load_config() -> Result<Config, String> {
    let token_file = env_or("QUORUM_AGENT_TOKEN_FILE", "/etc/quorum-agent/token");
    let token = std::fs::read_to_string(&token_file)
        .map_err(|e| format!("can't read the token file {token_file}: {e}"))?
        .trim()
        .to_owned();
    if token.len() < MIN_TOKEN_LEN {
        return Err(format!(
            "the token in {token_file} must be at least {MIN_TOKEN_LEN} characters"
        ));
    }
    let allow = env_or("QUORUM_AGENT_ALLOW", "")
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| {
            s.parse::<IpAddr>()
                .map_err(|_| format!("QUORUM_AGENT_ALLOW: not an IP address: {s}"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Config {
        bind: env_or("QUORUM_AGENT_BIND", "0.0.0.0:9587"),
        token,
        names_file: env_or("QUORUM_AGENT_NAMES", "/etc/quorum-agent/names"),
        allow,
        wg: env_or("QUORUM_AGENT_WG", "wg"),
    })
}

fn main() {
    if std::env::args().nth(1).as_deref() == Some("--version") {
        println!("quorum-agent {VERSION}");
        return;
    }
    let config = match load_config() {
        Ok(c) => Arc::new(c),
        Err(e) => {
            eprintln!("quorum-agent: {e}");
            std::process::exit(1);
        }
    };
    let listener = match TcpListener::bind(&config.bind) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("quorum-agent: can't listen on {}: {e}", config.bind);
            std::process::exit(1);
        }
    };
    eprintln!("quorum-agent {VERSION} listening on {}", config.bind);
    let active = Arc::new(AtomicUsize::new(0));
    for stream in listener.incoming() {
        let Ok(stream) = stream else { continue };
        if active.load(Ordering::SeqCst) >= MAX_CONNECTIONS {
            continue; // Dropping the stream closes it.
        }
        let (config, active) = (config.clone(), active.clone());
        active.fetch_add(1, Ordering::SeqCst);
        std::thread::spawn(move || {
            let _ = handle(stream, &config);
            active.fetch_sub(1, Ordering::SeqCst);
        });
    }
}

struct Request {
    method: String,
    path: String,
    headers: HashMap<String, String>,
}

/// Read the request line and headers (the agent takes no request bodies).
fn read_request(stream: &mut impl Read) -> Option<Request> {
    let mut reader = BufReader::new(stream.take(MAX_HEADER_BYTES as u64));
    let mut line = String::new();
    reader.read_line(&mut line).ok()?;
    let mut parts = line.split_whitespace();
    let method = parts.next()?.to_owned();
    let path = parts.next()?.to_owned();
    let mut headers = HashMap::new();
    loop {
        let mut h = String::new();
        if reader.read_line(&mut h).ok()? == 0 {
            return None;
        }
        let h = h.trim_end();
        if h.is_empty() {
            break;
        }
        let (k, v) = h.split_once(':')?;
        headers.insert(k.trim().to_ascii_lowercase(), v.trim().to_owned());
    }
    Some(Request {
        method,
        path,
        headers,
    })
}

/// Compare without leaking where the first difference is.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

fn authorized(req: &Request, token: &str) -> bool {
    req.headers
        .get("authorization")
        .and_then(|v| v.strip_prefix("Bearer "))
        .is_some_and(|t| constant_time_eq(t.trim().as_bytes(), token.as_bytes()))
}

fn respond(stream: &mut TcpStream, status: &str, body: &str) -> std::io::Result<()> {
    write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )?;
    stream.flush()
}

fn handle(mut stream: TcpStream, config: &Config) -> std::io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    stream.set_write_timeout(Some(Duration::from_secs(5)))?;
    let peer_ip = stream.peer_addr()?.ip();
    if !config.allow.is_empty() && !config.allow.contains(&canonical(peer_ip)) {
        return respond(&mut stream, "403 Forbidden", r#"{"error":"forbidden"}"#);
    }
    let Some(req) = read_request(&mut stream) else {
        return respond(&mut stream, "400 Bad Request", r#"{"error":"bad_request"}"#);
    };
    if req.method != "GET" {
        return respond(
            &mut stream,
            "405 Method Not Allowed",
            r#"{"error":"method_not_allowed"}"#,
        );
    }
    match req.path.as_str() {
        "/version" => respond(
            &mut stream,
            "200 OK",
            &format!(r#"{{"version":"{VERSION}"}}"#),
        ),
        "/peers" => {
            if !authorized(&req, &config.token) {
                // Slow down guessing.
                std::thread::sleep(Duration::from_secs(1));
                return respond(
                    &mut stream,
                    "401 Unauthorized",
                    r#"{"error":"unauthorized"}"#,
                );
            }
            match run_wg(&config.wg) {
                Ok(dump) => {
                    let names = std::fs::read_to_string(&config.names_file)
                        .map(|s| parse_names(&s))
                        .unwrap_or_default();
                    respond(&mut stream, "200 OK", &to_json(&parse_dump(&dump), &names))
                }
                Err(e) => respond(
                    &mut stream,
                    "500 Internal Server Error",
                    &format!(r#"{{"error":"wg_failed","message":{}}}"#, json_str(&e)),
                ),
            }
        }
        _ => respond(&mut stream, "404 Not Found", r#"{"error":"not_found"}"#),
    }
}

/// IPv4-mapped IPv6 addresses compare as IPv4.
fn canonical(ip: IpAddr) -> IpAddr {
    match ip {
        IpAddr::V6(v6) => v6.to_ipv4_mapped().map(IpAddr::V4).unwrap_or(ip),
        v4 => v4,
    }
}

fn run_wg(wg: &str) -> Result<String, String> {
    let out = Command::new(wg)
        .args(["show", "all", "dump"])
        .output()
        .map_err(|e| format!("can't run {wg}: {e}"))?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_owned());
    }
    String::from_utf8(out.stdout).map_err(|_| "wg printed invalid UTF-8".to_owned())
}

#[derive(Debug, PartialEq)]
struct Interface {
    name: String,
    public_key: String,
    listen_port: Option<u16>,
}

#[derive(Debug, PartialEq)]
struct Peer {
    interface: String,
    public_key: String,
    endpoint: Option<String>,
    allowed_ips: Vec<String>,
    /// Unix seconds; `None` = never.
    latest_handshake: Option<u64>,
    rx_bytes: u64,
    tx_bytes: u64,
    persistent_keepalive: Option<u32>,
}

/// Parse `wg show all dump` (see wg(8)): per interface a line of
/// `interface private-key public-key listen-port fwmark`, then per peer
/// `interface public-key preshared-key endpoint allowed-ips latest-handshake transfer-rx
/// transfer-tx persistent-keepalive`, tab-separated. Keys that are secret are skipped here.
fn parse_dump(dump: &str) -> (Vec<Interface>, Vec<Peer>) {
    let mut interfaces = Vec::new();
    let mut peers = Vec::new();
    for line in dump.lines() {
        let f: Vec<&str> = line.split('\t').collect();
        match f.len() {
            5 => interfaces.push(Interface {
                name: f[0].to_owned(),
                public_key: f[2].to_owned(),
                listen_port: f[3].parse().ok(),
            }),
            9 => peers.push(Peer {
                interface: f[0].to_owned(),
                public_key: f[1].to_owned(),
                endpoint: (f[3] != "(none)").then(|| f[3].to_owned()),
                allowed_ips: if f[4] == "(none)" {
                    Vec::new()
                } else {
                    f[4].split(',').map(str::to_owned).collect()
                },
                latest_handshake: f[5].parse().ok().filter(|t| *t > 0),
                rx_bytes: f[6].parse().unwrap_or(0),
                tx_bytes: f[7].parse().unwrap_or(0),
                persistent_keepalive: f[8].parse().ok(),
            }),
            _ => {}
        }
    }
    (interfaces, peers)
}

/// `<public key> <name>` per line; `#` starts a comment.
fn parse_names(text: &str) -> HashMap<String, String> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter_map(|l| {
            let (key, name) = l.split_once(char::is_whitespace)?;
            Some((key.to_owned(), name.trim().to_owned()))
        })
        .collect()
}

fn json_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn opt<T: ToString>(v: &Option<T>) -> String {
    v.as_ref().map_or("null".into(), T::to_string)
}

fn to_json(
    (interfaces, peers): &(Vec<Interface>, Vec<Peer>),
    names: &HashMap<String, String>,
) -> String {
    let ifaces: Vec<String> = interfaces
        .iter()
        .map(|i| {
            format!(
                r#"{{"name":{},"public_key":{},"listen_port":{}}}"#,
                json_str(&i.name),
                json_str(&i.public_key),
                opt(&i.listen_port)
            )
        })
        .collect();
    let peers: Vec<String> = peers
        .iter()
        .map(|p| {
            let ips: Vec<String> = p.allowed_ips.iter().map(|ip| json_str(ip)).collect();
            format!(
                r#"{{"interface":{},"public_key":{},"name":{},"endpoint":{},"allowed_ips":[{}],"latest_handshake":{},"rx_bytes":{},"tx_bytes":{},"persistent_keepalive":{}}}"#,
                json_str(&p.interface),
                json_str(&p.public_key),
                names.get(&p.public_key).map_or("null".into(), |n| json_str(n)),
                p.endpoint.as_deref().map_or("null".into(), json_str),
                ips.join(","),
                opt(&p.latest_handshake),
                p.rx_bytes,
                p.tx_bytes,
                opt(&p.persistent_keepalive)
            )
        })
        .collect();
    format!(
        r#"{{"version":"{VERSION}","interfaces":[{}],"peers":[{}]}}"#,
        ifaces.join(","),
        peers.join(",")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const DUMP: &str = "wg0\tPRIVATEKEY=\tPUBKEYSERVER=\t51820\toff\n\
        wg0\tPEERA=\tPRESHARED=\t203.0.113.7:51820\t10.8.0.2/32,fd00::2/128\t1791380000\t1024\t2048\t25\n\
        wg0\tPEERB=\t(none)\t(none)\t10.8.0.3/32\t0\t0\t0\toff\n";

    #[test]
    fn parses_dump_without_secrets() {
        let (ifaces, peers) = parse_dump(DUMP);
        assert_eq!(
            ifaces,
            vec![Interface {
                name: "wg0".into(),
                public_key: "PUBKEYSERVER=".into(),
                listen_port: Some(51820)
            }]
        );
        assert_eq!(peers.len(), 2);
        assert_eq!(peers[0].endpoint.as_deref(), Some("203.0.113.7:51820"));
        assert_eq!(peers[0].allowed_ips, ["10.8.0.2/32", "fd00::2/128"]);
        assert_eq!(peers[0].latest_handshake, Some(1791380000));
        assert_eq!((peers[0].rx_bytes, peers[0].tx_bytes), (1024, 2048));
        assert_eq!(peers[0].persistent_keepalive, Some(25));
        assert_eq!(peers[1].endpoint, None);
        assert_eq!(peers[1].latest_handshake, None);
        assert_eq!(peers[1].persistent_keepalive, None);

        let names = parse_names("# comment\nPEERA= Janick's phone\n\n");
        let json = to_json(&(ifaces, peers), &names);
        assert!(!json.contains("PRIVATEKEY") && !json.contains("PRESHARED"));
        assert!(json.contains(r#""name":"Janick's phone""#));
        assert!(json.contains(r#""allowed_ips":["10.8.0.2/32","fd00::2/128"]"#));
        assert!(json.contains(r#""latest_handshake":null"#));
    }

    #[test]
    fn escapes_json() {
        assert_eq!(json_str("a\"b\\c\n\u{1}"), r#""a\"b\\c\n\u0001""#);
    }

    #[test]
    fn checks_the_bearer_token() {
        let req = |auth: Option<&str>| Request {
            method: "GET".into(),
            path: "/peers".into(),
            headers: auth
                .map(|a| [("authorization".to_owned(), a.to_owned())].into())
                .unwrap_or_default(),
        };
        let token = "x".repeat(32);
        assert!(authorized(&req(Some(&format!("Bearer {token}"))), &token));
        assert!(!authorized(&req(Some("Bearer nope")), &token));
        assert!(!authorized(&req(Some(&token)), &token));
        assert!(!authorized(&req(None), &token));
        assert!(constant_time_eq(b"abc", b"abc"));
        assert!(!constant_time_eq(b"abc", b"abd"));
    }

    #[test]
    fn reads_request_head() {
        let raw = b"GET /peers HTTP/1.1\r\nHost: x\r\nAuthorization: Bearer t\r\n\r\n";
        let req = read_request(&mut &raw[..]).unwrap();
        assert_eq!((req.method.as_str(), req.path.as_str()), ("GET", "/peers"));
        assert_eq!(req.headers["authorization"], "Bearer t");
        assert!(read_request(&mut &b"GET /peers HTTP/1.1\r\nHost: x\r\n"[..]).is_none());
    }

    #[test]
    fn maps_ipv4_in_ipv6() {
        let ip: IpAddr = "::ffff:10.0.20.5".parse().unwrap();
        assert_eq!(canonical(ip), "10.0.20.5".parse::<IpAddr>().unwrap());
    }
}
