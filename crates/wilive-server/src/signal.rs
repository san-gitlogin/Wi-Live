//! One viewer's signaling WebSocket: proof, offer and answer, then the session's
//! control channel. Ported from HID-Fi Companion's `server.rs`; admission is
//! Wi-Live's own (docs/DESIGN.md "Trust").
//!
//! Wire format (JSON text frames):
//!   app    -> {"t":"chal","c"}                          at once
//!   viewer -> {"t":"hello","id","mac"}                  within 5 s
//!   app    -> {"t":"ready","name","fp","fp_mac","displays","display","audio"?}
//!           | {"t":"error","reason"}  reason: hello | device | paused | busy
//!   viewer -> {"t":"display","id"}?  {"t":"offer","sdp"}  within 10 s
//!   app    -> {"t":"answer","sdp"}
//!   app    -> {"t":"stats","fps"}                       every second while live
//!   app    -> {"t":"displays",...} {"t":"display","id","fallback"} {"t":"audio",...}
//!   viewer -> {"t":"key"} {"t":"display","id"} {"t":"lat","p50","p99"} {"t":"size","w","h"}
//!             {"t":"quality","q"} (auto | sharp | smooth | saver) {"t":"bye"}
//!   app    -> {"t":"end","reason"}  reason: ended | paused | forgotten | capture | permission | ...
//!
//! Before the proof is accepted only a JSON parse and one HMAC happen.

use crate::state::{lock, unix_now, Live, State};
use serde::Deserialize;
use std::collections::{HashMap, VecDeque};
use std::io::ErrorKind;
use std::net::{IpAddr, SocketAddr, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::sync_channel;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tungstenite::protocol::{Role, WebSocketConfig};
use tungstenite::{Message, WebSocket};
use wilive_core::log::Kind;
use wilive_stream::display::{self, Display};
use wilive_stream::pipeline::{Pipeline, ToViewer, MAIN};
use wilive_stream::rtc::{self, DtlsCert, End, ToSession};

/// Sessions, not devices: a device watching all four displays at once holds four.
pub const MAX_VIEWERS: usize = 16;
pub const HELLO_DEADLINE: Duration = Duration::from_secs(5);
pub const OFFER_DEADLINE: Duration = Duration::from_secs(10);
pub const MAX_MESSAGE: usize = 64 * 1024;
const BAD_LIMIT: usize = 5;
const BAD_WINDOW: Duration = Duration::from_secs(600);
const DISPLAYS_EVERY: Duration = Duration::from_secs(3);

/// The displays viewers may choose from, asked when one connects and every few
/// seconds while it watches (monitors come and go).
pub type Displays = Box<dyn Fn() -> Vec<Display> + Send + Sync>;

/// What a session needs from the app.
pub struct Media {
    pub pipeline: Arc<Pipeline>,
    pub audio: Option<Arc<wilive_stream::audio::AudioHub>>,
    pub cert: DtlsCert,
    pub fp: String,
    pub displays: Displays,
}

/// Addresses that showed bad proofs, to refuse them for a while.
#[derive(Default)]
pub struct Strikes(Mutex<HashMap<IpAddr, VecDeque<Instant>>>);

impl Strikes {
    /// Never true for loopback: the owner's own window lives there, and a lockout
    /// must never lock the owner out of their own computer.
    pub fn blocked(&self, ip: IpAddr) -> bool {
        if ip.is_loopback() {
            return false;
        }
        let mut b = lock(&self.0);
        let now = Instant::now();
        let q = b.entry(ip).or_default();
        q.retain(|t| now.duration_since(*t) < BAD_WINDOW);
        let blocked = q.len() >= BAD_LIMIT;
        if q.is_empty() {
            b.remove(&ip);
        }
        if b.len() > 256 {
            b.retain(|_, q| q.len() >= BAD_LIMIT);
        }
        blocked
    }
    pub fn strike(&self, ip: IpAddr) {
        lock(&self.0)
            .entry(ip)
            .or_default()
            .push_back(Instant::now());
    }
}

#[derive(Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
enum FromViewer {
    Hello {
        id: String,
        mac: String,
    },
    Offer {
        sdp: String,
    },
    Display {
        id: u32,
    },
    Lat {
        p50: u32,
        p99: u32,
    },
    Size {
        w: u32,
        h: u32,
    },
    /// "auto" | "sharp" | "smooth" | "saver": this viewer's choice of size.
    Quality {
        q: String,
    },
    Key,
    Bye,
}

fn send(ws: &mut WebSocket<TcpStream>, v: serde_json::Value) -> tungstenite::Result<()> {
    ws.send(Message::text(v.to_string()))
}

enum Got {
    Msg(Option<FromViewer>),
    Quiet,
}

fn read(ws: &mut WebSocket<TcpStream>, deadline: Instant) -> Result<Got, String> {
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return Ok(Got::Quiet);
        }
        let _ = ws
            .get_ref()
            .set_read_timeout(Some(left.min(Duration::from_millis(250))));
        match ws.read() {
            Ok(Message::Text(t)) => return Ok(Got::Msg(serde_json::from_str(&t).ok())),
            Ok(Message::Close(_)) => return Err("closed".into()),
            Ok(_) => continue,
            Err(tungstenite::Error::Io(e))
                if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) =>
            {
                continue
            }
            Err(e) => return Err(e.to_string()),
        }
    }
}

