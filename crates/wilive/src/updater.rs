//! Updating Wi-Live itself (docs/RELEASING.md). Once a day (and a minute after
//! start) it reads the latest release's SHA256SUMS and signature; wilive-core's
//! `update::decide` checks the signature against the built-in release key before
//! anything else. The owner's window shows what is new; only the owner's tap installs:
//!
//!   download this computer's file (capped), check its hash against the signed list,
//!   macOS: open the disk image, require Wi-Live's certificate on the app inside,
//!          put it in place of the running app (a sibling copy, then two renames),
//!   Windows, Linux: the new program beside the running one, then two renames,
//!   start the new copy with --after-update (it waits for this one to leave), exit.
//!
//! WILIVE_RELEASES points it at another release folder (tests); the signature rules do
//! not change. Test builds (feature test-key) also read WILIVE_TEST_VERSION.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use wilive_core::update::{self, Offer};

pub const RELEASES: &str = "https://github.com/san-gitlogin/Wi-Live/releases/latest/download/";
const SUMS_CAP: u64 = 64 * 1024;
const ASSET_CAP: u64 = 200 * 1024 * 1024;
pub const CHECK_EVERY: Duration = Duration::from_secs(24 * 3600);
/// The macOS app in a release must carry this certificate (tools/release_keys.json).
#[cfg(target_os = "macos")]
pub const MAC_CERT_SHA1: &str = "7ec828ea86b744681e2c1456c700112e77a9086c";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Phase {
    /// Not checked yet, or checking.
    Unknown,
    Checking,
    UpToDate,
    Available(Offer),
    Installing(u8),
    Failed(String),
}

pub struct Updater {
    base: String,
    current: (u32, u32, u32),
    phase: Mutex<Phase>,
    checked: Mutex<Option<Instant>>,
    agent: ureq::Agent,
}

pub fn current_version() -> (u32, u32, u32) {
    #[cfg(feature = "test-key")]
    if let Some(v) = std::env::var("WILIVE_TEST_VERSION")
        .ok()
        .and_then(|v| update::parse_version(&v))
    {
        return v;
    }
    update::parse_version(env!("CARGO_PKG_VERSION")).unwrap_or((0, 0, 0))
}

#[cfg(any(target_os = "macos", windows))]
fn tls() -> ureq::tls::TlsConfig {
    ureq::tls::TlsConfig::builder()
        .provider(ureq::tls::TlsProvider::NativeTls)
        .root_certs(ureq::tls::RootCerts::PlatformVerifier)
        .build()
}

#[cfg(target_os = "linux")]
fn tls() -> ureq::tls::TlsConfig {
    ureq::tls::TlsConfig::builder()
        .provider(ureq::tls::TlsProvider::Rustls)
        .unversioned_rustls_crypto_provider(Arc::new(rustls::crypto::aws_lc_rs::default_provider()))
        .root_certs(ureq::tls::RootCerts::WebPki)
        .build()
}

#[cfg(not(any(target_os = "macos", windows, target_os = "linux")))]
fn tls() -> ureq::tls::TlsConfig {
    ureq::tls::TlsConfig::builder().build()
}

pub fn os_arch() -> (&'static str, &'static str) {
    let os = if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(windows) {
        "windows"
    } else if cfg!(target_os = "linux") {
        "linux"
    } else {
        "other"
    };
    (os, std::env::consts::ARCH)
}

impl Updater {
    pub fn new() -> Arc<Updater> {
        let base = std::env::var("WILIVE_RELEASES").unwrap_or_else(|_| RELEASES.to_string());
        let base = if base.ends_with('/') {
            base
        } else {
            base + "/"
        };
        let agent = ureq::Agent::config_builder()
            .tls_config(tls())
            .timeout_connect(Some(Duration::from_secs(15)))
            .timeout_per_read(Some(Duration::from_secs(60)))
            .user_agent(concat!("Wi-Live/", env!("CARGO_PKG_VERSION")))
            .build()
            .into();
        Arc::new(Updater {
            base,
            current: current_version(),
            phase: Mutex::new(Phase::Unknown),
            checked: Mutex::new(None),
            agent,
        })
    }

    fn set(&self, p: Phase) {
        *self.phase.lock().unwrap_or_else(|e| e.into_inner()) = p;
    }
    pub fn phase(&self) -> Phase {
        self.phase.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    fn get(
        &self,
        name: &str,
        cap: u64,
        mut progress: impl FnMut(u64, u64),
    ) -> Result<Vec<u8>, String> {
        let mut r = self
            .agent
            .get(&format!("{}{name}", self.base))
            .call()
            .map_err(|e| e.to_string())?;
        let total = r
            .headers()
            .get("content-length")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(0);
        if total > cap {
            return Err(format!("{name} is larger than {cap} bytes"));
        }
        let mut out = Vec::new();
        let mut rd = r.body_mut().with_config().limit(cap).reader();
        let mut buf = [0u8; 64 * 1024];
        loop {
            let n = std::io::Read::read(&mut rd, &mut buf).map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }
            out.extend_from_slice(&buf[..n]);
            progress(out.len() as u64, total);
        }
        Ok(out)
    }

