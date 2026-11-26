//! Wi-Live: this computer's screen, sound and microphone, live on any device in the
//! home network (docs/DESIGN.md). This file sets up the folder, the single
//! instance, the media and the server; `desktop` is the tray and the window.

#![cfg_attr(windows, windows_subsystem = "windows")]

mod autostart;
#[cfg(any(target_os = "macos", windows))]
mod desktop;
mod icons;
#[cfg(target_os = "linux")]
mod linux;
mod logfile;
mod model;
mod store;
mod updater;

use std::sync::{Arc, Mutex};
use store::{FilePersist, Settings, Store};
use wilive_server::signal::Media;
use wilive_server::state::{Host, State};
use wilive_stream::pipeline::{Pipeline, Source};

pub const FPS: u32 = 60;

/// The app's switches and permissions, for the owner's window.
struct AppHost {
    updater: Arc<updater::Updater>,
    store: Store,
    settings: Mutex<Settings>,
    audio: Arc<wilive_stream::audio::Settings>,
    no_autostart: bool,
}

pub fn capture_allowed() -> bool {
    #[cfg(target_os = "macos")]
    {
        wilive_stream::sck::allowed()
    }
    #[cfg(not(target_os = "macos"))]
    {
        true
    }
}

impl Host for AppHost {
    fn capture_allowed(&self) -> bool {
        capture_allowed()
    }
    fn ask_capture(&self) {
        #[cfg(target_os = "macos")]
        {
            let _ = wilive_stream::sck::ask();
        }
    }
    fn update(&self) -> serde_json::Value {
        self.updater.view()
    }
    fn update_action(&self, act: &str) -> bool {
        match act {
            "check" => {
                let u = self.updater.clone();
                std::thread::spawn(move || u.check());
                true
            }
            "install" => {
                self.updater.install();
                true
            }
            _ => false,
        }
    }
    fn switches(&self) -> Vec<(&'static str, bool)> {
        let s = self.settings.lock().unwrap_or_else(|e| e.into_inner());
        vec![
            ("sound", s.sound),
            ("mic", s.mic),
            ("start_at_login", s.start_at_login),
        ]
    }
    fn set_switch(&self, name: &str, on: bool) -> bool {
        use std::sync::atomic::Ordering::Relaxed;
        let mut s = self.settings.lock().unwrap_or_else(|e| e.into_inner());
        match name {
            "sound" => {
                s.sound = on;
                self.audio.system.store(on, Relaxed);
            }
            "mic" => {
                s.mic = on;
                self.audio.mic.store(on, Relaxed);
            }
            "start_at_login" => {
                s.start_at_login = on;
                if !self.no_autostart {
                    if let Err(e) = autostart::set(on) {
                        log::warn!("start at login: {e}");
                    }
                }
            }
            _ => return false,
        }
        self.store.save_settings(&s);
        true
    }
}