fn audio_json(s: &wilive_stream::audio::State) -> serde_json::Value {
    use wilive_stream::audio::Share;
    let word = |x: &Share| match x {
        Share::On => "on",
        Share::Off => "off",
        Share::Unavailable(_) => "unavailable",
    };
    let why = |x: &Share| match x {
        Share::Unavailable(w) => serde_json::Value::String(w.chars().take(120).collect()),
        _ => serde_json::Value::Null,
    };
    serde_json::json!({"t":"audio","system":word(&s.system),"mic":word(&s.mic),
                       "why":{"system":why(&s.system),"mic":why(&s.mic)}})
}

/// Completes the WebSocket handshake for a request already read.
pub fn upgrade(mut stream: TcpStream, key: &str) -> std::io::Result<WebSocket<TcpStream>> {
    use std::io::Write;
    let accept = tungstenite::handshake::derive_accept_key(key.as_bytes());
    let mut h = format!("HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {accept}\r\n");
    for (k, v) in crate::http::SECURITY_HEADERS {
        h.push_str(&format!("{k}: {v}\r\n"));
    }
    h.push_str("\r\n");
    stream.write_all(h.as_bytes())?;
    let cfg = WebSocketConfig::default()
        .max_message_size(Some(MAX_MESSAGE))
        .max_frame_size(Some(MAX_MESSAGE));
    Ok(WebSocket::from_raw_socket(stream, Role::Server, Some(cfg)))
}

/// Removes this viewer from the live list on every way out.
struct LiveGuard<'a> {
    st: &'a State,
    live: Arc<Live>,
}
impl Drop for LiveGuard<'_> {
    fn drop(&mut self) {
        lock(&self.st.live).retain(|l| !Arc::ptr_eq(l, &self.live));
    }
}

