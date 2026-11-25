//! Everything the server shares between connections: devices, pairing requests,
//! the access log, who is watching now, and the owner's switches.

use std::net::IpAddr;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Instant;
use wilive_core::devices::Devices;
use wilive_core::log::{AccessLog, Event, Kind};
use wilive_core::pending::Pending;

/// Where devices and the log are kept between runs. The app writes owner-only
/// files; tests keep them in memory.
pub trait Persist: Send + Sync {
    fn save_devices(&self, json: &str);
    fn save_log(&self, json: &str);
}

/// Keeps nothing (tests, and `serve` runs that must leave no trace).
pub struct NoPersist;
impl Persist for NoPersist {
    fn save_devices(&self, _: &str) {}
    fn save_log(&self, _: &str) {}
}

pub fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

pub fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// One viewer watching now, as the owner sees it, with the switch that ends it.
pub struct Live {
    pub id: String,
    pub device: String,
    pub name: String,
    pub addr: IpAddr,
    pub agent: String,
    pub since: u64,
    pub stop: AtomicBool,
    pub display: AtomicU32,
    pub fps: AtomicU32,
    /// Latest capture-to-present figures the viewer reported (ms; 0 = unknown).
    pub lat_p50: AtomicU32,
    pub lat_p99: AtomicU32,
    pub sound: AtomicBool,
    pub width: AtomicU32,
    pub height: AtomicU32,
}

impl Live {
    pub fn view(&self) -> serde_json::Value {
        let r = Ordering::Relaxed;
        serde_json::json!({
            "id": self.id, "device": self.device, "name": self.name, "addr": self.addr.to_string(),
            "agent": self.agent, "since": self.since, "display": self.display.load(r),
            "fps": self.fps.load(r), "lat_p50": self.lat_p50.load(r), "lat_p99": self.lat_p99.load(r),
            "sound": self.sound.load(r), "width": self.width.load(r), "height": self.height.load(r),
        })
    }
}

pub struct State {
    /// What viewers see as this computer's name.
    pub name: String,
    pub owner_token: String,
    pub devices: Mutex<Devices>,
    pub pending: Mutex<Pending>,
    pub log: Mutex<AccessLog>,
    /// Sharing paused by the owner: nobody new is let in, and everyone watching
    /// is ended.
    pub paused: AtomicBool,
    pub live: Mutex<Vec<Arc<Live>>>,
    persist: Box<dyn Persist>,
    started: Instant,
    seq: AtomicU64,
}

impl State {
    pub fn new(name: &str, owner_token: String, devices: Devices, log: AccessLog, persist: Box<dyn Persist>) -> Self {
        State {
            name: wilive_core::pending::clean_name(name),
            owner_token,
            devices: Mutex::new(devices),
            pending: Mutex::default(),
            log: Mutex::new(log),
            paused: AtomicBool::new(false),
            live: Mutex::default(),
            persist,
            started: Instant::now(),
            seq: AtomicU64::new(1),
        }
    }

    /// Milliseconds since start: the pairing table's clock.
    pub fn ms(&self) -> u64 {
        self.started.elapsed().as_millis() as u64
    }

    pub fn next_id(&self) -> u64 {
        self.seq.fetch_add(1, Ordering::Relaxed)
    }

    pub fn record(&self, kind: Kind, who: &str, addr: &str, detail: &str) {
        let json = {
            let mut l = lock(&self.log);
            l.push(Event { at: unix_now(), kind, who: who.into(), addr: addr.into(), detail: detail.into() });
            l.to_json()
        };
        self.persist.save_log(&json);
    }

    pub fn save_devices(&self) {
        let json = lock(&self.devices).to_json();
        self.persist.save_devices(&json);
    }

    pub fn watching(&self) -> usize {
        lock(&self.live).len()
    }

    /// Ends every session of one device (forgotten) or all (paused).
    pub fn end_where(&self, f: impl Fn(&Live) -> bool) -> usize {
        let live = lock(&self.live);
        let mut n = 0;
        for s in live.iter().filter(|s| f(s)) {
            s.stop.store(true, Ordering::Relaxed);
            n += 1;
        }
        n
    }
}
