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
//! stdin: "still" / "move" (freeze the pattern), "unplug <id>" (take a display away)

use std::sync::Arc;
use wilive_server::signal::Media;
use wilive_server::state::{NoPersist, State};
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
            if real {
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
    let state = Arc::new(State::new(
        &name,
        token.clone(),
        Default::default(),
        Default::default(),
        Box::new(NoPersist),
    ));
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
    println!(
        "{}",
        serde_json::json!({"port": srv.port, "owner": token, "fp": fp})
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