pub fn serve(
    st: &Arc<State>,
    media: &Media,
    strikes: &Strikes,
    stopping: &AtomicBool,
    mut ws: WebSocket<TcpStream>,
    peer: SocketAddr,
    agent: String,
) -> Result<(), String> {
    let local_ip = ws.get_ref().local_addr().map_err(|e| e.to_string())?.ip();
    let _ = ws.get_ref().set_write_timeout(Some(Duration::from_secs(5)));
    let _ = ws.get_ref().set_nodelay(true);
    let addr = peer.ip().to_string();

    // An address that showed too many bad proofs gets a plain answer, not a dead
    // socket: its page can say why. Its dashboard still loads.
    if strikes.blocked(peer.ip()) {
        let _ = send(&mut ws, serde_json::json!({"t":"error","reason":"blocked"}));
        return Err("blocked".into());
    }
    let chal = wilive_core::pair::random_hex(16);
    send(&mut ws, serde_json::json!({"t":"chal","c":chal})).map_err(|e| e.to_string())?;
    let (id, mac) = match read(&mut ws, Instant::now() + HELLO_DEADLINE) {
        Ok(Got::Msg(Some(FromViewer::Hello { id, mac }))) => (id, mac),
        _ => {
            strikes.strike(peer.ip());
            let _ = send(&mut ws, serde_json::json!({"t":"error","reason":"hello"}));
            return Err("no hello".into());
        }
    };
    let mac = hex::decode(&mac).unwrap_or_default();
    let known = lock(&st.devices).name_of(&id);
    let admitted = id.len() <= 64 && lock(&st.devices).admit(&id, &chal, &mac, &addr, unix_now());
    if !admitted {
        // A removed device asking again is normal (its page goes back to asking);
        // a known device with a wrong proof is someone trying keys.
        match &known {
            Some(name) => {
                strikes.strike(peer.ip());
                st.record(Kind::Refused, name, &addr, "wrong proof for this device");
            }
            None => st.record(Kind::Refused, "", &addr, "not an allowed device"),
        }
        let _ = send(&mut ws, serde_json::json!({"t":"error","reason":"device"}));
        let _ = ws.close(None);
        return Err("bad proof".into());
    }
    let name = lock(&st.devices).name_of(&id).unwrap_or_default();
    // Saved last_seen/last_addr; cheap, and the owner's list stays true.
    st.save_devices();
    if st.paused.load(Ordering::Relaxed) {
        let _ = send(&mut ws, serde_json::json!({"t":"error","reason":"paused"}));
        return Err("paused".into());
    }
    let live = Arc::new(Live {
        id: format!("s{}", st.next_id()),
        device: id.clone(),
        name: name.clone(),
        addr: peer.ip(),
        agent,
        since: unix_now(),
        stop: AtomicBool::new(false),
        display: AtomicU32::new(0),
        fps: AtomicU32::new(0),
        lat_p50: AtomicU32::new(0),
        lat_p99: AtomicU32::new(0),
        sound: AtomicBool::new(false),
        width: AtomicU32::new(0),
        height: AtomicU32::new(0),
    });
    {
        let mut l = lock(&st.live);
        if l.len() >= MAX_VIEWERS {
            drop(l);
            let _ = send(&mut ws, serde_json::json!({"t":"error","reason":"busy"}));
            return Err("busy".into());
        }
        l.push(live.clone());
    }
    let _guard = LiveGuard {
        st,
        live: live.clone(),
    };

    let fp_mac = lock(&st.devices)
        .fp_mac(&id, &chal, &media.fp)
        .map(hex::encode)
        .unwrap_or_default();
    let mut shown = (media.displays)();
    send(&mut ws, {
        let mut r = serde_json::json!({"t":"ready","name":st.name,"fp":media.fp,"fp_mac":fp_mac,
            "displays":shown,"display":display::main_id(&shown)});
        if let Some(h) = &media.audio {
            r["audio"] = audio_json(&h.state());
        }
        r
    })
    .map_err(|e| e.to_string())?;

    let until = Instant::now() + OFFER_DEADLINE;
    let mut early_display: Option<u32> = None;
    let sdp = loop {
        match read(&mut ws, until) {
            Ok(Got::Msg(Some(FromViewer::Offer { sdp }))) => break sdp,
            Ok(Got::Msg(Some(FromViewer::Display { id }))) => early_display = Some(id),
            Ok(Got::Msg(_)) => {}
            Ok(Got::Quiet) | Err(_) => return Err("no offer".into()),
        }
    };
    let prepared = match rtc::prepare(&sdp, local_ip, media.cert.clone()) {
        Ok(p) => p,
        Err(e) => {
            let _ = send(
                &mut ws,
                serde_json::json!({"t":"error","reason":"offer","detail":e.to_string()}),
            );
            return Err(e.to_string());
        }
    };
    send(
        &mut ws,
        serde_json::json!({"t":"answer","sdp":prepared.answer}),
    )
    .map_err(|e| e.to_string())?;
    st.record(Kind::Watching, &name, &addr, "");
    let started = Instant::now();

    let (tx, rx) = sync_channel::<ToSession>(256);
    let (vtx, vrx) = sync_channel::<ToViewer>(8);
    let stop = Arc::new(AtomicBool::new(false));
    let fps = Arc::new(AtomicU32::new(0));
    let ended: Arc<Mutex<Option<End>>> = Arc::default();
    let sub = media.pipeline.subscribe(vtx);
    let control = media.pipeline.control(sub);
    if let Some(d) = early_display {
        if display::main_id(&shown) == Some(d) {
            control.set_display(MAIN);
        } else if shown.iter().any(|x| x.id == d) {
            control.set_display(d);
        }
    }
    let audio_sub = match (&media.audio, sdp.contains("m=audio")) {
        (Some(h), true) => {
            live.sound.store(true, Ordering::Relaxed);
            let (atx, arx) = sync_channel::<Arc<wilive_stream::audio::Packet>>(50);
            let aid = h.subscribe(atx);
            let (ftx, fstop) = (tx.clone(), stop.clone());
            let t = std::thread::Builder::new()
                .name("rtc-audio".into())
                .spawn(move || {
                    while !fstop.load(Ordering::Relaxed) {
                        match arx.recv_timeout(Duration::from_millis(500)) {
                            Ok(p) => {
                                let _ = ftx.try_send(ToSession::Audio(p));
                            }
                            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                            Err(_) => break,
                        }
                    }
                })
                .ok();
            Some((h.clone(), aid, t))
        }
        _ => None,
    };
    let mut told_audio = media.audio.as_ref().map(|h| h.state());
    let (fwd_tx, fwd_stop, fwd_ctl, fwd_ended) =
        (tx.clone(), stop.clone(), control.clone(), ended.clone());
    let forwarder = std::thread::Builder::new()
        .name("rtc-fwd".into())
        .spawn(move || {
            while !fwd_stop.load(Ordering::Relaxed) {
                match vrx.recv_timeout(Duration::from_millis(500)) {
                    Ok(m) => {
                        if fwd_tx.try_send(m.into()).is_err() {
                            fwd_ctl.request_keyframe();
                        }
                    }
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                    Err(_) => {
                        lock(&fwd_ended).get_or_insert(End::PipelineGone);
                        fwd_stop.store(true, Ordering::Relaxed);
                        break;
                    }
                }
            }
        });
    let (s2, f2, e2, ctl2) = (stop.clone(), fps.clone(), ended.clone(), control.clone());
    let session = std::thread::Builder::new()
        .name("rtc".into())
        .spawn(move || {
            let end = rtc::run(prepared, rx, tx, ctl2, s2.clone(), |n| {
                f2.store(n, Ordering::Relaxed)
            });
            lock(&e2).get_or_insert(end);
            s2.store(true, Ordering::Relaxed);
        });

    let mut last_stats = Instant::now();
    let mut last_displays = Instant::now();
    let current = |list: &[Display], d: u32| {
        if d == MAIN {
            display::main_id(list)
        } else {
            Some(d)
        }
    };
    let mut told = current(&shown, MAIN);
    live.display.store(told.unwrap_or(0), Ordering::Relaxed);
    let mut asked: Option<u32> = None;
    let mut forgotten = false;
    while !stop.load(Ordering::Relaxed)
        && !stopping.load(Ordering::Relaxed)
        && !live.stop.load(Ordering::Relaxed)
        && !st.paused.load(Ordering::Relaxed)
    {
        match read(&mut ws, Instant::now() + Duration::from_millis(250)) {
            Ok(Got::Msg(Some(FromViewer::Key))) => control.browser_wants_keyframe(),
            Ok(Got::Msg(Some(FromViewer::Display { id }))) => {
                asked = Some(id);
                if display::main_id(&shown) == Some(id) {
                    control.set_display(MAIN);
                } else if shown.iter().any(|d| d.id == id) {
                    control.set_display(id);
                }
            }
            Ok(Got::Msg(Some(FromViewer::Lat { p50, p99 }))) => {
                live.lat_p50.store(p50.min(60_000), Ordering::Relaxed);
                live.lat_p99.store(p99.min(60_000), Ordering::Relaxed);
            }
            Ok(Got::Msg(Some(FromViewer::Quality { q }))) => {
                if let Some(q) = wilive_stream::pipeline::Quality::from_word(&q) {
                    control.set_quality(q);
                }
            }
            Ok(Got::Msg(Some(FromViewer::Size { w, h }))) => {
                live.width.store(w.min(16_384), Ordering::Relaxed);
                live.height.store(h.min(16_384), Ordering::Relaxed);
            }
            Ok(Got::Msg(Some(FromViewer::Bye))) | Err(_) => break,
            Ok(_) => {}
        }
        if last_displays.elapsed() >= DISPLAYS_EVERY {
            last_displays = Instant::now();
            // A device forgotten while watching is ended within a turn.
            if lock(&st.devices).name_of(&id).is_none() {
                forgotten = true;
                break;
            }
            let now = (media.displays)();
            if now != shown {
                if control.display() != MAIN && !now.iter().any(|d| d.id == control.display()) {
                    control.set_display(MAIN);
                }
                shown = now;
                let id = current(&shown, control.display());
                if send(
                    &mut ws,
                    serde_json::json!({"t":"displays","displays":shown,"display":id}),
                )
                .is_err()
                {
                    break;
                }
                told = id;
            }
        }
        let id = current(&shown, control.display());
        if id != told {
            let moved = asked.is_some_and(|a| Some(a) != id);
            if moved {
                asked = None;
            }
            if send(
                &mut ws,
                serde_json::json!({"t":"display","id":id,"fallback":moved}),
            )
            .is_err()
            {
                break;
            }
            told = id;
        }
        live.display.store(told.unwrap_or(0), Ordering::Relaxed);
        if let Some(h) = &media.audio {
            let now = h.state();
            if told_audio.as_ref() != Some(&now) {
                if send(&mut ws, audio_json(&now)).is_err() {
                    break;
                }
                told_audio = Some(now);
            }
        }
        if last_stats.elapsed() >= Duration::from_secs(1) {
            last_stats = Instant::now();
            let f = fps.load(Ordering::Relaxed);
            live.fps.store(f, Ordering::Relaxed);
            if send(&mut ws, serde_json::json!({"t":"stats","fps":f})).is_err() {
                break;
            }
        }
    }
    stop.store(true, Ordering::Relaxed);
    media.pipeline.unsubscribe(sub);
    if let Some((h, aid, t)) = audio_sub {
        h.unsubscribe(aid);
        if let Some(t) = t {
            let _ = t.join();
        }
    }
    if let Ok(t) = session {
        let _ = t.join();
    }
    if let Ok(t) = forwarder {
        let _ = t.join();
    }
    let end = lock(&ended).take().unwrap_or(End::Stopped);
    let reason = if forgotten || lock(&st.devices).name_of(&id).is_none() {
        "forgotten".to_string()
    } else if st.paused.load(Ordering::Relaxed) {
        "paused".to_string()
    } else if live.stop.load(Ordering::Relaxed) {
        "ended".to_string()
    } else if end == End::PipelineGone {
        match media.pipeline.last_error() {
            Some(e) if e.contains("Screen Recording") => "permission".to_string(),
            _ => "capture".to_string(),
        }
    } else {
        match end {
            End::Stopped => "left",
            End::Disconnected => "network",
            End::NeverConnected => "never_connected",
            End::PipelineGone => "capture",
            End::Failed(_) => "failed",
        }
        .to_string()
    };
    let secs = started.elapsed().as_secs();
    st.record(
        Kind::Ended,
        &name,
        &addr,
        &format!("after {}m {}s ({reason})", secs / 60, secs % 60),
    );
    let _ = send(&mut ws, serde_json::json!({"t":"end","reason":reason}));
    let _ = ws.close(None);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn five_bad_proofs_block_an_address_but_never_loopback() {
        let s = Strikes::default();
        let lan: IpAddr = "192.168.0.50".parse().unwrap();
        let lo: IpAddr = "127.0.0.1".parse().unwrap();
        for _ in 0..4 {
            s.strike(lan);
            s.strike(lo);
        }
        assert!(!s.blocked(lan));
        s.strike(lan);
        s.strike(lo);
        assert!(s.blocked(lan));
        assert!(!s.blocked(lo), "the owner's window is on loopback");
        assert!(
            !s.blocked("192.168.0.51".parse().unwrap()),
            "only that address"
        );
    }
}
