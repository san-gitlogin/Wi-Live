//! Pairing and the proof each session gives (docs/DESIGN.md "Trust"). The viewer
//! (a browser, pure-JS crypto in `web/crypto.js`) starts; this computer answers.
//! `vectors/pairing_v1.json` pins the Rust and JS sides to identical bytes.
//!
//! Pure: no clock, no network, and randomness only in [`Responder::new`].

use hkdf::Hkdf;
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use x25519_dalek::{PublicKey, StaticSecret};
use zeroize::{Zeroize, ZeroizeOnDrop};

type HmacSha256 = Hmac<Sha256>;

pub const NONCE_LEN: usize = 16;
pub const KEY_LEN: usize = 32;
pub const HKDF_INFO: &[u8] = b"wilive-device-v1";

pub type Nonce = [u8; NONCE_LEN];

#[derive(Debug, PartialEq, Eq)]
pub enum PairError {
    /// A low-order public key: the shared secret would be all zeros.
    WeakPeerKey,
    /// The revealed nonce does not match the commitment.
    CommitMismatch,
}

/// A device's key. Never printed; wiped on drop.
#[derive(Clone, Zeroize, ZeroizeOnDrop, PartialEq, Eq)]
pub struct DeviceKey([u8; KEY_LEN]);

impl DeviceKey {
    pub fn from_bytes(b: [u8; KEY_LEN]) -> Self {
        Self(b)
    }
    pub fn as_bytes(&self) -> &[u8; KEY_LEN] {
        &self.0
    }
}

impl std::fmt::Debug for DeviceKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("DeviceKey(..)")
    }
}

/// `H("wl-commit" || v_pub || nV)`: sent before this computer chooses its key, so
/// the viewer cannot pick `nV` after seeing it (no grinding for a matching code).
pub fn commit(v_pub: &[u8; 32], n_v: &Nonce) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(b"wl-commit");
    h.update(v_pub);
    h.update(n_v);
    h.finalize().into()
}

/// The six-digit code both screens show, in `0..1_000_000`.
pub fn sas(v_pub: &[u8; 32], a_pub: &[u8; 32], n_v: &Nonce, n_a: &Nonce) -> u32 {
    let mut h = Sha256::new();
    h.update(b"wl-sas");
    h.update(v_pub);
    h.update(a_pub);
    h.update(n_v);
    h.update(n_a);
    let d = h.finalize();
    u32::from_be_bytes([d[0], d[1], d[2], d[3]]) % 1_000_000
}

/// Zero-padded and split for reading aloud: 42 reads "000 042" on both screens.
pub fn format_code(code: u32) -> String {
    let s = format!("{:06}", code % 1_000_000);
    format!("{} {}", &s[..3], &s[3..])
}

pub fn derive_key(
    shared: &[u8; 32],
    v_pub: &[u8; 32],
    a_pub: &[u8; 32],
    n_v: &Nonce,
    n_a: &Nonce,
) -> DeviceKey {
    let mut salt = Sha256::new();
    salt.update(v_pub);
    salt.update(a_pub);
    salt.update(n_v);
    salt.update(n_a);
    let salt: [u8; 32] = salt.finalize().into();
    let hk = Hkdf::<Sha256>::new(Some(&salt), shared);
    let mut out = [0u8; KEY_LEN];
    hk.expand(HKDF_INFO, &mut out)
        .expect("32 bytes is a valid HKDF length");
    DeviceKey(out)
}

fn hmac(k: &DeviceKey, msg: &[u8]) -> [u8; 32] {
    let mut m = <HmacSha256 as Mac>::new_from_slice(k.as_bytes()).expect("any key length");
    m.update(msg);
    m.finalize().into_bytes().into()
}

/// The exact ASCII a session's proof covers.
pub fn hello_message(id: &str, chal: &str) -> String {
    format!("wl-hello|{id}|{chal}")
}

pub fn hello_mac(k: &DeviceKey, id: &str, chal: &str) -> [u8; 32] {
    hmac(k, hello_message(id, chal).as_bytes())
}

/// Constant time: how long a wrong proof takes says nothing about how wrong.
pub fn hello_ok(k: &DeviceKey, id: &str, chal: &str, mac: &[u8]) -> bool {
    mac.len() == 32 && bool::from(hello_mac(k, id, chal).ct_eq(mac))
}

/// Ties the DTLS fingerprint to this session: the viewer checks it with its key
/// before trusting the answer's `a=fingerprint`, so a page swapped on the way
/// cannot hand it another computer's media.
pub fn fp_mac(k: &DeviceKey, chal: &str, fp: &str) -> [u8; 32] {
    hmac(k, format!("wl-fp|{chal}|{fp}").as_bytes())
}