    /// Reads the latest release and decides; never installs.
    pub fn check(&self) {
        self.set(Phase::Checking);
        *self.checked.lock().unwrap_or_else(|e| e.into_inner()) = Some(Instant::now());
        let r = (|| {
            let sums = self.get("SHA256SUMS", SUMS_CAP, |_, _| {})?;
            let sig = self.get("SHA256SUMS.sig", 1024, |_, _| {})?;
            let (os, arch) = os_arch();
            update::decide(&sums, &sig, &update::trusted_keys(), self.current, os, arch)
                .map_err(|e| format!("{e:?}"))
        })();
        match r {
            Ok(Some(o)) => {
                log::info!(
                    "update: {}.{}.{} is available",
                    o.version.0,
                    o.version.1,
                    o.version.2
                );
                self.set(Phase::Available(o))
            }
            Ok(None) => self.set(Phase::UpToDate),
            Err(e) => {
                log::warn!("update check: {e}");
                self.set(Phase::Failed(format!("Could not check for updates ({e})")))
            }
        }
    }

    /// Downloads, checks and installs the offered release, then hands over to it.
    pub fn install(self: &Arc<Self>) {
        let Phase::Available(o) = self.phase() else {
            return;
        };
        self.set(Phase::Installing(0));
        let me = self.clone();
        let _ = std::thread::Builder::new()
            .name("update".into())
            .spawn(move || {
                let r = (|| {
                    let data = me.get(&o.asset, ASSET_CAP, |got, total| {
                        if let Some(p) = (got * 90).checked_div(total) {
                            me.set(Phase::Installing(p as u8));
                        }
                    })?;
                    update::check_download(&data, &o.sha256).map_err(|_| {
                        "the download does not match the signed release".to_string()
                    })?;
                    me.set(Phase::Installing(92));
                    let next = put_in_place(&data, &o)?;
                    me.set(Phase::Installing(100));
                    Ok::<PathBuf, String>(next)
                })();
                match r {
                    Ok(next) => {
                        log::info!(
                            "update: installed {}.{}.{}; starting it",
                            o.version.0,
                            o.version.1,
                            o.version.2
                        );
                        relaunch(&next);
                        std::process::exit(0);
                    }
                    Err(e) => {
                        log::warn!("update failed: {e}");
                        me.set(Phase::Failed(format!("The update did not install: {e}")));
                    }
                }
            });
    }

    /// Checks on its own: a minute after start, then once a day.
    pub fn watch(self: &Arc<Self>) {
        let me = self.clone();
        let _ = std::thread::Builder::new()
            .name("update-watch".into())
            .spawn(move || {
                std::thread::sleep(Duration::from_secs(if cfg!(feature = "test-key") {
                    1
                } else {
                    60
                }));
                loop {
                    if !matches!(me.phase(), Phase::Installing(_)) {
                        me.check();
                    }
                    std::thread::sleep(CHECK_EVERY);
                }
            });
    }

    pub fn view(&self) -> serde_json::Value {
        let v = |t: (u32, u32, u32)| format!("{}.{}.{}", t.0, t.1, t.2);
        let (state, extra) = match self.phase() {
            Phase::Unknown => ("unknown", serde_json::Value::Null),
            Phase::Checking => ("checking", serde_json::Value::Null),
            Phase::UpToDate => ("up_to_date", serde_json::Value::Null),
            Phase::Available(o) => ("available", serde_json::Value::String(v(o.version))),
            Phase::Installing(p) => ("installing", serde_json::json!(p)),
            Phase::Failed(e) => ("failed", serde_json::Value::String(e)),
        };
        serde_json::json!({"state": state, "detail": extra, "current": v(self.current)})
    }
}

/// The running program (Windows, Linux) or app bundle (macOS).
fn installed_path() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    if cfg!(target_os = "macos") {
        // .../Wi-Live.app/Contents/MacOS/wilive
        let app = exe
            .parent()
            .and_then(Path::parent)
            .and_then(Path::parent)
            .ok_or("not inside an app bundle")?;
        if app.extension().and_then(|e| e.to_str()) != Some("app") {
            return Err("not running from an installed Wi-Live.app".into());
        }
        Ok(app.to_path_buf())
    } else {
        Ok(exe)
    }
}

/// `x` and its sibling with a suffix (`Wi-Live.app.new`, `wilive.exe.old`).
fn sibling(p: &Path, suffix: &str) -> PathBuf {
    let mut s = p.as_os_str().to_owned();
    s.push(suffix);
    PathBuf::from(s)
}

