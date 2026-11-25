// web/crypto.js against the RFC vectors and the Rust side's pinned bytes.
// Run by tests/unit/test_crypto_js.py; prints one line per check and exits 1 on any failure.
'use strict';
const path = require('path');
const fs = require('fs');
const W = require(process.env.WLC_PATH || path.join(__dirname, '../../web/crypto.js'));
const V = JSON.parse(fs.readFileSync(path.join(__dirname, '../../vectors/pairing_v1.json'), 'utf8'));
const h = W.hex, u = W.unhex, t = W.utf8;
let bad = 0;
function eq(name, got, want) {
  const ok = got === want;
  if (!ok) bad++;
  console.log((ok ? 'ok   ' : 'FAIL ') + name + (ok ? '' : `\n     got  ${got}\n     want ${want}`));
}
const rep = (b, n) => new Uint8Array(n).fill(b);
// SHA-256 (FIPS 180-4 examples)
eq('sha256 ""', h(W.sha256(t(''))), 'e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855');
eq('sha256 abc', h(W.sha256(t('abc'))), 'ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad');
eq('sha256 two blocks', h(W.sha256(t('abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq'))), '248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1');
eq('sha256 55/56/64 bytes differ', new Set([55, 56, 63, 64, 65].map(n => h(W.sha256(rep(0x61, n))))).size, 5);
eq('sha256 a million a', h(W.sha256(rep(0x61, 1000000))), 'cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0');
// HMAC-SHA256 (RFC 4231)
eq('hmac case 1', h(W.hmac(rep(0x0b, 20), t('Hi There'))), 'b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7');
eq('hmac case 2', h(W.hmac(t('Jefe'), t('what do ya want for nothing?'))), '5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843');
eq('hmac case 6 (long key)', h(W.hmac(rep(0xaa, 131), t('Test Using Larger Than Block-Size Key - Hash Key First'))), '60e431591ee0b67f0d8a26aacbf5b77f8e0bc6213728c5140546040f0ee37f54');
// HKDF-SHA256 (RFC 5869 case 1)
eq('hkdf case 1', h(W.hkdf(rep(0x0b, 22), u('000102030405060708090a0b0c'), u('f0f1f2f3f4f5f6f7f8f9'), 42)),
   '3cb25f25faacd57a90434f64d0362f2a2d2d0a90cf1a5a4c5db02d56ecc4c5bf34007208d5b887185865');
// X25519 (RFC 7748 5.2 and 6.1)
eq('x25519 5.2 #1', h(W.x25519(u('a546e36bf0527c9d3b16154b82465edd62144c0ac1fc5a18506a2244ba449ac4'), u('e6db6867583030db3594c1a424b15f7c726624ec26b3353b10a903a6d0ab1c4c'))),
   'c3da55379de9c6908e94ea4df28d084f32eccf03491c71f754b4075577a28552');
eq('x25519 5.2 #2', h(W.x25519(u('4b66e9d4d1b4673c5ad22691957d6af5c11b6421e0ea01d42ca4169e7918ba0d'), u('e5210f12786811d3f4b7959d0538ae2c31dbe7106fc03c3efc4cd549c715a493'))),
   '95cbde9476e8907d7aade45cb4b873f88b595a68799fa152e6f8f7647aac7957');
const aPriv = u('77076d0a7318a57d3c16c17251b26645df4c2f87ebc0992ab177fba51db92c2a');
const bPriv = u('5dab087e624a8a4b79e17f8b83800ee66f3bb1292618b6fd1c2f8b27ff88e0eb');
const aPub = W.x25519(aPriv, W.BASE), bPub = W.x25519(bPriv, W.BASE);
eq('x25519 6.1 alice pub', h(aPub), '8520f0098930a754748b7ddcb43ef75a0dbf3a0d26381af4eba4a98eaa9b4e6a');
eq('x25519 6.1 bob pub', h(bPub), 'de9edb7d7b7dc1b4d35b61c2ece435373f8343c85b78674dadfc7e146f882b4f');
eq('x25519 6.1 shared (a)', h(W.x25519(aPriv, bPub)), '4a5d9d5ba4ce2de1728e3bf480350f25e07e21c947d19e3376f09b3c1e161742');
eq('x25519 6.1 shared (b)', h(W.x25519(bPriv, aPub)), '4a5d9d5ba4ce2de1728e3bf480350f25e07e21c947d19e3376f09b3c1e161742');
{ // RFC 7748 5.2: k = u = 9, then k, u = x25519(k, u), k
  let k = new Uint8Array(W.BASE), uu = new Uint8Array(W.BASE);
  for (let i = 1; i <= 1000; i++) { const r = W.x25519(k, uu); uu = k; k = r; if (i === 1) eq('x25519 iterated once', h(k), '422c8e7a6227d7bca1350b3e2bb7279f7897b87bb6854b783c60e80311ae3079'); }
  eq('x25519 iterated 1,000 times', h(k), '684cf59ba83309552800ef566f2f4d3c1c3887c49360e3875f2eb94d99532c51');
}
eq('x25519 low-order point gives zero', h(W.x25519(aPriv, new Uint8Array(32))), '00'.repeat(32));
// Wi-Live's pairing, byte for byte with wilive-core (vectors/pairing_v1.json)
const vPub = W.x25519(u(V.v_secret), W.BASE);
eq('vectors v_pub', h(vPub), V.v_pub);
eq('vectors a_pub', h(W.x25519(u(V.a_secret), W.BASE)), V.a_pub);
eq('vectors commit', h(W.commit(vPub, u(V.n_v))), V.commit);
const shared = W.x25519(u(V.v_secret), u(V.a_pub));
eq('vectors shared', h(shared), V.shared);
eq('vectors code', W.sas(vPub, u(V.a_pub), u(V.n_v), u(V.n_a)), V.code);
eq('vectors code text', W.formatCode(V.code), V.code_text);
eq('code text pads', W.formatCode(42), '000 042');
const key = W.deviceKey(shared, vPub, u(V.a_pub), u(V.n_v), u(V.n_a));
eq('vectors key', h(key), V.key);
eq('vectors hello mac', h(W.helloMac(key, V.id, V.chal)), V.hello_mac);
eq('vectors fp mac', h(W.fpMac(key, V.chal, V.fp)), V.fp_mac);
eq('same() equal', W.same(u('0102'), u('0102')), true);
eq('same() differs', W.same(u('0102'), u('0103')), false);
eq('same() length', W.same(u('01'), u('0102')), false);
let threw = false; try { W.unhex('0g'); } catch (e) { threw = true; } eq('unhex refuses junk', threw, true);
const t0 = Date.now(); for (let i = 0; i < 5; i++) W.x25519(W.random(32), W.BASE);
const per = (Date.now() - t0) / 5;
console.log(`info x25519 ${per.toFixed(1)} ms`);
eq('x25519 under 150 ms (a phone is ~5x slower than this machine)', per < 150, true);
process.exit(bad ? 1 : 0);
