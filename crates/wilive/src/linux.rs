//! The tray on Linux: the freedesktop StatusNotifierItem over D-Bus (ksni), as HID-Fi
//! Companion does. Linux has no web view to borrow, so "Open Wi-Live" opens the
//! owner's window in the default browser through a one-time link that becomes an
//! owner session there (wilive-server: /owner?code=..., loopback only, used once).
//! A desktop without a tray host still gets a working app: the address is printed,
//! and a device asking to watch opens the window by itself.

use crate::model::{self, Icon, Model};
use crate::{icons, store::Store};
use ksni::blocking::TrayMethods;
use std::sync::atomic::Ordering;
use std::sync::mpsc::{channel, Sender};
use std::time::Duration;
use wilive_server::state::lock;
use wilive_server::Server;

enum Ask {
    Open,
    Pause,
    Quit,
}

struct Tray {
    model: Model,
    tx: Sender<Ask>,
}

/// ksni wants ARGB32 in network byte order.
fn pixmap(kind: Icon) -> ksni::Icon {
    let (rgba, w, h, _) = icons::rgba(kind);
    let mut data = Vec::with_capacity(rgba.len());
    for [r, g, b, a] in rgba.as_chunks::<4>().0 {
        data.extend_from_slice(&[*a, *r, *g, *b]);
    }
    ksni::Icon {
        width: w as i32,
        height: h as i32,
        data,
    }
}

impl ksni::Tray for Tray {
    fn id(&self) -> String {
        "wilive".into()
    }
    fn title(&self) -> String {
        "Wi-Live".into()
    }
    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        vec![pixmap(self.model.icon)]
    }
    fn status(&self) -> ksni::Status {
        if self.model.asking > 0 {
            ksni::Status::NeedsAttention
        } else {
            ksni::Status::Active
        }
    }
    fn tool_tip(&self) -> ksni::ToolTip {
        ksni::ToolTip {
            title: self.model.tooltip.clone(),
            description: self.model.lines.join("\n"),
            ..Default::default()
        }
    }
    fn activate(&mut self, _x: i32, _y: i32) {
        let _ = self.tx.send(Ask::Open);
    }
    fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
        use ksni::menu::*;
        let mut v: Vec<ksni::MenuItem<Self>> = Vec::new();
        for l in &self.model.lines {
            v.push(
                StandardItem {
                    label: l.clone(),
                    enabled: false,
                    ..Default::default()
                }
                .into(),
            );
        }
        v.push(MenuItem::Separator);
        v.push(
            StandardItem {
                label: "Open Wi-Live\u{2026}".into(),
                activate: Box::new(|t: &mut Self| {
                    let _ = t.tx.send(Ask::Open);
                }),
                ..Default::default()
            }
            .into(),
        );
        v.push(
            CheckmarkItem {
                label: "Pause sharing".into(),
                checked: self.model.paused,
                activate: Box::new(|t: &mut Self| {
                    let _ = t.tx.send(Ask::Pause);
                }),
                ..Default::default()
            }
            .into(),
        );
        v.push(MenuItem::Separator);
        v.push(
            StandardItem {
                label: format!("Version {}", env!("CARGO_PKG_VERSION")),
                enabled: false,
                ..Default::default()
            }
            .into(),
        );
        v.push(
            StandardItem {
                label: "Quit Wi-Live".into(),
                activate: Box::new(|t: &mut Self| {
                    let _ = t.tx.send(Ask::Quit);
                }),
                ..Default::default()
            }
            .into(),
        );
        v
    }
}

fn current(server: &Server) -> Model {
    let st = &server.state;
    let watching: Vec<String> = lock(&st.live).iter().map(|l| l.name.clone()).collect();
    let asking: Vec<String> = lock(&st.pending)
        .views(st.ms())
        .into_iter()
        .map(|v| v.name)
        .collect();
    let url = wilive_server::lan_addrs()
        .first()
        .map(|ip| format!("http://{ip}:{}/", server.port));
    model::model(&model::Input {
        watching: &watching,
        asking: &asking,
        paused: st.paused.load(Ordering::Relaxed),
        capture_allowed: true,
        url: url.as_deref(),
    })
}

/// The owner's window in the default browser, through a fresh one-time link.
fn open_window(server: &Server) {
    let url = format!(
        "http://127.0.0.1:{}{}",
        server.port,
        server.state.owner_link()
    );
    if let Err(e) = std::process::Command::new("xdg-open").arg(&url).spawn() {
        log::warn!("could not open the browser: {e}");
        println!("Wi-Live: open the owner's window at {url} (one use, within a minute)");
    }
}

pub fn run(server: Server, store: Store, first_run: bool) {
    let (tx, rx) = channel::<Ask>();
    let mut shown = current(&server);
    let tray = match (Tray {
        model: shown.clone(),
        tx: tx.clone(),
    })
    .spawn()
    {
        Ok(t) => Some(t),
        Err(e) => {
            log::warn!("no tray on this desktop: {e}");
            for u in wilive_server::lan_addrs() {
                println!("Wi-Live: open http://{u}:{}/ on a device", server.port);
            }
            None
        }
    };
    if first_run {
        open_window(&server);
    }
    let show_file = store.dir().join("show.request");
    loop {
        match rx.recv_timeout(Duration::from_millis(500)) {
            Ok(Ask::Open) => open_window(&server),
            Ok(Ask::Pause) => {
                let on = !server.state.paused.load(Ordering::Relaxed);
                server.state.paused.store(on, Ordering::Relaxed);
                if on {
                    server.state.end_where(|_| true);
                }
                server.state.record(
                    if on {
                        wilive_core::log::Kind::Paused
                    } else {
                        wilive_core::log::Kind::Resumed
                    },
                    "",
                    "",
                    "from the menu",
                );
            }
            Ok(Ask::Quit) => std::process::exit(0),
            Err(_) => {}
        }
        let m = current(&server);
        // A new request: the owner must see its code, so the window opens.
        if m.asking > shown.asking || show_file.exists() {
            let _ = std::fs::remove_file(&show_file);
            open_window(&server);
        }
        if m != shown {
            if let Some(t) = &tray {
                let m2 = m.clone();
                t.update(move |x: &mut Tray| x.model = m2);
            }
            shown = m;
        }
    }
}