/// This computer's half of one pairing.
pub struct Responder {
    secret: StaticSecret,
    pub a_pub: [u8; 32],
    pub n_a: Nonce,
}

impl Responder {
    pub fn new() -> Self {
        let mut s = [0u8; 32];
        let mut n = [0u8; NONCE_LEN];
        getrandom::fill(&mut s).expect("the OS random source");
        getrandom::fill(&mut n).expect("the OS random source");
        Self::from_parts(s, n)
    }

    /// Fixed inputs, for the test vectors.
    pub fn from_parts(secret: [u8; 32], n_a: Nonce) -> Self {
        let secret = StaticSecret::from(secret);
        let a_pub = PublicKey::from(&secret).to_bytes();
        Responder { secret, a_pub, n_a }
    }

    /// After the viewer reveals `n_v`: checks its commitment and returns the code
    /// to show and the key to keep if the owner allows.
    pub fn finish(
        &self,
        v_pub: &[u8; 32],
        commit_v: &[u8; 32],
        n_v: &Nonce,
    ) -> Result<(u32, DeviceKey), PairError> {
        if !bool::from(commit(v_pub, n_v).ct_eq(commit_v)) {
            return Err(PairError::CommitMismatch);
        }
        let shared = self.secret.diffie_hellman(&PublicKey::from(*v_pub));
        if !shared.was_contributory() {
            return Err(PairError::WeakPeerKey);
        }
        let code = sas(v_pub, &self.a_pub, n_v, &self.n_a);
        Ok((
            code,
            derive_key(shared.as_bytes(), v_pub, &self.a_pub, n_v, &self.n_a),
        ))
    }
}

impl Default for Responder {
    fn default() -> Self {
        Self::new()
    }
}

/// A fresh random value in lowercase hex (ids, challenges).
pub fn random_hex(bytes: usize) -> String {
    let mut b = vec![0u8; bytes];
    getrandom::fill(&mut b).expect("the OS random source");
    hex::encode(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn viewer(secret: [u8; 32], n_v: Nonce) -> (StaticSecret, [u8; 32], Nonce, [u8; 32]) {
        let s = StaticSecret::from(secret);
        let p = PublicKey::from(&s).to_bytes();
        let c = commit(&p, &n_v);
        (s, p, n_v, c)
    }

    #[test]
    fn both_sides_get_the_same_code_and_key() {
        let (vs, vp, nv, c) = viewer([3; 32], [4; 16]);
        let r = Responder::from_parts([5; 32], [6; 16]);
        let (code, k) = r.finish(&vp, &c, &nv).unwrap();
        let shared = vs.diffie_hellman(&PublicKey::from(r.a_pub));
        assert_eq!(code, sas(&vp, &r.a_pub, &nv, &r.n_a));
        assert_eq!(k, derive_key(shared.as_bytes(), &vp, &r.a_pub, &nv, &r.n_a));
        assert!(code < 1_000_000);
    }

    #[test]
    fn a_reveal_that_does_not_match_the_commitment_is_refused() {
        let (_, vp, _, c) = viewer([3; 32], [4; 16]);
        let r = Responder::from_parts([5; 32], [6; 16]);
        assert_eq!(
            r.finish(&vp, &c, &[9; 16]).unwrap_err(),
            PairError::CommitMismatch
        );
    }

    #[test]
    fn a_low_order_key_is_refused() {
        let zero = [0u8; 32];
        let n = [1u8; 16];
        let r = Responder::from_parts([5; 32], [6; 16]);
        assert_eq!(
            r.finish(&zero, &commit(&zero, &n), &n).unwrap_err(),
            PairError::WeakPeerKey
        );
    }

    #[test]
    fn the_hello_proof_binds_the_device_and_the_challenge() {
        let k = DeviceKey::from_bytes([7; 32]);
        let m = hello_mac(&k, "dev1", "c0ffee");
        assert!(hello_ok(&k, "dev1", "c0ffee", &m));
        assert!(!hello_ok(&k, "dev2", "c0ffee", &m), "another device");
        assert!(!hello_ok(&k, "dev1", "c0ffef", &m), "another challenge");
        assert!(
            !hello_ok(&DeviceKey::from_bytes([8; 32]), "dev1", "c0ffee", &m),
            "another key"
        );
        assert!(!hello_ok(&k, "dev1", "c0ffee", &m[..31]), "short");
    }

    #[test]
    fn codes_read_in_two_groups_of_three() {
        assert_eq!(format_code(42), "000 042");
        assert_eq!(format_code(999_999), "999 999");
    }

    #[test]
    fn the_key_never_prints() {
        assert_eq!(
            format!("{:?}", DeviceKey::from_bytes([1; 32])),
            "DeviceKey(..)"
        );
    }
}
