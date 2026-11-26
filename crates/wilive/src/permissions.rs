//! macOS keeps a privacy grant (Screen Recording, System Audio Recording,
//! Microphone) with the code signature the app had when it was granted. When
//! the signature changes - a development build updated to the certificate-signed release,
//! or Wi-Live 0.2.0 (Wi-Live's certificate) replacing a build signed "HID-Fi Local" - the old
//! entry stays: System Settings shows Wi-Live switched on, the app
//! does not match it, macOS asks again, and allowing again changes nothing
//! (seen in HID-Fi on 2026-10-30, and in Wi-Live on 2026-11-26: "Failed to match existing code
//! requirement").
//!
//! So at start: if this build's signature differs from the one recorded last
//! time and Screen Recording is not working, this app's own entries are reset
//! once (`tccutil reset <service> <its bundle id>`), and the next ask records
//! the right signature. A grant that works, or a "Don't Allow" given to this
//! same signature, is never touched.

use std::path::{Path, PathBuf};

/// What macOS records grants under, per service the app uses.
pub const SERVICES: [&str; 3] = ["ScreenCapture", "AudioCapture", "Microphone"];
const FILE: &str = "signature.txt";

/// Reset only when the grant does not work and the signature is new to us.
pub fn should_reset(capture_allowed: bool, recorded: Option<&str>, current: &str) -> bool {
    !capture_allowed && recorded != Some(current)
}

/// The app bundle this program runs from, and its bundle id. None outside a
/// bundle (a development build run from target/): nothing to reset then.
pub fn bundle_of(exe: &Path) -> Option<(PathBuf, String)> {
    let app = exe.parent()?.parent()?.parent()?; // X.app/Contents/MacOS/x
    if app.extension()? != "app" {
        return None;
    }
    let plist = std::fs::read_to_string(app.join("Contents/Info.plist")).ok()?;
    let id = plist
        .split("<key>CFBundleIdentifier</key>")
        .nth(1)?
        .split("<string>")
        .nth(1)?
        .split("</string>")
        .next()?
        .trim()
        .to_string();
    (!id.is_empty()).then(|| (app.to_path_buf(), id))
}

/// The designated requirement macOS compares grants against.
fn requirement(app: &Path) -> Option<String> {
    let out = std::process::Command::new("/usr/bin/codesign")
        .args(["-d", "-r", "-"])
        .arg(app)
        .output()
        .ok()?;
    let text =
        String::from_utf8_lossy(&out.stdout).to_string() + &String::from_utf8_lossy(&out.stderr);
    designated(&text)
}

/// From `codesign -d -r -`: a signed app says `designated => ...`; an ad-hoc one
/// (development builds) says it as a comment, `# designated => cdhash H"..."`.
pub fn designated(text: &str) -> Option<String> {
    text.lines()
        .map(|l| l.trim().trim_start_matches('#').trim())
        .find_map(|l| l.strip_prefix("designated => "))
        .map(str::to_string)
}

/// macOS's tccutil. Test builds only (feature "test-key"): WILIVE_TEST_TCCUTIL
/// names a stand-in that records its arguments, then runs the real one
/// (tests).
fn tccutil() -> PathBuf {
    #[cfg(feature = "test-key")]
    if let Some(p) = std::env::var_os("WILIVE_TEST_TCCUTIL") {
        return p.into();
    }
    "/usr/bin/tccutil".into()
}

/// Run once at start, off the main thread (codesign and tccutil take a moment).
pub fn check(dir: &Path, capture_allowed: bool) {
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let Some((app, id)) = bundle_of(&exe) else {
        return;
    };
    let Some(current) = requirement(&app) else {
        log::warn!("permissions: could not read this app's signature");
        return;
    };
    let path = dir.join(FILE);
    let recorded = std::fs::read_to_string(&path).ok();
    let recorded = recorded.as_deref().map(str::trim);
    if should_reset(capture_allowed, recorded, &current) {
        for s in SERVICES {
            match std::process::Command::new(tccutil())
                .args(["reset", s, &id])
                .output()
            {
                Ok(o) if o.status.success() => {}
                Ok(o) => log::warn!(
                    "permissions: tccutil reset {s} {id}: {}",
                    String::from_utf8_lossy(&o.stderr).trim()
                ),
                Err(e) => log::warn!("permissions: tccutil: {e}"),
            }
        }
        log::info!(
            "permissions: this app's signature changed and Screen Recording did not work - \
             cleared the old entries for {id}, so macOS asks once for this signature"
        );
    }
    if recorded != Some(current.as_str()) {
        if let Err(e) = std::fs::write(&path, &current) {
            log::warn!("permissions: could not record the signature: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_grant_that_fails_under_a_new_signature_is_reset() {
        let (old, new) = (
            "certificate leaf = H\"07c2\"",
            "certificate root = H\"7ec8\"",
        );
        // The owner's Mac: granted to the old signature, the new one is refused.
        assert!(should_reset(false, Some(old), new));
        // A first start (nothing recorded) that does not work.
        assert!(should_reset(false, None, new));
        // Works: never touched, whatever was recorded.
        assert!(!should_reset(true, Some(old), new));
        assert!(!should_reset(true, None, new));
        // The same signature, not allowed: the person's own "Don't Allow" stands.
        assert!(!should_reset(false, Some(new), new));
    }

    #[test]
    fn the_requirement_is_read_signed_or_ad_hoc() {
        let signed = "Executable=/A/x\ndesignated => identifier \"live.wilive.app\" and certificate root = H\"7ec8\"\n";
        assert_eq!(
            designated(signed).as_deref(),
            Some("identifier \"live.wilive.app\" and certificate root = H\"7ec8\"")
        );
        // Ad hoc: a comment - the very case that needs the reset most.
        let adhoc = "Executable=/A/x\n# designated => cdhash H\"9ce8d09b\"\n";
        assert_eq!(designated(adhoc).as_deref(), Some("cdhash H\"9ce8d09b\""));
        assert_eq!(designated("code object is not signed at all"), None);
    }

    #[test]
    fn the_bundle_is_found_only_inside_an_app() {
        let d = std::env::temp_dir().join(format!("wilive-perm-{}", std::process::id()));
        let app = d.join("X.app/Contents/MacOS");
        std::fs::create_dir_all(&app).unwrap();
        std::fs::write(
            d.join("X.app/Contents/Info.plist"),
            "<dict>\n<key>CFBundleName</key><string>X</string>\n\
             <key>CFBundleIdentifier</key>\n  <string>live.wilive.app</string>\n</dict>",
        )
        .unwrap();
        let (b, id) = bundle_of(&app.join("wilive")).unwrap();
        assert_eq!(id, "live.wilive.app");
        assert!(b.ends_with("X.app"));
        assert!(bundle_of(&d.join("target/release/wilive")).is_none());
        let _ = std::fs::remove_dir_all(&d);
    }
}
