//! One port for everything a viewer needs: the dashboard's files, the pairing API,
//! the owner's API (this computer's window only) and the signaling WebSocket.
//!
//! Defences (R-27): private-network peers only; at most [`MAX_CONNS`] connections,
//! [`MAX_PER_ADDR`] from one address; capped heads and bodies with deadlines; a
//! Host header that must name this computer (no DNS rebinding); an Origin that
//! must match it on anything that changes state; and an address that shows 5 bad
//! proofs in 10 minutes cannot watch or pair for 10 minutes (loopback never: the
//! owner's window is there). Its page still loads and says why.

pub mod api;
pub mod health;
pub mod http;
pub mod qr;
pub mod signal;
pub mod state;
pub mod web;

use http::{read_request, write_response, ReadError, Request, Response};
use signal::{Media, Strikes};
use state::{lock, State};
use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Page loads leave idle keep-alive sockets for a few seconds; 64 refused a
/// browser's 16th quick reload (tests/e2e/test_endurance.py).
pub const MAX_CONNS: usize = 128;
/// One device: a page, its reloads' lingering keep-alive sockets and a viewer's
/// signaling socket. Seen in tests: 12 refused a browser's own page loads.
pub const MAX_PER_ADDR: usize = 24;
/// Tried in order; the first free one is used. The window shows the address.
pub const PORTS: std::ops::RangeInclusive<u16> = 47900..=47919;
const KEEP_ALIVE_IDLE: Duration = Duration::from_secs(5);
const KEEP_ALIVE_MAX: usize = 200;

/// Addresses a viewer may come from: the private ranges a home network uses,
/// link-local, and loopback. Never the public internet.
pub fn private_peer(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v) => v.is_private() || v.is_loopback() || v.is_link_local(),
        IpAddr::V6(v) => {
            if let Some(m) = v.to_ipv4_mapped() {
                return private_peer(IpAddr::V4(m));
            }
            v.is_loopback()
                || (v.segments()[0] & 0xfe00) == 0xfc00
                || (v.segments()[0] & 0xffc0) == 0xfe80
        }
    }
}

/// A Host header this computer answers to: an IP literal, `localhost`, or a
/// `.local` name. A web page elsewhere that rebinds its own name to this
/// computer's address sends its own name and is refused.
pub fn host_ok(host: &str) -> bool {
    let h = if let Some(rest) = host.strip_prefix('[') {
        // [v6]:port
        return rest
            .split(']')
            .next()
            .is_some_and(|v6| v6.parse::<std::net::Ipv6Addr>().is_ok());
    } else {
        host.rsplit_once(':')
            .map(|(h, p)| {
                if p.bytes().all(|c| c.is_ascii_digit()) {
                    h
                } else {
                    host
                }
            })
            .unwrap_or(host)
    };
    let h = h.to_ascii_lowercase();
    h.parse::<IpAddr>().is_ok()
        || h == "localhost"
        || (h.ends_with(".local")
            && h.len() > 6
            && h.bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'.'))
}

/// For requests that change something: an Origin, when sent, must be this very
/// host. Browsers always send one on POST and WebSocket.
pub fn origin_ok(req: &Request) -> bool {
    match (req.header("origin"), req.header("host")) {
        (None, _) => req.method == "GET" && !req.is_websocket(),
        (Some(o), Some(h)) => o == format!("http://{h}") || o == format!("https://{h}"),
        (Some(_), None) => false,
    }
}

