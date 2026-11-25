//! The JSON API: pairing for viewers, and the owner's controls (this computer's
//! window only: loopback and the owner token).

use crate::http::{Request, Response};
use crate::state::{lock, State};
use serde::Deserialize;
use std::net::IpAddr;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use subtle_eq::ct_eq;
use wilive_core::devices::Device;
use wilive_core::log::Kind;
use wilive_core::pair::{random_hex, Responder};
use wilive_core::pending::{Refuse, Status};

mod subtle_eq {
    /// Constant-time string equality for the owner token.
    pub fn ct_eq(a: &str, b: &str) -> bool {
        a.len() == b.len()
            && a.bytes()
                .zip(b.bytes())
                .fold(0u8, |acc, (x, y)| acc | (x ^ y))
                == 0
    }
}

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// "Chrome on Android", "Safari on iPhone": enough for the owner to recognise a
/// device, never the whole User-Agent.
pub fn agent_summary(ua: &str) -> String {
    let os = if ua.contains("iPhone") {
        "iPhone"
    } else if ua.contains("iPad") {
        "iPad"
    } else if ua.contains("Android") {
        "Android"
    } else if ua.contains("CrOS") {
        "ChromeOS"
    } else if ua.contains("Mac OS X") || ua.contains("Macintosh") {
        "Mac"
    } else if ua.contains("Windows") {
        "Windows"
    } else if ua.contains("Linux") {
        "Linux"
    } else {
        ""
    };
    let browser = if ua.contains("Edg/") {
        "Edge"
    } else if ua.contains("OPR/") {
        "Opera"
    } else if ua.contains("Firefox/") || ua.contains("FxiOS") {
        "Firefox"
    } else if ua.contains("Chrome/") || ua.contains("CriOS") {
        "Chrome"
    } else if ua.contains("Safari/") {
        "Safari"
    } else {
        "A browser"
    };
    if os.is_empty() {
        browser.to_string()
    } else {
        format!("{browser} on {os}")
    }
}

fn hex32(s: &str) -> Option<[u8; 32]> {
    if s.len() != 64 {
        return None;
    }
    hex::decode(s).ok()?.try_into().ok()
}
fn hex16(s: &str) -> Option<[u8; 16]> {
    if s.len() != 32 {
        return None;
    }
    hex::decode(s).ok()?.try_into().ok()
}

fn refuse(r: Refuse) -> Response {
    match r {
        Refuse::Busy => Response::error(429, "busy"),
        Refuse::Unknown => Response::error(404, "unknown"),
        Refuse::Order => Response::error(409, "order"),
        Refuse::Proof => Response::error(403, "proof"),
    }
}

fn body<T: for<'a> Deserialize<'a>>(req: &Request) -> Result<T, Response> {
    serde_json::from_slice(&req.body).map_err(|_| Response::error(400, "body"))
}

#[derive(Deserialize)]
struct Start {
    name: String,
    v_pub: String,
    commit: String,
}
#[derive(Deserialize)]
struct Reveal {
    req: String,
    n_v: String,
}
#[derive(Deserialize)]
struct ReqId {
    req: String,
}
#[derive(Deserialize)]
struct DevId {
    id: String,
}
#[derive(Deserialize)]
struct Rename {
    id: String,
    name: String,
}
#[derive(Deserialize)]
struct On {
    on: bool,
}
#[derive(Deserialize)]
struct Switch {
    name: String,
    on: bool,
}

/// Whether this request comes from the owner's window.
pub fn is_owner(st: &State, req: &Request, peer: IpAddr) -> bool {
    peer.is_loopback()
        && req
            .header("x-wilive-owner")
            .is_some_and(|t| ct_eq(t, &st.owner_token))
}

