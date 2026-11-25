// Wi-Live dashboard: the viewer (pair, watch) and the owner's window, one file,
// no framework. Every string from the network goes in as textContent, never HTML.
'use strict';
(() => {
  const $ = id => document.getElementById(id);
  const OWNER = typeof window.WILIVE_OWNER === 'string' ? window.WILIVE_OWNER : '';
  const PROBE = /[?&]probe\b/.test(location.search);
  const DEV_KEY = 'wilive.device.v1';
  const T = window.__wl = { state: 'boot', lat: null, events: [] };   // read by the tests
  const note = (s) => { T.state = s; T.events.push(s); if (T.events.length > 200) T.events.shift(); };

  function el(tag, props, ...kids) {
    const e = document.createElement(tag);
    for (const [k, v] of Object.entries(props || {})) {
      if (k === 'class') e.className = v;
      else if (k === 'text') e.textContent = v;
      else if (k.startsWith('on')) e.addEventListener(k.slice(2), v);
      else e.setAttribute(k, v);
    }
    for (const k of kids) if (k != null) e.append(k);
    return e;
  }
  function icon(name) {
    const s = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
    const u = document.createElementNS('http://www.w3.org/2000/svg', 'use');
    u.setAttribute('href', '#i-' + name); s.append(u); return s;
  }
  async function api(path, body, timeout) {
    const ctl = new AbortController();
    const timer = setTimeout(() => ctl.abort(), timeout || 6000);
    const headers = {};
    if (body !== undefined) headers['Content-Type'] = 'application/json';
    if (OWNER) headers['X-Wilive-Owner'] = OWNER;
    try {
      const r = await fetch(path, { method: body === undefined ? 'GET' : 'POST', headers, body: body === undefined ? undefined : JSON.stringify(body), signal: ctl.signal, cache: 'no-store' });
      let j = {}; try { j = await r.json(); } catch (e) {}
      if (!r.ok) { const err = new Error(j.error || ('http ' + r.status)); err.status = r.status; throw err; }
      return j;
    } catch (e) {
      if (e.name === 'AbortError') { const err = new Error('timeout'); err.status = 0; throw err; }
      if (e.status === undefined) e.status = 0;
      throw e;
    } finally { clearTimeout(timer); }
  }
  const store = {
    get() { try { const v = JSON.parse(localStorage.getItem(DEV_KEY) || 'null'); return v && /^[0-9a-f]{32}$/.test(v.id) && /^[0-9a-f]{64}$/.test(v.key) ? v : null; } catch (e) { return store.mem; } },
    set(v) { store.mem = v; try { localStorage.setItem(DEV_KEY, JSON.stringify(v)); } catch (e) {} },
    clear() { store.mem = null; try { localStorage.removeItem(DEV_KEY); } catch (e) {} },
    mem: null,
  };
  function show(id) { for (const v of ['vPair', 'vWatch', 'vOwner']) $(v).hidden = v !== id; document.body.classList.toggle('watching', id === 'vWatch'); }
  function setName(n) { for (const e of document.querySelectorAll('.pcname')) e.textContent = n; document.title = n ? n + ' - Wi-Live' : 'Wi-Live'; }
  function guessName() {
    const ua = navigator.userAgent;
    const os = /iPhone/.test(ua) ? 'iPhone' : /iPad/.test(ua) ? 'iPad' : /Android/.test(ua) ? 'Android' : /Mac/.test(ua) ? 'Mac' : /Windows/.test(ua) ? 'Windows PC' : /CrOS/.test(ua) ? 'Chromebook' : /Linux/.test(ua) ? 'Linux PC' : 'device';
    return 'My ' + os;
  }

  // ================= viewer: pairing =================
  let info = { name: '' };
  let pairing = null;
  function pairErr(t) { $('pairErr').textContent = t; $('pairErr').hidden = !t; }
  function startPairUI() {
    show('vPair'); note('pair');
    $('pairAsk').hidden = false; $('pairCode').hidden = true; pairErr('');
    if (!$('devName').value) $('devName').value = guessName();
  }
  async function ask() {
    const W = WLC;
    pairErr('');
    $('btnAsk').disabled = true;
    try {
      const secret = W.random(32), vPub = W.x25519(secret, W.BASE), nV = W.random(16);
      const s = await api('/api/pair/start', { name: $('devName').value.trim() || guessName(), v_pub: W.hex(vPub), commit: W.hex(W.commit(vPub, nV)) });
      const aPub = W.unhex(s.a_pub), nA = W.unhex(s.n_a);
      if (aPub.length !== 32 || nA.length !== 16) throw new Error('reply');
      await api('/api/pair/reveal', { req: s.req, n_v: W.hex(nV) });
      const shared = W.x25519(secret, aPub);
      if (shared.every(b => b === 0)) throw new Error('weak');
      const key = W.deviceKey(shared, vPub, aPub, nV, nA);
      const code = W.sas(vPub, aPub, nV, nA);
      pairing = { req: s.req, key: W.hex(key), alive: true };
      $('codeTx').textContent = W.formatCode(code);
      $('pairAsk').hidden = true; $('pairCode').hidden = false;
      $('pairWait').textContent = 'Waiting for the computer...';
      note('code');
      pollPair(pairing);
    } catch (e) {
      pairErr(e.message === 'busy' ? 'Too many devices are asking right now. Try again in a minute.'
        : e.message === 'paused' ? 'Sharing is paused on this computer. Ask the person at it to turn it on.'
        : e.status === 0 ? 'The computer did not answer. Check this device is on the same network.'
        : 'That did not work (' + e.message + '). Try again.');
    } finally { $('btnAsk').disabled = false; }
  }
  async function pollPair(p) {
    const started = Date.now();
    while (p.alive) {
      await sleep(700);
      if (!p.alive) return;
      let r;
      try { r = await api('/api/pair/status?req=' + encodeURIComponent(p.req)); }
      catch (e) {
        if (e.status === 404) { p.alive = false; startPairUI(); pairErr('The request expired. Ask again.'); return; }
        if (Date.now() - started > 130000) { p.alive = false; startPairUI(); pairErr('The request expired. Ask again.'); return; }
        continue;
      }
      if (r.state === 'allowed' && /^[0-9a-f]{32}$/.test(r.device || '')) {
        p.alive = false;
        store.set({ id: r.device, key: p.key });
        note('allowed');
        watch();
        return;
      }
      if (r.state === 'denied') { p.alive = false; note('denied'); startPairUI(); pairErr('The computer said no.'); return; }
    }
  }
  const sleep = ms => new Promise(r => setTimeout(r, ms));

  // ================= viewer: watching =================
  let session = null, retry = 0, retryTimer = 0;
  function overlay(t, again) { $('overlayMsg').textContent = t || ''; $('overlay').hidden = !t; $('btnAgain').hidden = !again; }
  const REASONS = {
    paused: () => 'Sharing is paused on ' + info.name + '. It starts again when they turn it on.',
    busy: () => 'Too many people are watching ' + info.name + ' right now.',
    permission: () => info.name + ' has not allowed screen recording for Wi-Live yet.',
    capture: () => info.name + ' could not capture its screen. Trying again...',
    ended: () => 'The person at ' + info.name + ' ended your view.',
    fingerprint: () => 'This does not look like ' + info.name + '. Stopped for your safety.',
    blocked: () => 'Too many wrong tries came from this device. Try again in 10 minutes.',
  };
  function watch() {
    const dev = store.get();
    if (!dev) return startPairUI();
    show('vWatch'); note('connecting');
    overlay('Connecting to ' + info.name + '...');
    clearTimeout(retryTimer);
    if (session) session.stop('restart');
    session = connect(dev);
  }
  function again(reason) {
    const wait = Math.min(8000, 500 * Math.pow(2, retry++));
    const why = REASONS[reason] ? REASONS[reason]() : 'Connection lost. Reconnecting...';
    overlay(why);
    clearTimeout(retryTimer);
    retryTimer = setTimeout(() => { if (!document.hidden) watch(); else again(reason); }, reason === 'paused' ? 5000 : reason === 'blocked' ? 60000 : wait);
  }
  function fpOf(sdp) { const m = /a=fingerprint:sha-256 ([0-9A-Fa-f:]+)/.exec(sdp); return m ? m[1].replace(/:/g, '').toLowerCase() : ''; }

  function connect(dev) {
    const W = WLC, key = W.unhex(dev.key);
    let ws, pc, done = false, live = false, chal = '', fp = '';
    const video = $('video');
    const statsTimer = setInterval(() => stats(pc), 1000);
    const end = (reason, retryIt) => {
      if (done) return; done = true;
      clearInterval(statsTimer);
      try { ws && ws.readyState === 1 && ws.send(JSON.stringify({ t: 'bye' })); } catch (e) {}
      try { ws && ws.close(); } catch (e) {}
      try { pc && pc.close(); } catch (e) {}
      $('liveChip').hidden = true;
      note('end:' + reason);
      if (reason === 'device' || reason === 'forgotten') { store.clear(); startPairUI(); pairErr(reason === 'forgotten' ? 'This device was removed on the computer. Ask again to watch.' : 'This device is no longer allowed. Ask again to watch.'); return; }
      if (reason === 'fingerprint') { overlay(REASONS.fingerprint(), true); return; }
      // Ended by the owner: never back on its own, or "End" would mean nothing.
      if (reason === 'ended') { session = null; overlay(REASONS.ended(), true); return; }
      if (retryIt) again(reason);
    };
    const opened = setTimeout(() => { if (!live) end('timeout', true); }, 12000);
    try { ws = new WebSocket((location.protocol === 'https:' ? 'wss://' : 'ws://') + location.host + '/v1/signal'); }
    catch (e) { end('unreachable', true); return { stop() {} }; }
    ws.onclose = () => { clearTimeout(opened); end(live ? 'closed' : 'unreachable', true); };
    ws.onmessage = async ev => {
      let m; try { m = JSON.parse(ev.data); } catch (e) { return; }
      if (m.t === 'chal') { chal = String(m.c || ''); ws.send(JSON.stringify({ t: 'hello', id: dev.id, mac: W.hex(W.helloMac(key, dev.id, chal)) })); return; }
      if (m.t === 'error') return end(m.reason || 'error', !['device', 'fingerprint'].includes(m.reason));
      if (m.t === 'end') return end(m.reason || 'ended', !['forgotten', 'ended'].includes(m.reason));
      if (m.t === 'stats') { T.fps = m.fps | 0; return; }
      if (m.t === 'displays') return displays(m.displays || [], m.display);
      if (m.t === 'display') { $('displaySel').value = String(m.id); return; }
      if (m.t === 'ready') {
        // The fingerprint comes with a proof made with this device's key: a page
        // or reply swapped on the way cannot pass off another computer's media.
        let proof; try { proof = W.unhex(m.fp_mac || ''); } catch (e) { proof = new Uint8Array(0); }
        if (!/^[0-9a-f]{64}$/.test(m.fp || '') || !W.same(proof, W.fpMac(key, chal, m.fp))) return end('fingerprint', false);
        fp = m.fp;
        if (m.name) { info.name = m.name; setName(m.name); }
        displays(m.displays || [], m.display);
        pc = new RTCPeerConnection({ iceServers: [] });
        const tr = pc.addTransceiver('video', { direction: 'recvonly' });
        if (m.audio && (m.audio.system === 'on' || m.audio.mic === 'on')) pc.addTransceiver('audio', { direction: 'recvonly' });
        try {
          const caps = RTCRtpReceiver.getCapabilities && RTCRtpReceiver.getCapabilities('video');
          if (caps && tr.setCodecPreferences) {
            const h = caps.codecs.filter(c => /h264/i.test(c.mimeType));
            if (h.length) tr.setCodecPreferences(h.concat(caps.codecs.filter(c => !/h264/i.test(c.mimeType))));
          }
        } catch (e) {}
        const ms = new MediaStream();
        pc.ontrack = e => {
          // As little buffering as the network allows, for picture and sound.
          try { if ('jitterBufferTarget' in e.receiver) e.receiver.jitterBufferTarget = 0; else e.receiver.playoutDelayHint = 0; } catch (x) {}
          ms.addTrack(e.track);
          if (e.track.kind === 'audio') $('btnSound').hidden = false;
          if (video.srcObject !== ms) video.srcObject = ms;
          video.play().catch(() => {});
        };
        pc.onconnectionstatechange = () => {
          const s = pc.connectionState;
          if (s === 'connected') { live = true; retry = 0; clearTimeout(opened); overlay(''); $('liveChip').hidden = false; note('live'); }
          else if (s === 'disconnected') overlay('Connection unsteady...');
          else if (s === 'failed') end('network', true);
        };
        const offer = await pc.createOffer();
        await pc.setLocalDescription(offer);
        ws.send(JSON.stringify({ t: 'offer', sdp: pc.localDescription.sdp }));
        return;
      }
      if (m.t === 'answer') {
        if (!fp || fpOf(m.sdp) !== fp) return end('fingerprint', false);
        try { await pc.setRemoteDescription({ type: 'answer', sdp: m.sdp }); } catch (e) { end('answer', true); }
      }
    };
    const send = o => { try { ws && ws.readyState === 1 && ws.send(JSON.stringify(o)); } catch (e) {} };
    T.send = send;
    return { stop(r) { end(r || 'stopped', false); }, send };
  }

  function displays(list, current) {
    const sel = $('displaySel');
    sel.textContent = '';
    for (const d of list) sel.append(el('option', { value: String(d.id), text: d.name || ('Display ' + d.id) + (d.width ? ' (' + d.width + 'x' + d.height + ')' : '') }));
    if (current != null) sel.value = String(current);
    const d = list.find(x => x.id === current) || list[0];
    T.srcW = d && d.width ? d.width : 0;
    sel.hidden = list.length < 2;
  }

  // ----- stats: what the HUD shows and what the owner sees for this viewer -----
  let last = null;
  async function stats(pc) {
    if (!pc) return;
    let rep; try { rep = await pc.getStats(); } catch (e) { return; }
    let v = null, pair = null;
    rep.forEach(s => { if (s.type === 'inbound-rtp' && s.kind === 'video') v = s; if (s.type === 'candidate-pair' && s.nominated && s.state === 'succeeded') pair = s; });
    if (!v) return;
    const now = performance.now();
    const r = { t: now, bytes: v.bytesReceived || 0, frames: v.framesDecoded || 0, jb: v.jitterBufferDelay || 0, jbn: v.jitterBufferEmittedCount || 0, dec: v.totalDecodeTime || 0 };
    if (last) {
      const dt = (r.t - last.t) / 1000, df = r.frames - last.frames, dn = r.jbn - last.jbn;
      const rtt = pair && pair.currentRoundTripTime ? pair.currentRoundTripTime * 1000 : 0;
      const jbMs = dn > 0 ? (r.jb - last.jb) / dn * 1000 : 0;
      const decMs = df > 0 ? (r.dec - last.dec) / df * 1000 : 0;
      const est = Math.round(rtt / 2 + jbMs + decMs);
      T.stats = { fps: Math.round(df / dt), kbps: Math.round((r.bytes - last.bytes) * 8 / dt / 1000), w: v.frameWidth || 0, h: v.frameHeight || 0, rtt: Math.round(rtt), jb: Math.round(jbMs), dec: +decMs.toFixed(1), est };
      if (!PROBE) session && session.send({ t: 'lat', p50: est, p99: est });
      if (v.frameWidth && (!last.w || last.w !== v.frameWidth || last.h !== v.frameHeight)) session && session.send({ t: 'size', w: v.frameWidth, h: v.frameHeight });
      r.w = v.frameWidth; r.h = v.frameHeight;
      hud();
    }
    last = r;
  }
  function hud() {
    const s = T.stats; if (!s || $('hud').hidden) return;
    let t = s.fps + ' fps  ' + s.w + 'x' + s.h + '  ' + (s.kbps / 1000).toFixed(1) + ' Mb/s  ~' + s.est + ' ms';
    if (T.lat) t += '  p50 ' + T.lat.p50 + ' p99 ' + T.lat.p99;
    $('hud').textContent = t;
  }

  // ----- the latency probe (tests/latency): reads the time strip off each frame -----
  function probe() {
    const video = $('video');
    if (!video.requestVideoFrameCallback) return;
    const BITS = 32, BLOCK = 16, cv = document.createElement('canvas');
    cv.width = (BITS + 2) * BLOCK; cv.height = BLOCK;
    const cx = cv.getContext('2d', { willReadFrequently: true });
    const samples = [], parts = [];
    let bad = 0;
    const onFrame = (now, meta) => {
      // The strip is drawn at the display's size; the stream may be smaller.
      const sx = T.srcW ? meta.width / T.srcW : 1;
      try {
        cx.drawImage(video, 0, 0, cv.width * sx, BLOCK * sx, 0, 0, cv.width, BLOCK);
        const px = cx.getImageData(0, 0, cv.width, BLOCK).data;
        const at = c => px[((BLOCK >> 1) * cv.width + c * BLOCK + (BLOCK >> 1)) * 4] > 128;
        if (at(0) && !at(BITS + 1)) {
          let v = 0; for (let c = 1; c <= BITS; c++) v = (v * 2) + (at(c) ? 1 : 0);
          const shown = (performance.timeOrigin + meta.expectedDisplayTime) % 4294967296;
          let d = shown - v; if (d < -2147483648) d += 4294967296;
          if (d > -50 && d < 10000) {
            samples.push(d);
            // Where the time went: to the last packet arriving, decoding, then display.
            if (meta.receiveTime) {
              const rx = ((performance.timeOrigin + meta.receiveTime) % 4294967296) - v;
              parts.push([Math.round(rx < -2147483648 ? rx + 4294967296 : rx), Math.round((meta.processingDuration || 0) * 1000),
                          Math.round(meta.expectedDisplayTime - meta.receiveTime - (meta.processingDuration || 0) * 1000)]);
              if (parts.length > 3000) parts.splice(0, parts.length - 3000);
            }
          } else bad++;
        } else bad++;
      } catch (e) { bad++; }
      if (samples.length > 3000) samples.splice(0, samples.length - 3000);
      video.requestVideoFrameCallback(onFrame);
    };
    video.requestVideoFrameCallback(onFrame);
    T.probe = { samples, parts, bad: () => bad, reset() { samples.length = 0; parts.length = 0; bad = 0; } };
    setInterval(() => {
      if (samples.length < 10) return;
      const s = samples.slice().sort((a, b) => a - b), q = p => Math.round(s[Math.min(s.length - 1, Math.floor(p * s.length))]);
      T.lat = { n: s.length, p50: q(.5), p95: q(.95), p99: q(.99), max: Math.round(s[s.length - 1]) };
      session && session.send({ t: 'lat', p50: T.lat.p50, p99: T.lat.p99 });
      hud();
    }, 1000);
  }

  // ----- the bar: fades while watching, back on any touch or move -----
  let fadeTimer = 0;
  function wake() { $('bar').classList.remove('faded'); clearTimeout(fadeTimer); fadeTimer = setTimeout(() => { if (!$('overlay').hidden) return; $('bar').classList.add('faded'); }, 3000); }
  function wireWatch() {
    for (const e of ['pointermove', 'pointerdown', 'keydown']) document.addEventListener(e, () => { if (!$('vWatch').hidden) wake(); }, { passive: true });
    const fits = ['fit', 'fill', 'actual'], names = { fit: 'Fit', fill: 'Fill', actual: '1:1' };
    $('btnFit').addEventListener('click', () => { const s = $('stage'); const n = fits[(fits.indexOf(s.dataset.fit) + 1) % fits.length]; s.dataset.fit = n; $('fitTx').textContent = names[n]; });
    $('btnFull').addEventListener('click', () => {
      const d = document, v = $('video');
      if (d.fullscreenElement) return d.exitFullscreen().catch(() => {});
      if (d.documentElement.requestFullscreen) d.documentElement.requestFullscreen().catch(() => {});
      else if (v.webkitEnterFullscreen) v.webkitEnterFullscreen();
    });
    $('btnSound').addEventListener('click', () => {
      const v = $('video'); v.muted = !v.muted; v.play().catch(() => {});
      $('btnSound').setAttribute('aria-pressed', String(!v.muted));
      $('btnSound').querySelector('use').setAttribute('href', v.muted ? '#i-mute' : '#i-sound');
    });
    $('btnStats').addEventListener('click', () => { const h = $('hud'); h.hidden = !h.hidden; $('btnStats').setAttribute('aria-pressed', String(!h.hidden)); hud(); });
    $('btnAgain').addEventListener('click', () => { retry = 0; watch(); });
    $('displaySel').addEventListener('change', e => session && session.send({ t: 'display', id: Number(e.target.value) }));
    document.addEventListener('visibilitychange', () => { if (!document.hidden && !$('vWatch').hidden && !session) watch(); });
  }

  // ================= owner: this computer's window =================
  let ownerBusy = false, armed = '';
  function ago(unix) {
    const s = Math.max(0, Math.round(Date.now() / 1000 - unix));
    return s < 60 ? 'just now' : s < 3600 ? Math.floor(s / 60) + ' min ago' : s < 86400 ? Math.floor(s / 3600) + ' h ago' : Math.floor(s / 86400) + ' d ago';
  }
  function dur(unix) { const s = Math.max(0, Math.round(Date.now() / 1000 - unix)); return Math.floor(s / 60) + 'm ' + String(s % 60).padStart(2, '0') + 's'; }
  function clock(unix) { const d = new Date(unix * 1000); return d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' }); }
  async function act(path, body) {
    try { await api(path, body); } catch (e) { note('owner-error:' + e.message); }
    refresh();
  }
  function twoStep(id, label, sure, fn) {
    // A second tap confirms: no dialog, which a web view may not show.
    return el('button', { class: 'btn small', type: 'button', 'data-act': id, text: armed === id ? sure : label, onclick: () => { if (armed === id) { armed = ''; fn(); } else { armed = id; redraw(); setTimeout(() => { if (armed === id) { armed = ''; redraw(); } }, 4000); } } });
  }
  const LOG_WORDS = { pair_asked: 'asked to watch', pair_allowed: 'was allowed', pair_denied: 'was refused', watching: 'started watching', ended: 'stopped watching', refused: 'was refused', forgotten: 'was removed', paused: 'Sharing paused', resumed: 'Sharing resumed' };
  function render(s) {
    T.owner = s;
    setName(s.name);
    const n = s.live.length;
    $('oLive').className = 'chip' + (n ? ' live' : '');
    $('oLiveTx').textContent = n ? (n === 1 ? '1 watching' : n + ' watching') : 'Nobody watching';
    $('oPause').setAttribute('aria-checked', String(!s.paused));
    $('oPause').setAttribute('aria-label', s.paused ? 'Sharing paused' : 'Sharing on');
    $('oPauseTx').textContent = s.paused ? 'Paused' : 'Sharing';
    $('oPausedMsg').hidden = !s.paused;
    // requests
    $('oAsks').hidden = !s.pending.length;
    $('oAskList').replaceChildren(...s.pending.map(p => el('div', { class: 'ask', 'data-req': p.req },
      el('div', { class: 'row' }, el('div', { class: 'grow' }, el('div', { class: 'title', text: p.name }), el('div', { class: 'sub', text: p.agent + ' at ' + p.addr }))),
      el('p', { class: 'note', text: 'Allow only if this device shows the same code:' }),
      el('div', { class: 'code', text: p.code }),
      el('div', { class: 'row mt16' },
        el('button', { class: 'btn primary', type: 'button', 'data-act': 'allow', text: 'Allow', onclick: () => act('/api/owner/allow', { req: p.req }) }),
        el('button', { class: 'btn', type: 'button', 'data-act': 'deny', text: 'Deny', onclick: () => act('/api/owner/deny', { req: p.req }) })))));
    // watching
    $('oLiveList').replaceChildren(...(n ? s.live.map(l => el('li', { class: 'item', 'data-live': l.id },
      el('div', { class: 'grow' },
        el('div', { class: 'title', text: l.name }),
        el('div', { class: 'sub', text: l.agent + ' at ' + l.addr + ' for ' + dur(l.since) }),
        el('div', { class: 'stats' },
          stat(l.fps, 'fps'), stat(l.width ? l.width + 'x' + l.height : '-', 'picture'),
          stat(l.lat_p50 ? l.lat_p50 + ' ms' : '-', 'delay'), stat(l.sound ? 'On' : 'Off', 'sound'))),
      el('button', { class: 'btn small', type: 'button', 'data-act': 'end', text: 'End', onclick: () => act('/api/owner/end', { id: l.id }) })))
      : [el('li', { class: 'empty', text: 'Nobody is watching this computer.' })]));
    // where to open
    $('oUrls').replaceChildren(...(s.urls.length ? s.urls.map(u => el('div', {}, el('a', { class: 'url', href: u, text: u, target: '_blank', rel: 'noopener' })))
      : [el('div', { class: 'msg', text: 'This computer is not on a home network right now.' })]));
    // devices
    $('oDevList').replaceChildren(...(s.devices.length ? s.devices.map(d => el('li', { class: 'item', 'data-dev': d.id },
      el('div', { class: 'grow' }, el('div', { class: 'title', text: d.name }), el('div', { class: 'sub', text: d.agent + ', last seen ' + ago(d.last_seen) + (d.last_addr ? ' at ' + d.last_addr : '') })),
      twoStep('forget:' + d.id, 'Remove', 'Remove?', () => act('/api/owner/forget', { id: d.id }))))
      : [el('li', { class: 'empty', text: 'No devices yet. Open the address above on a phone to add one.' })]));
    // log
    $('oLog').replaceChildren(...(s.log.length ? s.log.slice(0, 40).map(e => el('li', { class: 'item' },
      el('time', { text: clock(e.at) }),
      el('div', { class: 'grow' }, el('div', { class: 'sub', text: [e.who, LOG_WORDS[e.kind] || e.kind, e.addr ? 'from ' + e.addr : '', e.detail].filter(Boolean).join(' ') }))))
      : [el('li', { class: 'empty', text: 'Nothing yet.' })]));
  }
  // Redraws at once from the last state (a poll in flight must not delay a tap's answer).
  function redraw() { if (T.owner) render(T.owner); }
  function stat(v, label) { return el('div', { class: 'stat' }, el('b', { text: String(v) }), el('small', { text: label })); }
  async function refresh() {
    if (ownerBusy) return;
    ownerBusy = true;
    try { render(await api('/api/owner/state')); note('owner'); }
    catch (e) { note('owner-error:' + e.message); }
    finally { ownerBusy = false; }
  }
  function owner() {
    show('vOwner');
    $('oPause').addEventListener('click', () => act('/api/owner/pause', { on: $('oPause').getAttribute('aria-checked') === 'true' }));
    refresh();
    setInterval(refresh, 1000);
  }

  // ================= boot =================
  async function boot() {
    if (OWNER) return owner();
    $('btnAsk').addEventListener('click', ask);
    $('devName').addEventListener('keydown', e => { if (e.key === 'Enter') ask(); });
    $('btnCancel').addEventListener('click', () => { if (pairing) pairing.alive = false; startPairUI(); });
    wireWatch();
    if (PROBE) probe();
    try { info = await api('/api/info'); setName(info.name); }
    catch (e) { show('vPair'); pairErr('The computer did not answer. Reload to try again.'); return; }
    if (store.get()) watch(); else startPairUI();
  }
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', boot); else boot();
})();