fn main() {
    if std::env::args().any(|a| a == "--version" || a == "-V") {
        println!("Wi-Live {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    // WILIVE_HOME and WILIVE_NO_AUTOSTART are for development and tests: a test
    // run must not install a login item or touch the real app folder.
    let dir = std::env::var_os("WILIVE_HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(Store::default_dir);
    let no_autostart = std::env::var_os("WILIVE_NO_AUTOSTART").is_some();
    let store = match Store::open(dir) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Wi-Live cannot use its folder: {e}");
            std::process::exit(1);
        }
    };
    logfile::FileLog::install(store.dir(), log::LevelFilter::Info);

    // One instance: a second launch asks the first to show its window, then leaves.
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(store.dir().join("instance.lock"));
    // Started by an update: the old copy is on its way out; wait for its lock.
    let after_update = std::env::args().any(|a| a == "--after-update");
    let _lock = match lock {
        Ok(f) if f.try_lock().is_ok() => f,
        Ok(f) if after_update => {
            let end = std::time::Instant::now() + std::time::Duration::from_secs(20);
            loop {
                if f.try_lock().is_ok() {
                    break f;
                }
                if std::time::Instant::now() > end {
                    log::warn!("the copy before the update did not leave within 20 s");
                    return;
                }
                std::thread::sleep(std::time::Duration::from_millis(200));
            }
        }
        _ => {
            let _ = std::fs::write(store.dir().join("show.request"), b"1");
            log::info!("already running - asked it to show its window");
            return;
        }
    };
    log::info!(
        "starting, version {}{}",
        env!("CARGO_PKG_VERSION"),
        if after_update { " (just updated)" } else { "" }
    );
    updater::clean_old();
    let updater = updater::Updater::new();
    updater.watch();
    std::panic::set_hook(Box::new(|p| log::error!("panic: {p}")));

    let settings = store.settings();
    if settings.start_at_login && !no_autostart {
        if let Err(e) = autostart::set(true) {
            log::warn!("start at login: {e}");
        }
    }
    let Some(cert) = store.cert() else {
        log::error!("no WebRTC certificate could be made");
        std::process::exit(1);
    };
    let fp = wilive_stream::rtc::cert_fp(&cert);
    let pipeline = Arc::new(Pipeline::start(
        FPS,
        6_000_000,
        |d: Option<u32>| -> Result<Box<dyn Source>, String> { wilive_stream::screen(d, FPS) },
        wilive_stream::encoder,
    ));
    let audio_settings = wilive_stream::audio::Settings::new(settings.sound, settings.mic);
    let audio = Arc::new(wilive_stream::audio::AudioHub::start(
        Arc::new(wilive_stream::audio_capture),
        audio_settings.clone(),
    ));
    let name = computer_name();
    // A fresh owner token each run, held in memory only: the window gets it, and
    // nothing on disk can be stolen to drive the owner's controls.
    #[allow(unused_mut)]
    let mut token = wilive_core::pair::random_hex(24);
    // Test builds only: the e2e update test drives the owner's controls.
    #[cfg(feature = "test-key")]
    if let Ok(t) = std::env::var("WILIVE_TEST_OWNER") {
        token = t;
    }
    let host = AppHost {
        updater,
        store: Store::open(store.dir().to_path_buf()).expect("same folder"),
        settings: Mutex::new(settings.clone()),
        audio: audio_settings,
        no_autostart,
    };
    let state = Arc::new(
        State::new(
            &name,
            token.clone(),
            store.devices(),
            store.access_log(),
            Box::new(FilePersist {
                dir: store.dir().to_path_buf(),
            }),
        )
        .with_host(Box::new(host)),
    );
    let media = Media {
        pipeline,
        audio: Some(audio),
        cert,
        fp,
        displays: Box::new(wilive_stream::display::list),
    };
    let server = match settings
        .port
        .and_then(|p| wilive_server::start(Some(p), state.clone(), clone_media(&media)).ok())
    {
        Some(s) => s,
        None => match wilive_server::start(None, state.clone(), media) {
            Ok(s) => s,
            Err(e) => {
                log::error!("cannot listen: {e}");
                std::process::exit(1);
            }
        },
    };
    if settings.port != Some(server.port) {
        let mut s = settings.clone();
        s.port = Some(server.port);
        store.save_settings(&s);
    }
    log::info!("listening on port {}", server.port);
    let first_run = state
        .devices
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .is_empty();

    #[cfg(any(target_os = "macos", windows))]
    desktop::run(server, token, store, first_run);
    #[cfg(target_os = "linux")]
    linux::run(server, store, first_run);
    #[cfg(not(any(target_os = "macos", windows, target_os = "linux")))]
    {
        let _ = (token, first_run);
        loop {
            std::thread::park();
        }
    }
}

/// `Media` holds a pipeline and an audio hub that must stay shared; the first try
/// on the remembered port borrows them.
fn clone_media(m: &Media) -> Media {
    Media {
        pipeline: m.pipeline.clone(),
        audio: m.audio.clone(),
        cert: m.cert.clone(),
        fp: m.fp.clone(),
        displays: Box::new(wilive_stream::display::list),
    }
}

/// What viewers see this computer called.
fn computer_name() -> String {
    #[cfg(target_os = "macos")]
    if let Ok(o) = std::process::Command::new("scutil")
        .args(["--get", "ComputerName"])
        .output()
    {
        let n = String::from_utf8_lossy(&o.stdout).trim().to_string();
        if !n.is_empty() {
            return n;
        }
    }
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "This computer".into())
}
