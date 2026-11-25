//! The tray and the owner's window on macOS and Windows (tray-icon, tao, wry).
//!
//! The window is the dashboard in owner mode: the same page viewers load, from
//! loopback, given this run's owner token by an initialization script. It is made
//! when opened and destroyed when closed, so the resident app is the tray alone.

use crate::model::{self, Icon, Model};
use crate::{icons, store::Store};
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};
use tao::dpi::LogicalSize;
use tao::event::{Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoopBuilder, EventLoopWindowTarget};
use tao::window::{Window, WindowBuilder};
use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{TrayIcon, TrayIconBuilder};
use wilive_server::state::lock;
use wilive_server::Server;
use wry::{WebView, WebViewBuilder};

fn tray_icon(kind: Icon) -> (tray_icon::Icon, bool) {
    let (px, w, h, template) = icons::rgba(kind);
    (
        tray_icon::Icon::from_rgba(px, w, h).expect("bundled icon"),
        template,
    )
}

struct Built {
    menu: Menu,
    open: MenuItem,
    pause: CheckMenuItem,
    quit: MenuItem,
}

fn build(m: &Model) -> Built {
    let menu = Menu::new();
    let _ = menu.append(&MenuItem::new("Wi-Live", false, None));
    let _ = menu.append(&PredefinedMenuItem::separator());
    for l in &m.lines {
        let _ = menu.append(&MenuItem::new(l, false, None));
    }
    let _ = menu.append(&PredefinedMenuItem::separator());
    let open = MenuItem::new("Open Wi-Live\u{2026}", true, None);
    let _ = menu.append(&open);
    let pause = CheckMenuItem::new("Pause sharing", true, m.paused, None);
    let _ = menu.append(&pause);
    let _ = menu.append(&PredefinedMenuItem::separator());
    let _ = menu.append(&MenuItem::new(
        format!("Version {}", env!("CARGO_PKG_VERSION")),
        false,
        None,
    ));
    let quit = MenuItem::new("Quit Wi-Live", true, None);
    let _ = menu.append(&quit);
    Built {
        menu,
        open,
        pause,
        quit,
    }
}

struct Panel {
    window: Window,
    _view: WebView,
}

fn open_external(url: &str) {
    // Only addresses the page itself shows: http(s) on this network.
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return;
    }
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(url).spawn();
    #[cfg(windows)]
    let _ = std::process::Command::new("rundll32")
        .args(["url.dll,FileProtocolHandler", url])
        .spawn();
}

/// The app icon for the window's title bar and the taskbar.
fn window_icon() -> Option<tao::window::Icon> {
    let dec = png::Decoder::new(std::io::Cursor::new(include_bytes!(
        "../assets/app-256.png"
    )));
    let mut r = dec.read_info().ok()?;
    let mut buf = vec![0; r.output_buffer_size()?];
    let info = r.next_frame(&mut buf).ok()?;
    buf.truncate(info.buffer_size());
    tao::window::Icon::from_rgba(buf, info.width, info.height).ok()
}

fn make_panel(target: &EventLoopWindowTarget<()>, port: u16, token: &str) -> Result<Panel, String> {
    let window = WindowBuilder::new()
        .with_title("Wi-Live")
        .with_inner_size(LogicalSize::new(980.0, 760.0))
        .with_min_inner_size(LogicalSize::new(380.0, 480.0))
        .with_window_icon(window_icon())
        .build(target)
        .map_err(|e| e.to_string())?;
    let origin = format!("http://127.0.0.1:{port}/");
    let allowed = origin.clone();
    let view = WebViewBuilder::new()
        .with_url(&origin)
        // The owner token, to this page only; never in the URL, never on disk.
        .with_initialization_script(format!(
            "if (location.origin === 'http://127.0.0.1:{port}') window.WILIVE_OWNER = {};",
            serde_json::Value::String(token.to_string())
        ))
        .with_navigation_handler(move |u| u.starts_with(&allowed))
        .with_new_window_req_handler(|u, _| {
            open_external(&u);
            wry::NewWindowResponse::Deny
        })
        .build(&window)
        .map_err(|e| e.to_string())?;
    window.set_focus();
    Ok(Panel {
        window,
        _view: view,
    })
}

