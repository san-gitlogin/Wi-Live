//! Updates, decided (docs/RELEASING.md). Every release publishes SHA256SUMS and its
//! signature (ECDSA P-256 over SHA-256, DER), made with the release key whose public
//! half is built in here. Nothing a release says is believed before its signature
//! checks; nothing downloaded is used before its hash matches the signed list.
//!
//! Pure: no network, no files. The app (crates/wilive/src/updater.rs) does the I/O.

use p256::ecdsa::signature::Verifier;
use p256::ecdsa::{Signature, VerifyingKey};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

/// The release key (tools/release_keys.json), as an uncompressed SEC1 point.
pub const RELEASE_KEY: &str = "0496d2da368e7422031cd01f0e70b4797e416800ce0f82d441ce2db16ea1e623e93eb34322abc16b51313e9fcdb18b3d74ce55edf1a4f0b74f7b30d1c359d2289c";

#[derive(Debug, PartialEq, Eq)]
pub enum UpdateError {
    BadSignature,
    BadSums,
    NoAssetForThisComputer,
    HashMismatch,
    BadVersion,
}

/// In a test build's binary, findable: tools/make_release.sh refuses any binary
/// that contains it.
#[cfg(feature = "test-key")]
pub static TEST_BUILD: &str = "wilive:test-key-build:never-ship";

/// The keys this build trusts.
pub fn trusted_keys() -> Vec<String> {
    #[allow(unused_mut)]
    let mut k = vec![RELEASE_KEY.to_string()];
    #[cfg(feature = "test-key")]
    {
        std::hint::black_box(TEST_BUILD);
        k.push(test_key_hex());
    }
    k
}

/// The test key's public point, derived from its fixed private scalar (so the
/// constant above can never drift from it).
pub fn test_key_hex() -> String {
    let sk = p256::ecdsa::SigningKey::from_bytes(&test_scalar().into()).expect("a valid scalar");
    hex::encode(sk.verifying_key().to_encoded_point(false).as_bytes())
}

/// The TEST private scalar: published on purpose, for tests and test builds only.
pub fn test_scalar() -> [u8; 32] {
    let mut s = [0u8; 32];
    for (i, b) in s.iter_mut().enumerate() {
        *b = 0x57 ^ (i as u8).wrapping_mul(29);
    }
    s
}

/// Checks `sig` (DER) over `sums` against any trusted key.
pub fn verify_sums(sums: &[u8], sig: &[u8], keys: &[String]) -> Result<(), UpdateError> {
    let sig = Signature::from_der(sig).map_err(|_| UpdateError::BadSignature)?;
    for k in keys {
        let Ok(bytes) = hex::decode(k) else { continue };
        let Ok(vk) = VerifyingKey::from_sec1_bytes(&bytes) else {
            continue;
        };
        if vk.verify(sums, &sig).is_ok() {
            return Ok(());
        }
    }
    Err(UpdateError::BadSignature)
}

/// `name -> sha256 hex` from a `shasum -a 256` listing. Strict: every line is a
/// 64-hex hash, two spaces (or a space and `*`), a plain file name.
pub fn parse_sums(text: &str) -> Result<BTreeMap<String, String>, UpdateError> {
    let mut m = BTreeMap::new();
    for l in text.lines().filter(|l| !l.trim().is_empty()) {
        let (h, rest) = l.split_at_checked(64).ok_or(UpdateError::BadSums)?;
        let name = rest
            .strip_prefix("  ")
            .or_else(|| rest.strip_prefix(" *"))
            .ok_or(UpdateError::BadSums)?;
        if !h.bytes().all(|c| c.is_ascii_hexdigit())
            || name.is_empty()
            || name.contains('/')
            || name.contains('\\')
            || name.contains("..")
        {
            return Err(UpdateError::BadSums);
        }
        m.insert(name.to_string(), h.to_ascii_lowercase());
    }
    if m.is_empty() {
        return Err(UpdateError::BadSums);
    }
    Ok(m)
}

/// The release's version, read from its macOS disk image's name (Wi-Live-X.Y.Z-macos.dmg).
pub fn release_version(sums: &BTreeMap<String, String>) -> Result<(u32, u32, u32), UpdateError> {
    sums.keys()
        .find_map(|n| {
            n.strip_prefix("Wi-Live-")
                .and_then(|r| r.strip_suffix("-macos.dmg"))
        })
        .and_then(parse_version)
        .ok_or(UpdateError::BadVersion)
}