/// New beside old, then two renames: at every moment there is a whole program at the
/// installed path or a whole one beside it. Returns what to start.
pub fn swap_in(installed: &Path, new_ready: &Path) -> Result<(), String> {
    let old = sibling(installed, ".old");
    let _ = remove_any(&old);
    std::fs::rename(installed, &old).map_err(|e| format!("moving the running copy aside: {e}"))?;
    if let Err(e) = std::fs::rename(new_ready, installed) {
        let _ = std::fs::rename(&old, installed); // put it back
        return Err(format!("putting the new copy in place: {e}"));
    }
    Ok(())
}

fn remove_any(p: &Path) -> std::io::Result<()> {
    if p.is_dir() {
        std::fs::remove_dir_all(p)
    } else {
        std::fs::remove_file(p)
    }
}

/// What an earlier update left beside the program.
pub fn clean_old() {
    if let Ok(p) = installed_path() {
        let _ = remove_any(&sibling(&p, ".old"));
        let _ = remove_any(&sibling(&p, ".new"));
    }
}

#[cfg(not(target_os = "macos"))]
fn put_in_place(data: &[u8], _o: &Offer) -> Result<PathBuf, String> {
    let installed = installed_path()?;
    let new = sibling(&installed, ".new");
    std::fs::write(&new, data).map_err(|e| format!("writing the new copy: {e}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&new, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| e.to_string())?;
    }
    swap_in(&installed, &new)?;
    Ok(installed)
}

#[cfg(target_os = "macos")]
fn put_in_place(data: &[u8], _o: &Offer) -> Result<PathBuf, String> {
    use std::process::Command;
    let installed = installed_path()?;
    let work = std::env::temp_dir().join(format!("wilive-update-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).map_err(|e| e.to_string())?;
    let dmg = work.join("update.dmg");
    std::fs::write(&dmg, data).map_err(|e| e.to_string())?;
    let mnt = work.join("mnt");
    std::fs::create_dir_all(&mnt).map_err(|e| e.to_string())?;
    let ok = Command::new("hdiutil")
        .args(["attach", "-quiet", "-nobrowse", "-readonly", "-mountpoint"])
        .arg(&mnt)
        .arg(&dmg)
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !ok {
        return Err("the disk image did not open".into());
    }
    let r = (|| {
        let app = mnt.join("Wi-Live.app");
        // Wi-Live's own certificate, or nothing: the same rule tools/sign_mac_release.sh checks.
        let out = Command::new("codesign")
            .args(["-d", "-r-"])
            .arg(&app)
            .output()
            .map_err(|e| e.to_string())?;
        let text = String::from_utf8_lossy(&out.stdout).to_string()
            + &String::from_utf8_lossy(&out.stderr);
        let want = format!("designated => identifier \"live.wilive.app\" and certificate root = H\"{MAC_CERT_SHA1}\"");
        if !text.lines().any(|l| l.trim() == want) {
            return Err("the new app is not signed with Wi-Live's certificate".to_string());
        }
        let new = sibling(&installed, ".new");
        let _ = std::fs::remove_dir_all(&new);
        let ok = Command::new("ditto")
            .arg(&app)
            .arg(&new)
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if !ok {
            return Err("copying the new app failed".into());
        }
        swap_in(&installed, &new)
    })();
    let _ = Command::new("hdiutil")
        .args(["detach", "-quiet"])
        .arg(&mnt)
        .status();
    let _ = std::fs::remove_dir_all(&work);
    r.map(|_| installed)
}

/// Starts the new copy; it waits for this one to leave (the single-instance lock).
fn relaunch(installed: &Path) {
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("/usr/bin/open")
        .arg("-n")
        .arg(installed)
        .args(["--args", "--after-update"])
        .spawn();
    #[cfg(not(target_os = "macos"))]
    let _ = std::process::Command::new(installed)
        .arg("--after-update")
        .spawn();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_swap_puts_the_new_copy_in_place_and_keeps_the_old_beside_it() {
        let d = std::env::temp_dir().join(format!("wilive-swap-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let installed = d.join("wilive");
        std::fs::write(&installed, b"old").unwrap();
        let new = sibling(&installed, ".new");
        std::fs::write(&new, b"new").unwrap();
        swap_in(&installed, &new).unwrap();
        assert_eq!(std::fs::read(&installed).unwrap(), b"new");
        assert_eq!(std::fs::read(sibling(&installed, ".old")).unwrap(), b"old");
        assert!(!new.exists());
    }

    #[test]
    fn a_failed_swap_puts_the_old_copy_back() {
        let d = std::env::temp_dir().join(format!("wilive-swap2-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let installed = d.join("wilive");
        std::fs::write(&installed, b"old").unwrap();
        let missing = d.join("nothing-here.new");
        assert!(swap_in(&installed, &missing).is_err());
        assert_eq!(
            std::fs::read(&installed).unwrap(),
            b"old",
            "still the old copy, whole"
        );
    }

    #[test]
    fn this_computer_has_a_release_file() {
        let (os, arch) = os_arch();
        assert!(
            update::asset_for(os, arch, (1, 0, 0)).is_some(),
            "{os} {arch}"
        );
    }
}