/// Routes `/api/...`. `None`: not an API path.
pub fn handle(st: &Arc<State>, req: &Request, peer: IpAddr, urls: &[String]) -> Option<Response> {
    let p = req.path.as_str();
    if !p.starts_with("/api/") {
        return None;
    }
    let post = req.method == "POST";
    Some(match (req.method.as_str(), p) {
        ("GET", "/api/info") => Response::json(
            200,
            &serde_json::json!({
                "product": "Wi-Live", "version": VERSION, "name": st.name,
                "paused": st.paused.load(Ordering::Relaxed),
            }),
        ),
        ("POST", "/api/pair/start") => pair_start(st, req, peer),
        ("POST", "/api/pair/reveal") => pair_reveal(st, req, peer),
        ("GET", "/api/pair/status") => {
            let req_id = req.query.get("req").cloned().unwrap_or_default();
            match lock(&st.pending).status(&req_id, peer, st.ms()) {
                Ok(Status::Started) => Response::json(200, &serde_json::json!({"state":"started"})),
                Ok(Status::Waiting) => Response::json(200, &serde_json::json!({"state":"waiting"})),
                Ok(Status::Allowed { device }) => {
                    Response::json(200, &serde_json::json!({"state":"allowed","device":device}))
                }
                Ok(Status::Denied) => Response::json(200, &serde_json::json!({"state":"denied"})),
                Err(r) => refuse(r),
            }
        }
        (_, p) if p.starts_with("/api/owner/") => {
            if !is_owner(st, req, peer) {
                return Some(Response::error(403, "owner"));
            }
            owner(st, req, urls)
        }
        (_, "/api/info" | "/api/pair/start" | "/api/pair/reveal" | "/api/pair/status") => {
            Response::error(405, "method")
        }
        _ if post => Response::error(404, "unknown"),
        _ => Response::error(404, "unknown"),
    })
}

fn pair_start(st: &State, req: &Request, peer: IpAddr) -> Response {
    let b: Start = match body(req) {
        Ok(b) => b,
        Err(r) => return r,
    };
    let (Some(v_pub), Some(commit)) = (hex32(&b.v_pub), hex32(&b.commit)) else {
        return Response::error(400, "body");
    };
    if st.paused.load(Ordering::Relaxed) {
        return Response::error(503, "paused");
    }
    let agent = agent_summary(req.header("user-agent").unwrap_or(""));
    let r = lock(&st.pending).start(
        &b.name,
        &agent,
        peer,
        v_pub,
        commit,
        Responder::new(),
        random_hex(16),
        st.ms(),
    );
    match r {
        Ok((id, a_pub, n_a)) => Response::json(
            200,
            &serde_json::json!({"req": id, "a_pub": hex::encode(a_pub), "n_a": hex::encode(n_a)}),
        ),
        Err(e) => refuse(e),
    }
}

fn pair_reveal(st: &State, req: &Request, peer: IpAddr) -> Response {
    let b: Reveal = match body(req) {
        Ok(b) => b,
        Err(r) => return r,
    };
    let Some(n_v) = hex16(&b.n_v) else {
        return Response::error(400, "body");
    };
    let mut pend = lock(&st.pending);
    // Only the address that started it may reveal (status() checks the same).
    if pend.status(&b.req, peer, st.ms()).is_err() {
        return refuse(Refuse::Unknown);
    }
    match pend.reveal(&b.req, &n_v, st.ms()) {
        Ok(_) => {
            let v = pend.views(st.ms()).into_iter().find(|v| v.req == b.req);
            drop(pend);
            if let Some(v) = v {
                st.record(Kind::PairAsked, &v.name, &v.addr, &v.agent);
            }
            Response::json(200, &serde_json::json!({"ok": true}))
        }
        Err(e) => {
            drop(pend);
            if e == Refuse::Proof {
                st.record(
                    Kind::Refused,
                    "",
                    &peer.to_string(),
                    "pairing proof did not match",
                );
            }
            refuse(e)
        }
    }
}

/// Everything the owner's window shows, in one reply.
pub fn owner_state(st: &State, urls: &[String]) -> serde_json::Value {
    let live: Vec<serde_json::Value> = lock(&st.live).iter().map(|l| l.view()).collect();
    serde_json::json!({
        "name": st.name, "version": VERSION, "urls": urls,
        "qr": urls.iter().map(|u| crate::qr::data_uri(u).unwrap_or_default()).collect::<Vec<_>>(),
        "paused": st.paused.load(Ordering::Relaxed),
        "live": live,
        "pending": lock(&st.pending).views(st.ms()),
        "devices": lock(&st.devices).views(),
        "log": lock(&st.log).recent(100),
        "capture_allowed": st.host.capture_allowed(),
        "switches": st.host.switches().into_iter().map(|(k, v)| (k.to_string(), serde_json::Value::Bool(v))).collect::<serde_json::Map<_, _>>(),
    })
}