pub struct Server {
    pub port: u16,
    pub state: Arc<State>,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        state::lock(&self.state.live)
            .iter()
            .for_each(|l| l.stop.store(true, Ordering::Relaxed));
        let _ = TcpStream::connect_timeout(
            &SocketAddr::from(([127, 0, 0, 1], self.port)),
            Duration::from_millis(200),
        );
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

struct Ctx {
    state: Arc<State>,
    media: Media,
    strikes: Strikes,
    stop: Arc<AtomicBool>,
    conns: Mutex<HashMap<IpAddr, usize>>,
    port: u16,
}

impl Ctx {
    fn claim(&self, ip: IpAddr) -> bool {
        let mut c = lock(&self.conns);
        let total: usize = c.values().sum();
        let mine = c.get(&ip).copied().unwrap_or(0);
        // Loopback has no per-address cap: the owner's window is there.
        if total >= MAX_CONNS || (mine >= MAX_PER_ADDR && !ip.is_loopback()) {
            return false;
        }
        *c.entry(ip).or_default() += 1;
        true
    }
    fn release(&self, ip: IpAddr) {
        let mut c = lock(&self.conns);
        if let Some(n) = c.get_mut(&ip) {
            *n -= 1;
            if *n == 0 {
                c.remove(&ip);
            }
        }
    }
    /// The addresses a viewer can type, for the owner's window.
    fn urls(&self) -> Vec<String> {
        lan_addrs()
            .into_iter()
            .map(|ip| format!("http://{ip}:{}/", self.port))
            .collect()
    }
}

/// This computer's private IPv4 addresses, best first (the one with a default
/// route first, found without sending anything).
pub fn lan_addrs() -> Vec<IpAddr> {
    let mut out = Vec::new();
    if let Ok(s) = std::net::UdpSocket::bind("0.0.0.0:0") {
        // connect() on UDP only picks the route; nothing is sent.
        if s.connect("192.168.255.255:9").is_ok() || s.connect("10.255.255.255:9").is_ok() {
            if let Ok(a) = s.local_addr() {
                if private_peer(a.ip()) && !a.ip().is_loopback() && !a.ip().is_unspecified() {
                    out.push(a.ip());
                }
            }
        }
    }
    out
}

/// Binds the first free port in [`PORTS`] (or exactly `port`).
pub fn start(port: Option<u16>, state: Arc<State>, media: Media) -> std::io::Result<Server> {
    let listener = match port {
        Some(p) => TcpListener::bind(("0.0.0.0", p))?,
        None => PORTS
            .clone()
            .find_map(|p| TcpListener::bind(("0.0.0.0", p)).ok())
            .ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::AddrInUse, "no free port in 47900-47919")
            })?,
    };
    let port = listener.local_addr()?.port();
    let stop = Arc::new(AtomicBool::new(false));
    let ctx = Arc::new(Ctx {
        state: state.clone(),
        media,
        strikes: Strikes::default(),
        stop: stop.clone(),
        conns: Mutex::default(),
        port,
    });
    let thread = std::thread::Builder::new()
        .name("wilive-accept".into())
        .spawn(move || {
            for conn in listener.incoming() {
                if ctx.stop.load(Ordering::Relaxed) {
                    break;
                }
                let Ok(stream) = conn else { continue };
                let Ok(peer) = stream.peer_addr() else {
                    continue;
                };
                if !private_peer(peer.ip()) || !ctx.claim(peer.ip()) {
                    continue; // dropped: closes the connection
                }
                let c = ctx.clone();
                // Small stacks: a connection thread parses a request and answers it.
                let spawned = std::thread::Builder::new()
                    .name("wilive-conn".into())
                    .stack_size(512 * 1024)
                    .spawn(move || {
                        connection(&c, stream, peer);
                        c.release(peer.ip());
                    });
                if spawned.is_err() {
                    ctx.release(peer.ip());
                }
            }
        })?;
    Ok(Server {
        port,
        state,
        stop,
        thread: Some(thread),
    })
}

