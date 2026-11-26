//! The whole server with a test picture, for end-to-end checks in a real browser
//! (tests/e2e, tests/latency) and for trying the dashboard by hand. Prints one JSON
//! line: the port and the owner token. Keeps nothing on disk.
//!
//!   cargo run -p wilive-server --example serve -- [WIDTH HEIGHT FPS [still|busy]]
//!
//! WILIVE_CLOCK=1      draw the time strip (the latency probe reads it)
//! WILIVE_DISPLAYS=n   n pretend displays (main first)
//! WILIVE_REAL_SCREEN=1 this computer's real screen instead of the pattern
//! WILIVE_SOUND=test|real  a test tone (clicks each second) or the real sound
//! WILIVE_PORT=p       exactly this port
//! WILIVE_DEMO_IMAGE=a.png  a still picture as the screen (README screenshots; never a real screen)
//! WILIVE_DEMO_FRAMES=dirA,dirB  a made-up screen per display, played from f00000.png... in a loop (films);
//!                    an entry "auto|sharp" names the same screen rendered at two sizes: Sharp
//!                    opens the second, as the real capture opens a Retina screen at full size
//! WILIVE_HOME=dir     keep devices and the log in dir across restarts (as the app does)
//! WILIVE_FILL_LOG=n   start with n made-up log events; WILIVE_FILL_DEVICES=n with n devices
//! stdin: "still" / "move" (freeze the pattern), "unplug <id>" (take a display away)

use std::sync::Arc;
use wilive_server::signal::Media;
use wilive_server::state::{Host, NoPersist, State};

/// A still picture as the screen (WILIVE_DEMO_IMAGE): the README's screenshots show
/// a made-up desktop, never anyone's real one.
struct Still {
    w: u32,
    h: u32,
    bgra: Vec<u8>,
}
/// A PNG as the encoder's BGRA, its size rounded down to even.
fn decode(path: &std::path::Path) -> (u32, u32, Vec<u8>) {
    let dec = png::Decoder::new(std::io::BufReader::new(
        std::fs::File::open(path).unwrap_or_else(|e| panic!("{}: {e}", path.display())),
    ));
    let mut r = dec.read_info().expect("a PNG");
    let mut buf = vec![0; r.output_buffer_size().expect("size")];
    let info = r.next_frame(&mut buf).expect("a PNG frame");
    let (w, h) = (info.width & !1, info.height & !1);
    let ch = info.color_type.samples();
    let mut bgra = vec![255u8; (w * h * 4) as usize];
    for y in 0..h as usize {
        for x in 0..w as usize {
            let s = (y * info.width as usize + x) * ch;
            let d = (y * w as usize + x) * 4;
            bgra[d] = buf[s + 2];
            bgra[d + 1] = buf[s + 1];
            bgra[d + 2] = buf[s];
        }
    }
    (w, h, bgra)
}
impl Still {
    fn open(path: &str) -> Still {
        let (w, h, bgra) = decode(std::path::Path::new(path));
        Still { w, h, bgra }
    }
}

/// A made-up screen that moves (WILIVE_DEMO_FRAMES): numbered PNGs played one per
/// grab, in a loop. Four threads decode ahead, in order, a few frames at most, so a
/// 4K sequence keeps up at 30 fps and costs no more memory than a handful of frames.
struct Frames {
    rx: Vec<std::sync::mpsc::Receiver<(u32, u32, Vec<u8>)>>,
    next: usize,
    w: u32,
    h: u32,
    bgra: Vec<u8>,
}
const DECODERS: usize = 4;
impl Frames {
    fn open(dir: &str) -> Frames {
        let mut files: Vec<_> = std::fs::read_dir(dir)
            .unwrap_or_else(|e| panic!("WILIVE_DEMO_FRAMES {dir}: {e}"))
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|x| x == "png"))
            .collect();
        files.sort();
        assert!(!files.is_empty(), "no PNG frames in {dir}");
        let (w, h, bgra) = decode(&files[0]);
        let files = Arc::new(files);
        // One clock for the whole run: a screen opened again (a viewer back after a pause,
        // another quality) carries on where it was, as a real screen would, never from
        // its first frame.
        static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
        let now = (START
            .get_or_init(std::time::Instant::now)
            .elapsed()
            .as_secs_f64()
            * 30.0) as usize;
        let rx = (0..DECODERS)
            .map(|k| {
                let (tx, rx) = std::sync::mpsc::sync_channel(2);
                let files = files.clone();
                std::thread::spawn(move || {
                    // Worker k decodes frames now+k, now+k+4 ... until the source is dropped.
                    let mut i = (now + k) % files.len();
                    while tx.send(decode(&files[i])).is_ok() {
                        i = (i + DECODERS) % files.len();
                    }
                });
                rx
            })
            .collect();
        Frames {
            rx,
            next: 0,
            w,
            h,
            bgra,
        }
    }
}
impl Source for Frames {
    fn size(&self) -> (u32, u32) {
        (self.w, self.h)
    }
    fn grab(
        &mut self,
        _force: bool,
        f: &mut dyn FnMut(&wilive_stream::frame::Frame),
    ) -> Result<bool, String> {
        // The next frame in order; if its decoder is behind, the last one again.
        if let Ok((w, h, bgra)) = self.rx[self.next].try_recv() {
            if (w, h) == (self.w, self.h) {
                self.bgra = bgra;
            }
            self.next = (self.next + 1) % DECODERS;
        }
        f(&wilive_stream::frame::Frame {
            width: self.w,
            height: self.h,
            stride: (self.w * 4) as usize,
            bgra: &self.bgra,
        });
        Ok(true)
    }
}