pub fn parse_version(v: &str) -> Option<(u32, u32, u32)> {
    let mut p = v.trim().trim_start_matches('v').split('.');
    let t = (
        p.next()?.parse().ok()?,
        p.next()?.parse().ok()?,
        p.next()?.parse().ok()?,
    );
    p.next().is_none().then_some(t)
}

/// The file this computer installs from a release.
pub fn asset_for(os: &str, arch: &str, version: (u32, u32, u32)) -> Option<String> {
    Some(match (os, arch) {
        ("macos", _) => format!(
            "Wi-Live-{}.{}.{}-macos.dmg",
            version.0, version.1, version.2
        ),
        ("windows", "x86_64") | ("windows", "aarch64") => "wilive-windows-x86_64.exe".into(),
        ("linux", "x86_64") => "wilive-linux-x86_64".into(),
        ("linux", "aarch64") => "wilive-linux-aarch64".into(),
        _ => return None,
    })
}

/// A download is used only if its hash is the signed one.
pub fn check_download(data: &[u8], want_hex: &str) -> Result<(), UpdateError> {
    if hex::encode(Sha256::digest(data)) == want_hex.to_ascii_lowercase() {
        Ok(())
    } else {
        Err(UpdateError::HashMismatch)
    }
}

/// What a check found, for the owner's window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Offer {
    pub version: (u32, u32, u32),
    pub asset: String,
    pub sha256: String,
}

/// Verifies a release (signature first) and says whether it is newer than `current`
/// and what to download for this computer.
pub fn decide(
    sums: &[u8],
    sig: &[u8],
    keys: &[String],
    current: (u32, u32, u32),
    os: &str,
    arch: &str,
) -> Result<Option<Offer>, UpdateError> {
    verify_sums(sums, sig, keys)?;
    let m = parse_sums(std::str::from_utf8(sums).map_err(|_| UpdateError::BadSums)?)?;
    let v = release_version(&m)?;
    if v <= current {
        return Ok(None);
    }
    let asset = asset_for(os, arch, v).ok_or(UpdateError::NoAssetForThisComputer)?;
    let sha256 = m
        .get(&asset)
        .ok_or(UpdateError::NoAssetForThisComputer)?
        .clone();
    Ok(Some(Offer {
        version: v,
        asset,
        sha256,
    }))
}