fn connection(c: &Arc<Ctx>, stream: TcpStream, peer: SocketAddr) {
    let _ = stream.set_nodelay(true);
    let _ = stream.set_write_timeout(Some(Duration::from_secs(5)));
    let mut carry = Vec::new();
    let mut reader = match stream.try_clone() {
        Ok(s) => s,
        Err(_) => return,
    };
    let mut writer = stream;
    for n in 0..KEEP_ALIVE_MAX {
        let r = {
            let s2 = writer.try_clone();
            let mut set = |d: Duration| {
                if let Ok(s) = &s2 {
                    let _ =
                        s.set_read_timeout(Some(if n == 0 { d } else { d.min(KEEP_ALIVE_IDLE) }));
                }
            };
            read_request(&mut reader, &mut carry, &mut set)
        };
        let req = match r {
            Ok(r) => r,
            Err(ReadError::Closed) | Err(ReadError::Io(_)) => return,
            Err(ReadError::TooBig) => {
                let _ = write_response(&mut writer, &Response::error(413, "too_big"), false, false);
                return;
            }
            Err(ReadError::Malformed) | Err(ReadError::Slow) => {
                let _ = write_response(
                    &mut writer,
                    &Response::error(400, "bad_request"),
                    false,
                    false,
                );
                return;
            }
        };
        if !req.header("host").is_some_and(host_ok) {
            let _ = write_response(&mut writer, &Response::error(421, "host"), false, false);
            return;
        }
        if !origin_ok(&req) && (req.method != "GET" || req.is_websocket()) {
            let _ = write_response(&mut writer, &Response::error(403, "origin"), false, false);
            return;
        }
        if req.is_websocket() {
            if req.path != "/v1/signal" {
                let _ = write_response(&mut writer, &Response::error(404, "unknown"), false, false);
                return;
            }
            let Some(key) = req.header("sec-websocket-key").map(str::to_string) else {
                let _ = write_response(
                    &mut writer,
                    &Response::error(400, "bad_request"),
                    false,
                    false,
                );
                return;
            };
            let agent = api::agent_summary(req.header("user-agent").unwrap_or(""));
            if let Ok(ws) = signal::upgrade(writer, &key) {
                if let Err(e) =
                    signal::serve(&c.state, &c.media, &c.strikes, &c.stop, ws, peer, agent)
                {
                    log::debug!("viewer {peer}: {e}");
                }
            }
            return;
        }
        let keep = req.keep_alive() && n + 1 < KEEP_ALIVE_MAX;
        let resp = route(c, &req, peer.ip());
        if write_response(&mut writer, &resp, keep, req.method == "HEAD").is_err() || !keep {
            return;
        }
    }
}

fn route(c: &Ctx, req: &Request, peer: IpAddr) -> Response {
    if let Some(r) = api::handle(&c.state, req, peer, &c.urls()) {
        return r;
    }
    match req.method.as_str() {
        "GET" | "HEAD" => web::serve(&req.path, req.header("if-none-match"))
            .unwrap_or_else(|| Response::error(404, "unknown")),
        _ => Response::error(405, "method"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_home_network_addresses_are_answered() {
        for ok in [
            "192.168.0.5",
            "10.1.2.3",
            "172.16.0.1",
            "127.0.0.1",
            "169.254.1.1",
            "::1",
            "fe80::1",
            "fd00::5",
            "::ffff:192.168.1.2",
        ] {
            assert!(private_peer(ok.parse().unwrap()), "{ok}");
        }
        for no in [
            "8.8.8.8",
            "100.64.0.1",
            "2001:4860::8888",
            "::ffff:1.1.1.1",
            "172.32.0.1",
        ] {
            assert!(!private_peer(no.parse().unwrap()), "{no}");
        }
    }

    #[test]
    fn the_host_must_name_this_computer() {
        for ok in [
            "192.168.0.5:47900",
            "127.0.0.1",
            "localhost:47900",
            "santhus-mac.local:47900",
            "[fe80::1]:47900",
            "[::1]",
        ] {
            assert!(host_ok(ok), "{ok}");
        }
        for no in [
            "evil.example.com",
            "evil.example.com:47900",
            "attacker.local.evil.com",
            ".local",
            "a b.local",
            "[nope]:1",
            "",
        ] {
            assert!(!host_ok(no), "{no}");
        }
    }

    #[test]
    fn a_change_needs_an_origin_that_is_this_host() {
        let mk = |m: &str, o: Option<&str>, ws: bool| {
            let mut h = HashMap::new();
            h.insert("host".to_string(), "192.168.0.5:47900".to_string());
            if let Some(o) = o {
                h.insert("origin".to_string(), o.to_string());
            }
            if ws {
                h.insert("upgrade".into(), "websocket".into());
                h.insert("connection".into(), "Upgrade".into());
            }
            Request {
                method: m.into(),
                path: "/".into(),
                query: HashMap::new(),
                headers: h,
                body: vec![],
            }
        };
        assert!(origin_ok(&mk(
            "POST",
            Some("http://192.168.0.5:47900"),
            false
        )));
        assert!(origin_ok(&mk("GET", None, false)), "a plain page load");
        assert!(!origin_ok(&mk("POST", None, false)));
        assert!(!origin_ok(&mk(
            "POST",
            Some("http://evil.example.com"),
            false
        )));
        assert!(
            !origin_ok(&mk("GET", None, true)),
            "a WebSocket always needs it"
        );
        assert!(
            !origin_ok(&mk("GET", Some("http://192.168.0.5:47901"), true)),
            "another port is another origin"
        );
    }
}
