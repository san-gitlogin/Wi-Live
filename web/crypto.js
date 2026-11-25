// Wi-Live: the crypto the page needs, in plain JS. WebCrypto's subtle API exists
// only on https pages, and the dashboard is served over http on the home network
// (docs/DECISIONS.md D-05). Each function is checked against its RFC vectors and
// against the Rust side's bytes (tests/unit/test_crypto_js.py, vectors/pairing_v1.json).
// Only crypto.getRandomValues is used from the browser; it works on http.
'use strict';
const WLC = (() => {
  // ---- SHA-256 (FIPS 180-4) ----
  const K = new Uint32Array([
    0x428a2f98,0x71374491,0xb5c0fbcf,0xe9b5dba5,0x3956c25b,0x59f111f1,0x923f82a4,0xab1c5ed5,
    0xd807aa98,0x12835b01,0x243185be,0x550c7dc3,0x72be5d74,0x80deb1fe,0x9bdc06a7,0xc19bf174,
    0xe49b69c1,0xefbe4786,0x0fc19dc6,0x240ca1cc,0x2de92c6f,0x4a7484aa,0x5cb0a9dc,0x76f988da,
    0x983e5152,0xa831c66d,0xb00327c8,0xbf597fc7,0xc6e00bf3,0xd5a79147,0x06ca6351,0x14292967,
    0x27b70a85,0x2e1b2138,0x4d2c6dfc,0x53380d13,0x650a7354,0x766a0abb,0x81c2c92e,0x92722c85,
    0xa2bfe8a1,0xa81a664b,0xc24b8b70,0xc76c51a3,0xd192e819,0xd6990624,0xf40e3585,0x106aa070,
    0x19a4c116,0x1e376c08,0x2748774c,0x34b0bcb5,0x391c0cb3,0x4ed8aa4a,0x5b9cca4f,0x682e6ff3,
    0x748f82ee,0x78a5636f,0x84c87814,0x8cc70208,0x90befffa,0xa4506ceb,0xbef9a3f7,0xc67178f2]);
  function sha256(msg) {
    const len = msg.length, bitLen = len * 8;
    const n = ((len + 9 + 63) >> 6) << 6;
    const m = new Uint8Array(n);
    m.set(msg); m[len] = 0x80;
    const dv = new DataView(m.buffer);
    dv.setUint32(n - 4, bitLen >>> 0); dv.setUint32(n - 8, Math.floor(bitLen / 0x100000000));
    const H = new Uint32Array([0x6a09e667,0xbb67ae85,0x3c6ef372,0xa54ff53a,0x510e527f,0x9b05688c,0x1f83d9ab,0x5be0cd19]);
    const W = new Uint32Array(64);
    for (let o = 0; o < n; o += 64) {
      for (let i = 0; i < 16; i++) W[i] = dv.getUint32(o + i * 4);
      for (let i = 16; i < 64; i++) {
        const a = W[i-15], b = W[i-2];
        const s0 = ((a>>>7)|(a<<25)) ^ ((a>>>18)|(a<<14)) ^ (a>>>3);
        const s1 = ((b>>>17)|(b<<15)) ^ ((b>>>19)|(b<<13)) ^ (b>>>10);
        W[i] = (W[i-16] + s0 + W[i-7] + s1) | 0;
      }
      let [a,b,c,d,e,f,g,h] = H;
      for (let i = 0; i < 64; i++) {
        const S1 = ((e>>>6)|(e<<26)) ^ ((e>>>11)|(e<<21)) ^ ((e>>>25)|(e<<7));
        const t1 = (h + S1 + ((e & f) ^ (~e & g)) + K[i] + W[i]) | 0;
        const S0 = ((a>>>2)|(a<<30)) ^ ((a>>>13)|(a<<19)) ^ ((a>>>22)|(a<<10));
        const t2 = (S0 + ((a & b) ^ (a & c) ^ (b & c))) | 0;
        h = g; g = f; f = e; e = (d + t1) | 0; d = c; c = b; b = a; a = (t1 + t2) | 0;
      }
      H[0]+=a; H[1]+=b; H[2]+=c; H[3]+=d; H[4]+=e; H[5]+=f; H[6]+=g; H[7]+=h;
    }
    const out = new Uint8Array(32);
    const odv = new DataView(out.buffer);
    for (let i = 0; i < 8; i++) odv.setUint32(i * 4, H[i]);
    return out;
  }

  // ---- bytes ----
  const enc = new TextEncoder();
  function cat(...parts) {
    const n = parts.reduce((s, p) => s + p.length, 0);
    const out = new Uint8Array(n);
    let o = 0;
    for (const p of parts) { out.set(p, o); o += p.length; }
    return out;
  }
  function hex(b) { return Array.from(b, x => x.toString(16).padStart(2, '0')).join(''); }
  function unhex(s) {
    if (typeof s !== 'string' || s.length % 2 || /[^0-9a-f]/i.test(s)) throw new Error('hex');
    const out = new Uint8Array(s.length / 2);
    for (let i = 0; i < out.length; i++) out[i] = parseInt(s.substr(i * 2, 2), 16);
    return out;
  }
  function utf8(s) { return enc.encode(s); }
  function random(n) { const b = new Uint8Array(n); crypto.getRandomValues(b); return b; }
  // Equal without an early exit.
  function same(a, b) {
    if (a.length !== b.length) return false;
    let d = 0;
    for (let i = 0; i < a.length; i++) d |= a[i] ^ b[i];
    return d === 0;
  }

  // ---- HMAC-SHA256 (RFC 2104) and HKDF (RFC 5869) ----
  function hmac(key, msg) {
    if (key.length > 64) key = sha256(key);
    const k = new Uint8Array(64); k.set(key);
    const ip = k.map(x => x ^ 0x36), op = k.map(x => x ^ 0x5c);
    return sha256(cat(op, sha256(cat(ip, msg))));
  }
  function hkdf(ikm, salt, info, len) {
    const prk = hmac(salt.length ? salt : new Uint8Array(32), ikm);
    const out = new Uint8Array(len);
    let t = new Uint8Array(0), o = 0;
    for (let i = 1; o < len; i++) {
      t = hmac(prk, cat(t, info, new Uint8Array([i])));
      out.set(t.subarray(0, Math.min(t.length, len - o)), o);
      o += t.length;
    }
    return out;
  }

  // ---- X25519 (RFC 7748), BigInt Montgomery ladder ----
  const P = (1n << 255n) - 19n, A24 = 121665n;
  const mod = a => { const r = a % P; return r < 0n ? r + P : r; };
  function pow(b, e) { let r = 1n; b = mod(b); while (e > 0n) { if (e & 1n) r = mod(r * b); b = mod(b * b); e >>= 1n; } return r; }
  function leToBig(b) { let r = 0n; for (let i = b.length - 1; i >= 0; i--) r = (r << 8n) | BigInt(b[i]); return r; }
  function bigToLe(x) { const out = new Uint8Array(32); for (let i = 0; i < 32; i++) { out[i] = Number(x & 0xffn); x >>= 8n; } return out; }
  function x25519(scalar, u) {
    const k = new Uint8Array(scalar); k[0] &= 248; k[31] &= 127; k[31] |= 64;
    const uu = new Uint8Array(u); uu[31] &= 127;
    const kn = leToBig(k), x1 = leToBig(uu) % P;
    let x2 = 1n, z2 = 0n, x3 = x1, z3 = 1n, swap = 0n;
    for (let t = 254n; t >= 0n; t--) {
      const kt = (kn >> t) & 1n;
      if (swap ^ kt) { [x2, x3] = [x3, x2]; [z2, z3] = [z3, z2]; }
      swap = kt;
      const a = mod(x2 + z2), aa = mod(a * a), b = mod(x2 - z2), bb = mod(b * b), e = mod(aa - bb);
      const c = mod(x3 + z3), d = mod(x3 - z3), da = mod(d * a), cb = mod(c * b);
      x3 = mod((da + cb) * (da + cb)); z3 = mod(x1 * mod((da - cb) * (da - cb)));
      x2 = mod(aa * bb); z2 = mod(e * mod(aa + A24 * e));
    }
    if (swap) { [x2, x3] = [x3, x2]; [z2, z3] = [z3, z2]; }
    return bigToLe(mod(x2 * pow(z2, P - 2n)));
  }
  const BASE = (() => { const b = new Uint8Array(32); b[0] = 9; return b; })();

  // ---- Wi-Live's pairing (docs/DESIGN.md "Trust"; Rust: wilive-core pair.rs) ----
  const commit = (vPub, nV) => sha256(cat(utf8('wl-commit'), vPub, nV));
  function sas(vPub, aPub, nV, nA) {
    const d = sha256(cat(utf8('wl-sas'), vPub, aPub, nV, nA));
    return (((d[0] << 24) >>> 0) + (d[1] << 16) + (d[2] << 8) + d[3]) % 1000000;
  }
  function formatCode(c) { const s = String(c).padStart(6, '0'); return s.slice(0, 3) + ' ' + s.slice(3); }
  function deviceKey(shared, vPub, aPub, nV, nA) {
    return hkdf(shared, sha256(cat(vPub, aPub, nV, nA)), utf8('wilive-device-v1'), 32);
  }
  const helloMac = (key, id, chal) => hmac(key, utf8('wl-hello|' + id + '|' + chal));
  const fpMac = (key, chal, fp) => hmac(key, utf8('wl-fp|' + chal + '|' + fp));

  return { sha256, hmac, hkdf, x25519, BASE, cat, hex, unhex, utf8, random, same,
           commit, sas, formatCode, deviceKey, helloMac, fpMac };
})();
if (typeof module !== 'undefined') module.exports = WLC;
