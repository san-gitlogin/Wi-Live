//! The pairing's bytes, pinned: `vectors/pairing_v1.json` holds fixed inputs and the
//! outputs this crate computes. web/crypto.js is checked against the same file
//! (tests/unit/test_crypto_js.py), so the browser and the app can never drift.
//! WILIVE_WRITE_VECTORS=1 rewrites the file (only on purpose, with a DECISIONS entry).
use wilive_core::pair::*;
use x25519_dalek::{PublicKey, StaticSecret};

fn compute() -> serde_json::Value {
    let v_secret = [0x11u8; 32];
    let a_secret = [0x22u8; 32];
    let n_v = [0x33u8; 16];
    let n_a = [0x44u8; 16];
    let v_pub = PublicKey::from(&StaticSecret::from(v_secret)).to_bytes();
    let r = Responder::from_parts(a_secret, n_a);
    let c = commit(&v_pub, &n_v);
    let (code, key) = r.finish(&v_pub, &c, &n_v).unwrap();
    let shared = StaticSecret::from(v_secret).diffie_hellman(&PublicKey::from(r.a_pub));
    let id = "0123456789abcdef0123456789abcdef";
    let chal = "c0ffeec0ffeec0ffeec0ffeec0ffee00";
    let fp = "ab".repeat(32);
    serde_json::json!({
        "v_secret": hex::encode(v_secret), "a_secret": hex::encode(a_secret),
        "n_v": hex::encode(n_v), "n_a": hex::encode(n_a),
        "v_pub": hex::encode(v_pub), "a_pub": hex::encode(r.a_pub),
        "commit": hex::encode(c), "shared": hex::encode(shared.as_bytes()),
        "code": code, "code_text": format_code(code), "key": hex::encode(key.as_bytes()),
        "id": id, "chal": chal, "hello_mac": hex::encode(hello_mac(&key, id, chal)),
        "fp": fp, "fp_mac": hex::encode(fp_mac(&key, chal, &fp)),
    })
}

#[test]
fn the_pairing_bytes_match_the_pinned_vectors() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../vectors/pairing_v1.json");
    let got = compute();
    if std::env::var("WILIVE_WRITE_VECTORS").is_ok_and(|v| v == "1") {
        std::fs::write(path, serde_json::to_string_pretty(&got).unwrap() + "\n").unwrap();
    }
    let want: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path).expect("vectors file")).unwrap();
    assert_eq!(got, want);
}