/// Signs `sums` with the TEST key (tests and the release-server fixture only).
pub fn test_sign(sums: &[u8]) -> Vec<u8> {
    use p256::ecdsa::signature::Signer;
    let sk = p256::ecdsa::SigningKey::from_bytes(&test_scalar().into()).expect("a valid scalar");
    let s: Signature = sk.sign(sums);
    s.to_der().as_bytes().to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sums(v: &str) -> String {
        let h = |c: char| c.to_string().repeat(64);
        format!("{}  Wi-Live-{v}-macos.dmg\n{}  wilive-windows-x86_64.exe\n{}  wilive-linux-x86_64\n{}  wilive-linux-aarch64\n", h('a'), h('b'), h('c'), h('d'))
    }
    fn test_keys() -> Vec<String> {
        vec![test_key_hex()]
    }

    #[test]
    fn a_newer_signed_release_is_offered_with_this_computers_file() {
        let s = sums("0.2.0");
        let sig = test_sign(s.as_bytes());
        let o = decide(
            s.as_bytes(),
            &sig,
            &test_keys(),
            (0, 1, 0),
            "macos",
            "aarch64",
        )
        .unwrap()
        .unwrap();
        assert_eq!(o.version, (0, 2, 0));
        assert_eq!(o.asset, "Wi-Live-0.2.0-macos.dmg");
        assert_eq!(o.sha256, "a".repeat(64));
        let w = decide(
            s.as_bytes(),
            &sig,
            &test_keys(),
            (0, 1, 0),
            "windows",
            "x86_64",
        )
        .unwrap()
        .unwrap();
        assert_eq!(w.asset, "wilive-windows-x86_64.exe");
        let l = decide(
            s.as_bytes(),
            &sig,
            &test_keys(),
            (0, 1, 0),
            "linux",
            "aarch64",
        )
        .unwrap()
        .unwrap();
        assert_eq!(l.sha256, "d".repeat(64));
    }

    #[test]
    fn the_same_or_an_older_release_is_not_offered() {
        let s = sums("0.1.0");
        let sig = test_sign(s.as_bytes());
        assert_eq!(
            decide(
                s.as_bytes(),
                &sig,
                &test_keys(),
                (0, 1, 0),
                "macos",
                "aarch64"
            )
            .unwrap(),
            None
        );
        assert_eq!(
            decide(
                s.as_bytes(),
                &sig,
                &test_keys(),
                (0, 2, 0),
                "macos",
                "aarch64"
            )
            .unwrap(),
            None
        );
    }

    #[test]
    fn a_tampered_list_or_a_foreign_key_is_refused_before_anything_is_read() {
        let s = sums("9.9.9");
        let sig = test_sign(s.as_bytes());
        let tampered = s.replace("aaaa", "aaab");
        assert_eq!(
            decide(
                tampered.as_bytes(),
                &sig,
                &test_keys(),
                (0, 1, 0),
                "macos",
                "x"
            )
            .unwrap_err(),
            UpdateError::BadSignature
        );
        assert_eq!(
            decide(
                s.as_bytes(),
                &sig,
                &[RELEASE_KEY.to_string()],
                (0, 1, 0),
                "macos",
                "x"
            )
            .unwrap_err(),
            UpdateError::BadSignature,
            "the release key did not sign it"
        );
        assert_eq!(
            decide(s.as_bytes(), b"junk", &test_keys(), (0, 1, 0), "macos", "x").unwrap_err(),
            UpdateError::BadSignature
        );
    }

    #[test]
    fn a_release_build_trusts_only_the_release_key() {
        if !cfg!(feature = "test-key") {
            assert_eq!(trusted_keys(), vec![RELEASE_KEY.to_string()]);
        }
        assert_ne!(test_key_hex(), RELEASE_KEY);
    }

    #[test]
    fn sums_are_parsed_strictly() {
        assert!(parse_sums(&format!("{}  ../../etc/passwd\n", "a".repeat(64))).is_err());
        assert!(parse_sums(&format!("{}  dir/file\n", "a".repeat(64))).is_err());
        assert!(parse_sums("zz  file\n").is_err());
        assert!(parse_sums("").is_err());
        assert_eq!(
            parse_sums(&format!("{} *f.exe\n", "A".repeat(64))).unwrap()["f.exe"],
            "a".repeat(64)
        );
    }

    #[test]
    fn a_download_is_used_only_if_its_hash_is_the_signed_one() {
        let d = b"the app";
        let h = hex::encode(Sha256::digest(d));
        assert!(check_download(d, &h).is_ok());
        assert_eq!(
            check_download(b"another app", &h).unwrap_err(),
            UpdateError::HashMismatch
        );
    }

    #[test]
    fn versions_compare_as_numbers() {
        assert_eq!(parse_version("v0.10.2"), Some((0, 10, 2)));
        assert!(
            parse_version("0.10").is_none()
                && parse_version("0.1.2.3").is_none()
                && parse_version("x.1.2").is_none()
        );
        assert!((0, 10, 0) > (0, 9, 9));
        assert!(asset_for("freebsd", "x86_64", (1, 0, 0)).is_none());
    }

    #[test]
    fn the_real_release_signature_of_v0_1_0_checks() {
        // tools/make_release.sh signs with ~/.wilive-release; vectors/ holds v0.1.0's
        // published SHA256SUMS and its signature, so the built-in key is proven real.
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../vectors");
        let sums = std::fs::read(format!("{dir}/release-v0.1.0.SHA256SUMS")).unwrap();
        let sig = std::fs::read(format!("{dir}/release-v0.1.0.SHA256SUMS.sig")).unwrap();
        assert!(verify_sums(&sums, &sig, &[RELEASE_KEY.to_string()]).is_ok());
        let o = decide(
            &sums,
            &sig,
            &[RELEASE_KEY.to_string()],
            (0, 0, 9),
            "linux",
            "x86_64",
        )
        .unwrap()
        .unwrap();
        assert_eq!(o.version, (0, 1, 0));
    }
}