/// The tray's input, from the server's state now.
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
        capture_allowed: crate::capture_allowed(),
        url: url.as_deref(),
    })
}

pub fn run(server: Server, token: String, store: Store, first_run: bool) {
    let mut event_loop = EventLoopBuilder::<()>::with_user_event().build();
    #[cfg(target_os = "macos")]
    {
        use tao::platform::macos::{ActivationPolicy, EventLoopExtMacOS};
        // A menu-bar app: no Dock icon.
        event_loop.set_activation_policy(ActivationPolicy::Accessory);
    }
    let _ = &mut event_loop;
    let proxy = event_loop.create_proxy();
    let (menu_tx, menu_rx) = std::sync::mpsc::channel::<MenuEvent>();
    {
        let wake = std::sync::Mutex::new(proxy);
        let menu_tx = std::sync::Mutex::new(menu_tx);
        MenuEvent::set_event_handler(Some(move |e: MenuEvent| {
            let _ = menu_tx.lock().map(|t| t.send(e));
            let _ = wake.lock().map(|p| p.send_event(()));
        }));
    }
    let mut tray: Option<TrayIcon> = None;
    let mut built: Option<Built> = None;
    let mut shown: Option<Model> = None;
    let mut panel: Option<Panel> = None;
    let mut open_now = first_run;
    let show_file = store.dir().join("show.request");
    let port = server.port;

    event_loop.run(move |event, target, flow| {
        // A short tick keeps the tray true (who watches, who asks) at no cost.
        *flow = ControlFlow::WaitUntil(Instant::now() + Duration::from_millis(500));
        if let Event::WindowEvent {
            event: WindowEvent::CloseRequested,
            window_id,
            ..
        } = &event
        {
            if panel.as_ref().is_some_and(|p| p.window.id() == *window_id) {
                panel = None; // destroyed: the web view's memory goes with it
            }
        }
        let m = current(&server);
        if tray.is_none() {
            let b = build(&m);
            let (icon, template) = tray_icon(m.icon);
            tray = TrayIconBuilder::new()
                .with_menu(Box::new(b.menu.clone()))
                .with_icon(icon)
                .with_icon_as_template(template)
                .with_tooltip(&m.tooltip)
                .build()
                .map_err(|e| log::error!("tray: {e}"))
                .ok();
            built = Some(b);
        } else if shown.as_ref() != Some(&m) {
            let b = build(&m);
            if let Some(t) = &tray {
                t.set_menu(Some(Box::new(b.menu.clone())));
                if shown.as_ref().map(|s| s.icon) != Some(m.icon) {
                    let (icon, template) = tray_icon(m.icon);
                    let _ = t.set_icon_with_as_template(Some(icon), template);
                }
                let _ = t.set_tooltip(Some(&m.tooltip));
            }
            built = Some(b);
        }
        // A new request: the owner must see its code, so the window comes forward.
        if m.asking > shown.as_ref().map_or(0, |s| s.asking) {
            open_now = true;
        }
        shown = Some(m);
        if show_file.exists() {
            let _ = std::fs::remove_file(&show_file);
            open_now = true;
        }
        while let Ok(e) = menu_rx.try_recv() {
            let Some(b) = &built else { continue };
            if e.id == b.open.id() {
                open_now = true;
            } else if e.id == b.pause.id() {
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
            } else if e.id == b.quit.id() {
                *flow = ControlFlow::Exit;
                return;
            }
        }
        if open_now {
            open_now = false;
            match &panel {
                Some(p) => {
                    p.window.set_minimized(false);
                    p.window.set_visible(true);
                    p.window.set_focus();
                }
                None => match make_panel(target, port, &token) {
                    Ok(p) => panel = Some(p),
                    Err(e) => log::error!("window: {e}"),
                },
            }
        }
    });
}