fn owner(st: &State, req: &Request, urls: &[String]) -> Response {
    let ok = || Response::json(200, &serde_json::json!({"ok": true}));
    match (req.method.as_str(), req.path.as_str()) {
        ("GET", "/api/owner/state") => Response::json(200, &owner_state(st, urls)),
        ("POST", "/api/owner/allow") => {
            let b: ReqId = match body(req) {
                Ok(b) => b,
                Err(r) => return r,
            };
            let a = match lock(&st.pending).allow(&b.req, random_hex(16), st.ms()) {
                Ok(a) => a,
                Err(e) => return refuse(e),
            };
            let dropped = lock(&st.devices).add(Device::new(
                a.device.clone(),
                a.name.clone(),
                a.agent.clone(),
                a.addr.to_string(),
                &a.key,
                crate::state::unix_now(),
            ));
            st.save_devices();
            st.record(Kind::PairAllowed, &a.name, &a.addr.to_string(), &a.agent);
            if let Some(d) = dropped {
                st.record(
                    Kind::Forgotten,
                    &d.name,
                    "",
                    "the device list was full; seen longest ago",
                );
            }
            ok()
        }
        ("POST", "/api/owner/deny") => {
            let b: ReqId = match body(req) {
                Ok(b) => b,
                Err(r) => return r,
            };
            let v = lock(&st.pending)
                .views(st.ms())
                .into_iter()
                .find(|v| v.req == b.req);
            match lock(&st.pending).deny(&b.req, st.ms()) {
                Ok(()) => {
                    if let Some(v) = v {
                        st.record(Kind::PairDenied, &v.name, &v.addr, &v.agent);
                    }
                    ok()
                }
                Err(e) => refuse(e),
            }
        }
        ("POST", "/api/owner/forget") => {
            let b: DevId = match body(req) {
                Ok(b) => b,
                Err(r) => return r,
            };
            let name = lock(&st.devices).name_of(&b.id);
            if !lock(&st.devices).forget(&b.id) {
                return refuse(Refuse::Unknown);
            }
            st.save_devices();
            st.end_where(|l| l.device == b.id);
            st.record(Kind::Forgotten, &name.unwrap_or_default(), "", "");
            ok()
        }
        ("POST", "/api/owner/rename") => {
            let b: Rename = match body(req) {
                Ok(b) => b,
                Err(r) => return r,
            };
            if !lock(&st.devices).rename(&b.id, &b.name) {
                return refuse(Refuse::Unknown);
            }
            st.save_devices();
            ok()
        }
        ("POST", "/api/owner/end") => {
            let b: DevId = match body(req) {
                Ok(b) => b,
                Err(r) => return r,
            };
            if st.end_where(|l| l.id == b.id) == 0 {
                return refuse(Refuse::Unknown);
            }
            ok()
        }
        ("POST", "/api/owner/pause") => {
            let b: On = match body(req) {
                Ok(b) => b,
                Err(r) => return r,
            };
            let was = st.paused.swap(b.on, Ordering::Relaxed);
            if b.on {
                st.end_where(|_| true);
            }
            if was != b.on {
                st.record(if b.on { Kind::Paused } else { Kind::Resumed }, "", "", "");
            }
            ok()
        }
        ("POST", "/api/owner/capture") => {
            st.host.ask_capture();
            ok()
        }
        ("POST", "/api/owner/switch") => {
            let b: Switch = match body(req) {
                Ok(b) => b,
                Err(r) => return r,
            };
            if !st.host.set_switch(&b.name, b.on) {
                return refuse(Refuse::Unknown);
            }
            ok()
        }
        _ => Response::error(404, "unknown"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agents_are_summarised_not_copied() {
        let cases = [
            ("Mozilla/5.0 (Linux; Android 14; Pixel 7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0 Mobile Safari/537.36", "Chrome on Android"),
            ("Mozilla/5.0 (iPhone; CPU iPhone OS 18_1 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.1 Mobile/15E148 Safari/604.1", "Safari on iPhone"),
            ("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.1 Safari/605.1.15", "Safari on Mac"),
            ("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0 Safari/537.36 Edg/131.0", "Edge on Windows"),
            ("Mozilla/5.0 (X11; Linux x86_64; rv:133.0) Gecko/20100101 Firefox/133.0", "Firefox on Linux"),
            ("curl/8.7.1", "A browser"),
        ];
        for (ua, want) in cases {
            assert_eq!(agent_summary(ua), want, "{ua}");
        }
    }

    #[test]
    fn the_owner_token_compare_is_exact() {
        assert!(ct_eq("abc", "abc"));
        assert!(!ct_eq("abc", "abd"));
        assert!(!ct_eq("abc", "abcd"));
    }
}
