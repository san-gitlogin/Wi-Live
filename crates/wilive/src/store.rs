//! The app's folder: allowed devices, the access log, the WebRTC certificate and
//! the owner's settings. Every file is written whole (temp file, then rename) and,
//! on Unix, readable by this user only. A damaged file never stops the app: it
//! starts empty and says so in the log.
//!
//!   devices.json   allowed devices with their keys (owner-only)
//!   access.json    the access log
//!   dtls.cert/.key the certificate viewers' pages check (stable fingerprint)
//!   settings.json  sound, microphone, start at login

use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use wilive_core::devices::Devices;
use wilive_core::log::AccessLog;
use wilive_server::state::Persist;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Settings {
    pub start_at_login: bool,
    pub sound: bool,
    pub mic: bool,
    /// The port viewers last used, tried first so their bookmark keeps working.
    pub port: Option<u16>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            start_at_login: true,
            sound: true,
            mic: false,
            port: None,
        }
    }
}

pub struct Store {
    dir: PathBuf,
}

impl Store {
    pub fn default_dir() -> PathBuf {
        let base = dirs::config_dir().unwrap_or_else(std::env::temp_dir);
        if cfg!(target_os = "linux") {
            base.join("wilive")
        } else {
            base.join("Wi-Live")
        }
    }

    pub fn open(dir: PathBuf) -> std::io::Result<Store> {
        fs::create_dir_all(&dir)?;
        private_dir(&dir);
        Ok(Store { dir })
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn devices(&self) -> Devices {
        match fs::read_to_string(self.dir.join("devices.json")) {
            Ok(s) => Devices::from_json(&s).unwrap_or_else(|e| {
                log::warn!("devices.json is damaged ({e}); starting with no devices");
                let _ = fs::rename(
                    self.dir.join("devices.json"),
                    self.dir.join("devices.damaged.json"),
                );
                Devices::default()
            }),
            Err(_) => Devices::default(),
        }
    }

    pub fn access_log(&self) -> AccessLog {
        fs::read_to_string(self.dir.join("access.json"))
            .map(|s| AccessLog::from_json(&s))
            .unwrap_or_default()
    }

    pub fn settings(&self) -> Settings {
        fs::read_to_string(self.dir.join("settings.json"))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save_settings(&self, s: &Settings) {
        if let Err(e) = write_atomic(
            &self.dir.join("settings.json"),
            serde_json::to_string_pretty(s)
                .expect("plain data")
                .as_bytes(),
            false,
        ) {
            log::warn!("could not save settings: {e}");
        }
    }

    /// The WebRTC certificate, made on first use and kept.
    pub fn cert(&self) -> Option<wilive_stream::rtc::DtlsCert> {
        let (c, k) = (self.dir.join("dtls.cert"), self.dir.join("dtls.key"));
        if let (Ok(certificate), Ok(private_key)) = (fs::read(&c), fs::read(&k)) {
            if !certificate.is_empty() && !private_key.is_empty() {
                return Some(wilive_stream::rtc::DtlsCert {
                    certificate,
                    private_key,
                });
            }
        }
        let made = wilive_stream::rtc::new_cert()?;
        let saved = write_atomic(&k, &made.private_key, true)
            .and_then(|_| write_atomic(&c, &made.certificate, true));
        if let Err(e) = saved {
            log::warn!(
                "could not save the certificate (its fingerprint will change next run): {e}"
            );
        }
        Some(made)
    }
}

/// Saves what the server changes, as it changes.
pub struct FilePersist {
    pub dir: PathBuf,
}

impl Persist for FilePersist {
    fn save_devices(&self, json: &str) {
        if let Err(e) = write_atomic(&self.dir.join("devices.json"), json.as_bytes(), true) {
            log::warn!("could not save the devices: {e}");
        }
    }
    fn save_log(&self, json: &str) {
        if let Err(e) = write_atomic(&self.dir.join("access.json"), json.as_bytes(), true) {
            log::warn!("could not save the access log: {e}");
        }
    }
}

pub fn write_atomic(path: &Path, data: &[u8], private: bool) -> std::io::Result<()> {
    let tmp = path.with_extension("tmp");
    {
        let mut f = open_new(&tmp, private)?;
        f.write_all(data)?;
        f.sync_all()?;
    }
    fs::rename(&tmp, path)
}

#[cfg(unix)]
fn open_new(p: &Path, private: bool) -> std::io::Result<fs::File> {
    use std::os::unix::fs::OpenOptionsExt;
    let _ = fs::remove_file(p);
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(if private { 0o600 } else { 0o644 })
        .open(p)
}

#[cfg(not(unix))]
fn open_new(p: &Path, _private: bool) -> std::io::Result<fs::File> {
    // %APPDATA% is already readable only by this user.
    let _ = fs::remove_file(p);
    fs::OpenOptions::new().write(true).create_new(true).open(p)
}

fn private_dir(_dir: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(_dir, fs::Permissions::from_mode(0o700));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> Store {
        let d = std::env::temp_dir().join(format!("wilive-store-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        Store::open(d).unwrap()
    }

    #[test]
    fn devices_and_the_log_survive_a_restart_and_keys_are_owner_only() {
        let s = tmp("persist");
        let p = FilePersist {
            dir: s.dir().to_path_buf(),
        };
        let mut ds = Devices::default();
        ds.add(wilive_core::devices::Device::new(
            "d1".into(),
            "Pixel".into(),
            "".into(),
            "".into(),
            &wilive_core::pair::DeviceKey::from_bytes([3; 32]),
            1,
        ));
        p.save_devices(&ds.to_json());
        assert_eq!(s.devices().views(), ds.views());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(s.dir().join("devices.json"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(mode, 0o600);
            let dmode = fs::metadata(s.dir()).unwrap().permissions().mode() & 0o777;
            assert_eq!(dmode, 0o700);
        }
    }

    #[test]
    fn a_damaged_devices_file_starts_empty_and_is_kept_aside() {
        let s = tmp("damaged");
        fs::write(s.dir().join("devices.json"), "{not json").unwrap();
        assert!(s.devices().is_empty());
        assert!(s.dir().join("devices.damaged.json").exists());
    }

    #[test]
    fn settings_default_and_round_trip() {
        let s = tmp("settings");
        assert_eq!(s.settings(), Settings::default());
        let x = Settings {
            mic: true,
            port: Some(47901),
            ..Default::default()
        };
        s.save_settings(&x);
        assert_eq!(s.settings(), x);
        fs::write(s.dir().join("settings.json"), "[]").unwrap();
        assert_eq!(s.settings(), Settings::default(), "damaged: defaults");
    }

    #[test]
    fn the_certificate_is_made_once_and_kept() {
        let s = tmp("cert");
        let a = s.cert().unwrap();
        let b = s.cert().unwrap();
        assert_eq!(
            wilive_stream::rtc::cert_fp(&a),
            wilive_stream::rtc::cert_fp(&b)
        );
    }
}