impl Source for Still {
    fn size(&self) -> (u32, u32) {
        (self.w, self.h)
    }
    fn grab(
        &mut self,
        _force: bool,
        f: &mut dyn FnMut(&wilive_stream::frame::Frame),
    ) -> Result<bool, String> {
        // Every frame, as a real screen would: the window's fps and picture size read as live.
        f(&wilive_stream::frame::Frame {
            width: self.w,
            height: self.h,
            stride: (self.w * 4) as usize,
            bgra: &self.bgra,
        });
        Ok(true)
    }
}

/// Devices and the log as files in WILIVE_HOME (the app's own store does the same).
struct DirPersist(std::path::PathBuf);
impl wilive_server::state::Persist for DirPersist {
    fn save_devices(&self, json: &str) {
        let _ = std::fs::write(self.0.join("devices.json"), json);
    }
    fn save_log(&self, json: &str) {
        let _ = std::fs::write(self.0.join("access.json"), json);
    }
}

/// WILIVE_TEST_HOST=1: switches and a screen-recording permission that starts
/// refused and is granted when asked, so the owner window's controls can be tested.
#[derive(Default)]
struct TestHost {
    allowed: std::sync::atomic::AtomicBool,
    sw: std::sync::Mutex<Vec<(&'static str, bool)>>,
}
impl Host for TestHost {
    fn capture_allowed(&self) -> bool {
        self.allowed.load(std::sync::atomic::Ordering::Relaxed)
    }
    fn ask_capture(&self) {
        self.allowed
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }
    fn switches(&self) -> Vec<(&'static str, bool)> {
        self.sw.lock().unwrap().clone()
    }
    fn set_switch(&self, name: &str, on: bool) -> bool {
        let mut sw = self.sw.lock().unwrap();
        match sw.iter_mut().find(|(k, _)| *k == name) {
            Some(x) => {
                x.1 = on;
                true
            }
            None => false,
        }
    }
}
use wilive_stream::pattern::{FakeDisplays, Pattern};
use wilive_stream::pipeline::{Pipeline, Source};

struct Stderr;
impl log::Log for Stderr {
    fn enabled(&self, m: &log::Metadata) -> bool {
        m.level() <= log::Level::Info
    }
    fn log(&self, r: &log::Record) {
        if self.enabled(r.metadata()) {
            eprintln!("{} {}", r.level(), r.args());
        }
    }
    fn flush(&self) {}
}

fn main() {
    let _ = log::set_logger(&Stderr).map(|()| log::set_max_level(log::LevelFilter::Info));
    let a: Vec<String> = std::env::args().collect();
    let w: u32 = a.get(1).and_then(|s| s.parse().ok()).unwrap_or(1280);
    let h: u32 = a.get(2).and_then(|s| s.parse().ok()).unwrap_or(720);
    let fps: u32 = a.get(3).and_then(|s| s.parse().ok()).unwrap_or(60);
    let still = a.get(4).is_some_and(|s| s == "still");
    let busy = a.get(4).is_some_and(|s| s == "busy");
    let clock = std::env::var("WILIVE_CLOCK").is_ok_and(|v| v == "1");
    let real = std::env::var_os("WILIVE_REAL_SCREEN").is_some();
    let fake = FakeDisplays::from_env(w, h);
    let f2 = fake.clone();
    let pipeline = Arc::new(Pipeline::start(
        fps,
        6_000_000,
        move |d: Option<u32>| -> Result<Box<dyn Source>, String> {
            if let Ok(why) = std::env::var("WILIVE_FAIL_CAPTURE") {
                return Err(why);
            }
            if let Ok(dirs) = std::env::var("WILIVE_DEMO_FRAMES") {
                let dirs: Vec<&str> = dirs.split(',').collect();
                let i = d.unwrap_or(1).saturating_sub(1) as usize;
                let entry = dirs[i.min(dirs.len() - 1)];
                let (auto, sharp) = entry.split_once('|').unwrap_or((entry, entry));
                let sharp_on = wilive_stream::edge::get() >= wilive_stream::edge::SHARP;
                Ok(Box::new(Frames::open(if sharp_on { sharp } else { auto })))
            } else if let Ok(path) = std::env::var("WILIVE_DEMO_IMAGE") {
                Ok(Box::new(Still::open(&path)))
            } else if real {
                wilive_stream::screen(d, fps)
            } else if still {
                Ok(Box::new(Pattern::new(w, h).still()))
            } else {
                f2.source_with(d, busy, clock)
            }
        },
        wilive_stream::encoder,
    ));
    let cert = wilive_stream::rtc::new_cert().expect("certificate");
    let fp = wilive_stream::rtc::cert_fp(&cert);
    let audio = std::env::var("WILIVE_SOUND").ok().map(|kind| {
        let opener: wilive_stream::audio::Opener = if kind == "real" {
            Arc::new(wilive_stream::audio_capture)
        } else {
            Arc::new(|i, sink, _ended| {
                let (rate, ch) = match i {
                    wilive_stream::audio::Input::System => (48_000, 2),
                    wilive_stream::audio::Input::Microphone => (44_100, 1),
                };
                Ok(wilive_stream::audio::test_sound(
                    wilive_stream::epoch(),
                    rate,
                    ch,
                    480,
                    sink,
                ))
            })
        };
        Arc::new(wilive_stream::audio::AudioHub::start(
            opener,
            wilive_stream::audio::Settings::new(true, false),
        ))
    });
    let token = wilive_core::pair::random_hex(24);
    let name = std::env::var("WILIVE_NAME").unwrap_or_else(|_| "Test computer".into());
    let home = std::env::var_os("WILIVE_HOME").map(std::path::PathBuf::from);
    let mut devices = home
        .as_ref()
        .and_then(|h| std::fs::read_to_string(h.join("devices.json")).ok())
        .and_then(|j| wilive_core::devices::Devices::from_json(&j).ok())
        .unwrap_or_default();
    let mut log = home
        .as_ref()
        .and_then(|h| std::fs::read_to_string(h.join("access.json")).ok())
        .map(|j| wilive_core::log::AccessLog::from_json(&j))
        .unwrap_or_default();
    let fill = |k: &str| {
        std::env::var(k)
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(0)
    };
    for i in 0..fill("WILIVE_FILL_LOG") {
        log.push(wilive_core::log::Event {
            at: 1_795_000_000 + i as u64,
            kind: wilive_core::log::Kind::Watching,
            who: format!("Device {i}"),
            addr: "192.168.0.50".into(),
            detail: String::new(),
        });
    }
    for i in 0..fill("WILIVE_FILL_DEVICES") {
        let k = wilive_core::pair::DeviceKey::from_bytes([i as u8; 32]);
        devices.add(wilive_core::devices::Device::new(
            format!("{i:032x}"),
            format!("Phone {i}"),
            "Safari on iPhone".into(),
            "192.168.0.60".into(),
            &k,
            1_795_000_000,
        ));
    }
    let persist: Box<dyn wilive_server::state::Persist> = match &home {
        Some(h) => {
            let _ = std::fs::create_dir_all(h);
            Box::new(DirPersist(h.clone()))
        }
        None => Box::new(NoPersist),
    };
    let mut state = State::new(&name, token.clone(), devices, log, persist);
    if std::env::var_os("WILIVE_TEST_HOST").is_some() {
        let h = TestHost::default();
        *h.sw.lock().unwrap() = vec![("sound", true), ("mic", false), ("start_at_login", true)];
        state = state.with_host(Box::new(h));
    }
    let state = Arc::new(state);
    let f3 = fake.clone();
    let displays: wilive_server::signal::Displays = Box::new(move || {
        if real {
            wilive_stream::display::list()
        } else {
            f3.list()
        }
    });
    let port = std::env::var("WILIVE_PORT")
        .ok()
        .and_then(|p| p.parse().ok());
    let srv = wilive_server::start(
        port,
        state,
        Media {
            pipeline: pipeline.clone(),
            audio,
            cert,
            fp: fp.clone(),
            displays,
        },
    )
    .expect("listening");
    let link = srv.state.owner_link();
    println!(
        "{}",
        serde_json::json!({"port": srv.port, "owner": token, "fp": fp, "owner_link": link})
    );
    std::thread::spawn(move || {
        for line in std::io::stdin().lines().map_while(Result::ok) {
            match line.trim() {
                "still" => {
                    wilive_stream::pattern::HELD.store(true, std::sync::atomic::Ordering::Relaxed)
                }
                "move" => {
                    wilive_stream::pattern::HELD.store(false, std::sync::atomic::Ordering::Relaxed)
                }
                l => {
                    if let Some(id) = l.strip_prefix("unplug ").and_then(|s| s.parse().ok()) {
                        fake.unplug(id);
                    }
                }
            }
        }
    });
    loop {
        std::thread::sleep(std::time::Duration::from_secs(5));
        eprintln!(
            "watching={} pipeline_viewers={}",
            srv.state.watching(),
            pipeline.viewers()
        );
    }
}
