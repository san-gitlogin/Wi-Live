// Wi-Live dashboard: the viewer (pair, watch) and the owner's window, one file,
// no framework. Every string from the network goes in as textContent, never HTML.
'use strict';
(() => {
  // ---- the floor: Safari 12.1 (iOS 12.2), Chrome 70, Firefox 68 (tests/static: legacy floor) ----
  if (!Element.prototype.replaceChildren) {
    Element.prototype.replaceChildren = function () { while (this.firstChild) this.removeChild(this.firstChild); this.append.apply(this, arguments); };
  }
  // Flex gap arrived in Safari 14.1: without it, the CSS spaces with margins.
  (() => { const f = document.createElement('div'); f.style.display = 'flex'; f.style.flexDirection = 'column'; f.style.rowGap = '1px';
    f.appendChild(document.createElement('div')); f.appendChild(document.createElement('div'));
    document.documentElement.appendChild(f); if (f.scrollHeight !== 1) document.documentElement.classList.add('no-flex-gap'); f.remove(); })();
  const IOS = /iPhone|iPad|iPod/.test(navigator.userAgent) || (navigator.platform === 'MacIntel' && navigator.maxTouchPoints > 1);
  const STANDALONE = navigator.standalone === true || (window.matchMedia && matchMedia('(display-mode: standalone), (display-mode: fullscreen)').matches);
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
    $('iosHint').hidden = !IOS || STANDALONE;
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
  // A view is one display in one <video>, over its own signaling session. One view
  // fills the stage; "All displays" makes a wall of them, one per display, arranged
  // as the viewer likes (layout and order kept per device).
  const WALL_KEY = 'wilive.wall.v1';
  let main = null, wall = [], retry = 0, retryTimer = 0, displayList = [], wallOn = false;
  const views = () => (wallOn ? wall : main ? [main] : []);
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
  function stopAll(r) { for (const v of wall) v.stop(r); wall = []; if (main) main.stop(r); main = null; }
  function watch() {
    const dev = store.get();
    if (!dev) return startPairUI();
    show('vWatch'); note('connecting');
    overlay('Connecting to ' + info.name + '...');
    clearTimeout(retryTimer);
    stopAll('restart');
    if (wallOn && displayList.length > 1) return buildWall(dev);
    wallOn = false; $('wall').hidden = true; $('stage').hidden = false;
    main = connect(dev, { video: $('video'), primary: true, display: T.wantDisplay });
  }
  function again(reason) {
    const wait = Math.min(8000, 500 * Math.pow(2, retry++));
    overlay(REASONS[reason] ? REASONS[reason]() : 'Connection lost. Reconnecting...');
    clearTimeout(retryTimer);
    retryTimer = setTimeout(() => { if (!document.hidden) watch(); else again(reason); }, reason === 'paused' ? 5000 : reason === 'blocked' ? 60000 : wait);
  }
  function fpOf(sdp) { const m = /a=fingerprint:sha-256 ([0-9A-Fa-f:]+)/.exec(sdp); return m ? m[1].replace(/:/g, '').toLowerCase() : ''; }

  // A live stream must never stay paused: Safari pauses on leaving its player,
  // picture-in-picture and background tabs; this undoes every one of them.
  function keepPlaying(video) {
    if (video && video.srcObject && video.paused) video.play().catch(() => {});
  }
  function wireVideo(video) {
    video.addEventListener('pause', () => setTimeout(() => keepPlaying(video), 0));
    video.addEventListener('webkitendfullscreen', () => keepPlaying(video));
    video.addEventListener('leavepictureinpicture', () => keepPlaying(video));
    video.addEventListener('webkitpresentationmodechanged', () => keepPlaying(video));
  }

  function connect(dev, o) {
    const W = WLC, key = W.unhex(dev.key), video = o.video, primary = !!o.primary;
    let ws, pc, done = false, live = false, chal = '', fp = '';
    const me = { video, display: o.display, live: false, state: 'connecting', shown: performance.now(), stats: null, last: null, stuck: 0 };
    const say = s => { me.state = s; if (primary) note(s); if (o.onState) o.onState(s); };
    // Every presented frame, for the watchdog.
    const onFrame = () => { me.shown = performance.now(); me.stuck = 0; if (!done && video.requestVideoFrameCallback) video.requestVideoFrameCallback(onFrame); };
    if (video.requestVideoFrameCallback) video.requestVideoFrameCallback(onFrame);
    const statsTimer = setInterval(() => tick(me, pc, send), 1000);
    const end = (reason, retryIt) => {
      if (done) return; done = true;
      clearInterval(statsTimer);
      try { ws && ws.readyState === 1 && ws.send(JSON.stringify({ t: 'bye' })); } catch (e) {}
      try { ws && ws.close(); } catch (e) {}
      try { pc && pc.close(); } catch (e) {}
      // Let go of the stream: no stale last frame, no decoder kept for nothing.
      if (video.srcObject) { video.srcObject = null; }
      me.live = false;
      say('end:' + reason);
      if (!primary) { if (retryIt && !['restart', 'stopped'].includes(reason)) me.retry = setTimeout(() => o.restart && o.restart(), 1500); return; }
      $('liveChip').hidden = true;
      if (reason === 'restart' || reason === 'stopped') return;
      if (reason === 'device' || reason === 'forgotten') { stopAll('stopped'); store.clear(); startPairUI(); pairErr(reason === 'forgotten' ? 'This device was removed on the computer. Ask again to watch.' : 'This device is no longer allowed. Ask again to watch.'); return; }
      if (reason === 'fingerprint') { stopAll('stopped'); overlay(REASONS.fingerprint(), true); return; }
      // Ended by the owner: never back on its own, or "End" would mean nothing.
      if (reason === 'ended') { stopAll('stopped'); overlay(REASONS.ended(), true); return; }
      if (retryIt) again(reason);
    };
    const opened = setTimeout(() => { if (!live) end('timeout', true); }, 12000);
    try { ws = new WebSocket((location.protocol === 'https:' ? 'wss://' : 'ws://') + location.host + '/v1/signal'); }
    catch (e) { end('unreachable', true); return { stop() {}, send() {} }; }
    ws.onclose = () => { clearTimeout(opened); end(live ? 'closed' : 'unreachable', true); };
    ws.onmessage = async ev => {
      let m; try { m = JSON.parse(ev.data); } catch (e) { return; }
      if (m.t === 'chal') { chal = String(m.c || ''); ws.send(JSON.stringify({ t: 'hello', id: dev.id, mac: W.hex(W.helloMac(key, dev.id, chal)) })); return; }
      if (m.t === 'error') return end(m.reason || 'error', !['device', 'fingerprint'].includes(m.reason));
      if (m.t === 'end') return end(m.reason || 'ended', !['forgotten', 'ended'].includes(m.reason));
      if (m.t === 'stats') { me.fps = m.fps | 0; if (primary) T.fps = me.fps; return; }
      if (m.t === 'displays') { if (primary) displays(m.displays || [], m.display); return; }
      if (m.t === 'display') { me.display = m.id; if (primary && !wallOn) $('displaySel').value = String(m.id); return; }
      if (m.t === 'ready') {
        // The fingerprint comes with a proof made with this device's key: a page
        // or reply swapped on the way cannot pass off another computer's media.
        let proof; try { proof = W.unhex(m.fp_mac || ''); } catch (e) { proof = new Uint8Array(0); }
        if (!/^[0-9a-f]{64}$/.test(m.fp || '') || !W.same(proof, W.fpMac(key, chal, m.fp))) return end('fingerprint', false);
        fp = m.fp;
        if (m.name) { info.name = m.name; setName(m.name); }
        if (primary) displays(m.displays || [], o.display != null ? o.display : m.display);
        if (o.display != null && o.display !== m.display) ws.send(JSON.stringify({ t: 'display', id: o.display }));
        pc = new RTCPeerConnection({ iceServers: [] });
        const tr = pc.addTransceiver('video', { direction: 'recvonly' });
        // Sound once per device: only the first view asks for it.
        if (primary && m.audio && (m.audio.system === 'on' || m.audio.mic === 'on')) pc.addTransceiver('audio', { direction: 'recvonly' });
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
          keepPlaying(video);
        };
        const onState = s => {
          if (s === 'connected') { live = true; me.live = true; if (primary) { retry = 0; overlay(''); $('liveChip').hidden = false; wake(); } clearTimeout(opened); say('live'); }
          else if (s === 'disconnected') { if (primary) overlay('Connection unsteady...'); }
          else if (s === 'failed') end('network', true);
        };
        pc.onconnectionstatechange = () => onState(pc.connectionState);
        // Browsers without connectionState (Firefox before 113) report ICE alone.
        pc.oniceconnectionstatechange = () => { if (!('connectionState' in pc)) onState({ completed: 'connected', checking: 'connecting' }[pc.iceConnectionState] || pc.iceConnectionState); };
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
    const send = o2 => { try { ws && ws.readyState === 1 && ws.send(JSON.stringify(o2)); } catch (e) {} };
    me.stop = r => end(r || 'stopped', false);
    me.send = send;
    me.heal = () => { if (!done) { end('restart', false); if (o.restart) o.restart(); } };
    if (primary) T.send = send;
    return me;
  }

  function displays(list, current) {
    displayList = list;
    const sel = $('displaySel');
    sel.textContent = '';
    for (const d of list) sel.append(el('option', { value: String(d.id), text: (d.name || 'Display ' + d.id) + (d.width ? ' (' + d.width + 'x' + d.height + ')' : '') }));
    if (current != null) sel.value = String(current);
    const d = list.find(x => x.id === current) || list[0];
    T.srcW = d && d.width ? d.width : 0;
    sel.hidden = list.length < 2 || wallOn;
    $('btnWall').hidden = list.length < 2;
  }

  // ----- stats and the watchdog, once a second per view -----
  async function tick(me, pc, send) {
    keepPlaying(me.video);
    if (!pc) return;
    let rep; try { rep = await pc.getStats(); } catch (e) { return; }
    let v = null, pair = null;
    rep.forEach(s => { if (s.type === 'inbound-rtp' && s.kind === 'video') v = s; if (s.type === 'candidate-pair' && s.nominated && s.state === 'succeeded') pair = s; });
    if (!v) return;
    const r = { t: performance.now(), bytes: v.bytesReceived || 0, frames: v.framesDecoded || 0, jb: v.jitterBufferDelay || 0, jbn: v.jitterBufferEmittedCount || 0, dec: v.totalDecodeTime || 0, w: v.frameWidth, h: v.frameHeight };
    const last = me.last;
    if (last) {
      const dt = (r.t - last.t) / 1000, df = r.frames - last.frames, dn = r.jbn - last.jbn;
      const rtt = pair && pair.currentRoundTripTime ? pair.currentRoundTripTime * 1000 : 0;
      const jbMs = dn > 0 ? (r.jb - last.jb) / dn * 1000 : 0;
      const decMs = df > 0 ? (r.dec - last.dec) / df * 1000 : 0;
      const est = Math.round(rtt / 2 + jbMs + decMs);
      me.stats = { fps: Math.round(df / dt), kbps: Math.round((r.bytes - last.bytes) * 8 / dt / 1000), w: v.frameWidth || 0, h: v.frameHeight || 0, rtt: Math.round(rtt), jb: Math.round(jbMs), dec: +decMs.toFixed(1), est };
      if (me === main || me === wall[0]) { T.stats = me.stats; hud(); }
      if (!PROBE) send({ t: 'lat', p50: est, p99: est });
      if (v.frameWidth && (last.w !== v.frameWidth || last.h !== v.frameHeight)) send({ t: 'size', w: v.frameWidth, h: v.frameHeight });
      // The watchdog: frames decode but none is shown, or bytes arrive but nothing
      // decodes. Ask for a keyframe; if that does not help, reconnect this view.
      const quiet = performance.now() - me.shown;
      const decoding = df > 0, arriving = r.bytes - last.bytes > 2000;
      if (me.live && !document.hidden && quiet > 2500 && (decoding || arriving)) {
        me.stuck++;
        T.healed = (T.healed || 0) + 1;
        if (me.stuck === 1) { send({ t: 'key' }); if (me.video.srcObject) { const s = me.video.srcObject; me.video.srcObject = null; me.video.srcObject = s; keepPlaying(me.video); } }
        else if (me.stuck >= 4) me.heal();
      }
    }
    me.last = r;
  }
  function hud() {
    const s = T.stats; if (!s || $('hud').hidden) return;
    let t = s.fps + ' fps  ' + s.w + 'x' + s.h + '  ' + (s.kbps / 1000).toFixed(1) + ' Mb/s  ~' + s.est + ' ms';
    if (T.lat) t += '  p50 ' + T.lat.p50 + ' p99 ' + T.lat.p99;
    if (wallOn) t += '  ' + wall.length + ' displays';
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
      main && main.send({ t: 'lat', p50: T.lat.p50, p99: T.lat.p99 });
      hud();
    }, 1000);
  }

  // ----- the wall: every display at once, arranged like monitors on a desk -----
  const LAYOUTS = ['grid', 'row', 'column', 'focus'], LAYOUT_NAMES = { grid: 'Grid', row: 'Row', column: 'Column', focus: 'Focus' };
  function wallPrefs() {
    try { const p = JSON.parse(localStorage.getItem(WALL_KEY) || '{}'); return { layout: LAYOUTS.includes(p.layout) ? p.layout : 'grid', order: Array.isArray(p.order) ? p.order.map(Number) : [] }; }
    catch (e) { return { layout: 'grid', order: [] }; }
  }
  function saveWall(p) { try { localStorage.setItem(WALL_KEY, JSON.stringify(p)); } catch (e) {} }
  function orderedDisplays() {
    const p = wallPrefs(), ids = displayList.map(d => d.id);
    const order = p.order.filter(id => ids.includes(id)).concat(ids.filter(id => !p.order.includes(id)));
    return order.map(id => displayList.find(d => d.id === id));
  }
  function applyLayout() {
    const p = wallPrefs(), w = $('wall');
    w.dataset.layout = p.layout;
    w.style.setProperty('--n', String(wall.length || 1));
    w.style.setProperty('--cols', String(Math.ceil(Math.sqrt(wall.length || 1))));
    $('layoutTx').textContent = LAYOUT_NAMES[p.layout];
    shapeScreens();
  }
  // Each screen as large as its cell allows, in its display's aspect.
  function shapeScreens() {
    for (const t of $('wall').children) {
      const sc = t.firstChild, ar = Number(t.dataset.ar) || 16 / 9, w = t.clientWidth, h = t.clientHeight;
      if (!w || !h) continue;
      const fill = $('wall').dataset.fit === 'fill';
      const sw = fill ? w : Math.min(w, h * ar), sh = fill ? h : Math.min(h, w / ar);
      sc.style.width = Math.floor(sw) + 'px'; sc.style.height = Math.floor(sh) + 'px';
    }
  }
  function buildWall(dev) {
    wallOn = true;
    $('stage').hidden = true; $('wall').hidden = false; $('displaySel').hidden = true; $('btnLayout').hidden = false;
    $('btnWall').setAttribute('aria-pressed', 'true');
    const box = $('wall');
    box.textContent = '';
    wall = [];
    orderedDisplays().forEach((d, i) => {
      const video = el('video', { playsinline: '', autoplay: '', muted: '' });
      video.muted = true;
      wireVideo(video);
      // The tile is the cell; the screen inside it has the display's own shape.
      const screen = el('div', { class: 'screen' }, video, el('span', { class: 'tile-name', text: d.name || 'Display ' + d.id }), el('span', { class: 'tile-state', text: 'Connecting...' }));
      const tile = el('div', { class: 'tile', tabindex: '0', 'data-display': String(d.id), 'data-ar': String(d.width && d.height ? d.width / d.height : 16 / 9), 'aria-label': d.name || 'Display ' + d.id }, screen);
      box.append(tile);
      const make = () => connect(dev, { video, display: d.id, primary: i === 0, onState: s => {
        tile.querySelector('.tile-state').textContent = s === 'live' ? '' : s.startsWith('end:') ? 'Reconnecting...' : 'Connecting...';
        tile.classList.toggle('live', s === 'live');
      }, restart: () => { const k = wall.indexOf(view); if (k >= 0 && wallOn) { view = make(); wall[k] = view; } } });
      let view = make();
      wall.push(view);
      wireTile(tile);
    });
    applyLayout();
    requestAnimationFrame(shapeScreens);
    note('wall');
  }
  function leaveWall() {
    const first = wall[0] && wall[0].display;
    wallOn = false;
    $('btnWall').setAttribute('aria-pressed', 'false'); $('btnLayout').hidden = true;
    for (const v of wall) v.stop('stopped');
    wall = []; $('wall').textContent = ''; $('wall').hidden = true;
    T.wantDisplay = first;
    watch();
  }
  function moveTile(from, to) {
    const p = wallPrefs(), ids = [...$('wall').children].map(t => Number(t.dataset.display));
    const [x] = ids.splice(from, 1); ids.splice(to, 0, x);
    p.order = ids; saveWall(p);
    const tiles = [...$('wall').children], t = tiles[from];
    $('wall').insertBefore(t, to > from ? tiles[to].nextSibling : tiles[to]);
    const [v] = wall.splice(from, 1); wall.splice(to, 0, v);
    applyLayout();
  }
  function focusTile(tile) {
    const tiles = [...$('wall').children], i = tiles.indexOf(tile), p = wallPrefs();
    if (p.layout === 'focus' && i === 0) { p.layout = 'grid'; saveWall(p); applyLayout(); return; }
    if (i > 0) moveTile(i, 0);
    const q = wallPrefs(); q.layout = 'focus'; saveWall(q); applyLayout();
  }
  // Drag a tile onto another to swap places; double-tap makes it the big one;
  // arrow keys move a focused tile.
  function wireTile(tile) {
    let start = null, ghost = false, lastTap = 0;
    tile.addEventListener('pointerdown', e => { if (e.button !== 0) return; start = { x: e.clientX, y: e.clientY, id: e.pointerId }; });
    tile.addEventListener('pointermove', e => {
      if (!start || e.pointerId !== start.id) return;
      if (!ghost && Math.hypot(e.clientX - start.x, e.clientY - start.y) > 10) { ghost = true; tile.classList.add('dragging'); tile.setPointerCapture(e.pointerId); }
      if (ghost) {
        tile.style.transform = 'translate(' + (e.clientX - start.x) + 'px,' + (e.clientY - start.y) + 'px)';
        for (const t of $('wall').children) t.classList.toggle('drop', t !== tile && hit(t, e));
      }
    });
    const up = e => {
      if (!start) return;
      if (ghost) {
        const tiles = [...$('wall').children], target = tiles.find(t => t !== tile && hit(t, e));
        tile.classList.remove('dragging'); tile.style.transform = '';
        for (const t of tiles) t.classList.remove('drop');
        if (target) moveTile(tiles.indexOf(tile), tiles.indexOf(target));
      } else {
        const now = Date.now();
        if (now - lastTap < 350) { focusTile(tile); lastTap = 0; } else lastTap = now;
      }
      start = null; ghost = false;
    };
    tile.addEventListener('pointerup', up);
    tile.addEventListener('pointercancel', () => { start = null; ghost = false; tile.classList.remove('dragging'); tile.style.transform = ''; });
    tile.addEventListener('keydown', e => {
      const tiles = [...$('wall').children], i = tiles.indexOf(tile);
      if (e.key === 'ArrowRight' || e.key === 'ArrowDown') { if (i < tiles.length - 1) { moveTile(i, i + 1); tile.focus(); } e.preventDefault(); }
      else if (e.key === 'ArrowLeft' || e.key === 'ArrowUp') { if (i > 0) { moveTile(i, i - 1); tile.focus(); } e.preventDefault(); }
      else if (e.key === 'Enter') { focusTile(tile); e.preventDefault(); }
    });
  }
  function hit(t, e) { const r = t.getBoundingClientRect(); return e.clientX >= r.left && e.clientX <= r.right && e.clientY >= r.top && e.clientY <= r.bottom; }

  // ----- zoom and pan on the single view: pinch, Ctrl/Cmd + wheel, drag, double-click -----
  const zoom = { s: 1, x: 0, y: 0 };
  function setZoom(s, cx, cy) {
    const st = $('stage').getBoundingClientRect();
    s = Math.min(8, Math.max(1, s));
    if (cx == null) { cx = st.width / 2; cy = st.height / 2; }
    // Keep the point under the fingers where it is.
    zoom.x = cx - (cx - zoom.x) * (s / zoom.s);
    zoom.y = cy - (cy - zoom.y) * (s / zoom.s);
    zoom.s = s;
    clampZoom();
  }
  function clampZoom() {
    const st = $('stage').getBoundingClientRect();
    if (zoom.s <= 1.001) { zoom.s = 1; zoom.x = 0; zoom.y = 0; }
    zoom.x = Math.min(0, Math.max(st.width - st.width * zoom.s, zoom.x));
    zoom.y = Math.min(0, Math.max(st.height - st.height * zoom.s, zoom.y));
    $('video').style.transform = zoom.s === 1 ? '' : 'translate(' + zoom.x + 'px,' + zoom.y + 'px) scale(' + zoom.s + ')';
    $('zoomChip').hidden = zoom.s === 1;
    $('zoomChip').textContent = Math.round(zoom.s * 100) + '%';
    $('stage').classList.toggle('zoomed', zoom.s > 1);
  }
  function wireZoom() {
    const st = $('stage'), pts = new Map();
    let pinch = null, pan = null, lastTap = 0;
    st.addEventListener('wheel', e => {
      if (!(e.ctrlKey || e.metaKey) && zoom.s === 1) return;
      e.preventDefault();
      const r = st.getBoundingClientRect();
      if (e.ctrlKey || e.metaKey) setZoom(zoom.s * Math.exp(-e.deltaY * 0.01), e.clientX - r.left, e.clientY - r.top);
      else { zoom.x -= e.deltaX; zoom.y -= e.deltaY; clampZoom(); }
    }, { passive: false });
    st.addEventListener('pointerdown', e => {
      pts.set(e.pointerId, { x: e.clientX, y: e.clientY });
      if (pts.size === 2) { const [a, b] = [...pts.values()]; pinch = { d: Math.hypot(a.x - b.x, a.y - b.y), s: zoom.s }; pan = null; }
      else if (pts.size === 1 && zoom.s > 1) pan = { x: e.clientX, y: e.clientY, zx: zoom.x, zy: zoom.y };
      const now = Date.now();
      if (pts.size === 1) { if (now - lastTap < 300) { const r = st.getBoundingClientRect(); setZoom(zoom.s > 1 ? 1 : 2.5, e.clientX - r.left, e.clientY - r.top); lastTap = 0; } else lastTap = now; }
    });
    st.addEventListener('pointermove', e => {
      if (!pts.has(e.pointerId)) return;
      pts.set(e.pointerId, { x: e.clientX, y: e.clientY });
      const r = st.getBoundingClientRect();
      if (pinch && pts.size === 2) { const [a, b] = [...pts.values()]; setZoom(pinch.s * Math.hypot(a.x - b.x, a.y - b.y) / pinch.d, (a.x + b.x) / 2 - r.left, (a.y + b.y) / 2 - r.top); }
      else if (pan) { zoom.x = pan.zx + e.clientX - pan.x; zoom.y = pan.zy + e.clientY - pan.y; clampZoom(); }
    });
    const lift = e => { pts.delete(e.pointerId); if (pts.size < 2) pinch = null; if (!pts.size) pan = null; };
    st.addEventListener('pointerup', lift); st.addEventListener('pointercancel', lift);
    $('zoomChip').addEventListener('click', () => setZoom(1));
    addEventListener('resize', () => { clampZoom(); if (wallOn) shapeScreens(); });
  }

  // ----- full screen: the page itself, never the browser's own video player -----
  function isFull() { return !!(document.fullscreenElement || document.webkitFullscreenElement) || document.body.classList.contains('immersive'); }
  function toggleFull() {
    const d = document, root = d.documentElement;
    if (d.fullscreenElement || d.webkitFullscreenElement) { (d.exitFullscreen || d.webkitExitFullscreen).call(d); return; }
    if (d.body.classList.contains('immersive')) { d.body.classList.remove('immersive'); fullChanged(); return; }
    const req = root.requestFullscreen || root.webkitRequestFullscreen;
    if (req) { try { const p = req.call(root, { navigationUI: 'hide' }); if (p && p.catch) p.catch(() => immersive()); } catch (e) { immersive(); } return; }
    // iPhone: no page full screen. Its own player is real full screen and turns
    // with the phone; leaving it pauses the video, which keepPlaying undoes.
    const v = $('video');
    if (!wallOn && v.webkitEnterFullscreen && v.srcObject) { try { v.webkitEnterFullscreen(); return; } catch (e) {} }
    immersive();
  }
  function immersive() { document.body.classList.add('immersive'); fullChanged(); }
  function fullChanged() {
    $('btnFull').setAttribute('aria-pressed', String(isFull()));
    $('btnFull').querySelector('use').setAttribute('href', isFull() ? '#i-unfull' : '#i-full');
    // Leaving or entering full screen must never stop the picture.
    for (const v of views()) { keepPlaying(v.video); v.send({ t: 'key' }); }
    clampZoom();
    note(T.state);
  }

  // ----- more: picture-in-picture, snapshot, stats, shortcuts -----
  function pip() {
    const v = wallOn && wall[0] ? wall[0].video : $('video');
    if (document.pictureInPictureElement) return document.exitPictureInPicture().catch(() => {});
    if (v.requestPictureInPicture) return v.requestPictureInPicture().catch(() => {});
    if (v.webkitSetPresentationMode) v.webkitSetPresentationMode(v.webkitPresentationMode === 'picture-in-picture' ? 'inline' : 'picture-in-picture');
  }
  function snapshot() {
    const v = wallOn && wall[0] ? wall[0].video : $('video');
    if (!v.videoWidth) return;
    const c = document.createElement('canvas'); c.width = v.videoWidth; c.height = v.videoHeight;
    c.getContext('2d').drawImage(v, 0, 0);
    c.toBlob(b => {
      if (!b) return;
      const a = el('a', { href: URL.createObjectURL(b), download: (info.name || 'Wi-Live') + ' ' + new Date().toISOString().slice(0, 19).replace(/:/g, '-') + '.png' });
      document.body.append(a); a.click(); a.remove();
      setTimeout(() => URL.revokeObjectURL(a.href), 4000);
    }, 'image/png');
  }
  function toggleStats() { const h = $('hud'); h.hidden = !h.hidden; hud(); }
  function toggleSound() {
    const v = wallOn && wall[0] ? wall[0].video : $('video');
    v.muted = !v.muted; keepPlaying(v);
    $('btnSound').setAttribute('aria-pressed', String(!v.muted));
    $('btnSound').querySelector('use').setAttribute('href', v.muted ? '#i-mute' : '#i-sound');
  }
  function cycleLayout() { const p = wallPrefs(); p.layout = LAYOUTS[(LAYOUTS.indexOf(p.layout) + 1) % LAYOUTS.length]; saveWall(p); applyLayout(); }
  function menu(open) { const m = $('moreMenu'); m.hidden = open === undefined ? !m.hidden : !open; $('btnMore').setAttribute('aria-expanded', String(!m.hidden)); }

  // ----- the bar: fades while watching, back on any touch, move or key -----
  let fadeTimer = 0;
  function wake() {
    $('bar').classList.remove('faded');
    clearTimeout(fadeTimer);
    const fade = () => {
      // Not while something needs reading or a menu is open: look again later.
      if (!$('overlay').hidden || !$('moreMenu').hidden || !$('helpSheet').hidden || !$('homeSheet').hidden) { fadeTimer = setTimeout(fade, 1000); return; }
      $('bar').classList.add('faded');
    };
    fadeTimer = setTimeout(fade, 3000);
  }
  function wireWatch() {
    wireVideo($('video'));
    wireZoom();
    for (const e of ['pointermove', 'pointerdown', 'touchstart', 'keydown']) document.addEventListener(e, () => { if (!$('vWatch').hidden) wake(); }, { passive: true });
    const fits = ['fit', 'fill'], names = { fit: 'Fit', fill: 'Fill' };
    $('btnFit').addEventListener('click', () => { const s = $('stage'); const n = fits[(fits.indexOf(s.dataset.fit) + 1) % fits.length]; s.dataset.fit = n; $('wall').dataset.fit = n; $('fitTx').textContent = names[n]; setZoom(1); if (wallOn) shapeScreens(); });
    $('btnFull').addEventListener('click', toggleFull);
    document.addEventListener('fullscreenchange', fullChanged);
    document.addEventListener('webkitfullscreenchange', fullChanged);
    $('btnSound').addEventListener('click', toggleSound);
    $('btnWall').addEventListener('click', () => { if (wallOn) leaveWall(); else { wallOn = true; watch(); } });
    $('btnLayout').addEventListener('click', cycleLayout);
    $('btnMore').addEventListener('click', e => { e.stopPropagation(); menu(); });
    document.addEventListener('click', e => { if (!$('moreMenu').hidden && !$('moreMenu').contains(e.target)) menu(false); });
    $('mFit').addEventListener('click', () => { menu(false); $('btnFit').click(); });
    $('mPip').addEventListener('click', () => { menu(false); pip(); });
    $('mShot').addEventListener('click', () => { menu(false); snapshot(); });
    $('mStats').addEventListener('click', () => { menu(false); toggleStats(); });
    $('mKeys').addEventListener('click', () => { menu(false); $('helpSheet').hidden = false; });
    $('btnHelpClose').addEventListener('click', () => { $('helpSheet').hidden = true; });
    $('mHome').hidden = !IOS || STANDALONE;
    $('mHome').addEventListener('click', () => { menu(false); $('homeSheet').hidden = false; });
    $('btnHomeClose').addEventListener('click', () => { $('homeSheet').hidden = true; });
    $('btnAgain').addEventListener('click', () => { retry = 0; watch(); });
    $('displaySel').addEventListener('change', e => { T.wantDisplay = Number(e.target.value); main && main.send({ t: 'display', id: T.wantDisplay }); setZoom(1); });
    if (!document.pictureInPictureEnabled && !('webkitSetPresentationMode' in HTMLVideoElement.prototype)) $('mPip').hidden = true;
    // Keyboard: every control has a key (the sheet lists them).
    document.addEventListener('keydown', e => {
      if ($('vWatch').hidden || e.target.closest('input, select, textarea') || e.metaKey || e.ctrlKey || e.altKey) return;
      const k = e.key.toLowerCase();
      if (k === 'f') toggleFull();
      else if (k === 'm' && !$('btnSound').hidden) toggleSound();
      else if (k === 'a' && displayList.length > 1) $('btnWall').click();
      else if (k === 'l' && wallOn) cycleLayout();
      else if (k === 's') toggleStats();
      else if (k === 'p') pip();
      else if (k === 'c') snapshot();
      else if (k === '+' || k === '=') setZoom(zoom.s * 1.25);
      else if (k === '-') setZoom(zoom.s / 1.25);
      else if (k === '0') setZoom(1);
      else if (k === '?') $('helpSheet').hidden = !$('helpSheet').hidden;
      else if (k === 'escape') { menu(false); $('helpSheet').hidden = true; $('homeSheet').hidden = true; if (document.body.classList.contains('immersive')) toggleFull(); }
      else if (/^[1-9]$/.test(k) && !wallOn && displayList[Number(k) - 1]) { $('displaySel').value = String(displayList[Number(k) - 1].id); $('displaySel').dispatchEvent(new Event('change')); }
      else return;
      e.preventDefault();
    });
    // Back to the tab, or back from the browser's back-forward cache: play on.
    document.addEventListener('visibilitychange', () => { if (document.hidden || $('vWatch').hidden) return; if (!views().length) watch(); else for (const v of views()) keepPlaying(v.video); });
    addEventListener('pageshow', e => { if (e.persisted && !$('vWatch').hidden) watch(); });
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
  const drawn = {};
  // A section is rebuilt only when its data changed: a window open all day costs
  // nothing between events, and a scrolled list keeps its place.
  function changed(key, data) { const j = JSON.stringify(data); if (drawn[key] === j) return false; drawn[key] = j; return true; }
  function render(s) {
    T.owner = s;
    if (s.app) {
      const a = s.app, up = a.uptime_s, h = Math.floor(up / 3600), m = Math.floor(up % 3600 / 60);
      $('oHealth').textContent = 'Wi-Live ' + s.version + ' \u00b7 ' + (a.rss_mb != null ? a.rss_mb + ' MB' : '') + (a.cpu_pct != null ? ' \u00b7 ' + a.cpu_pct + '% CPU' : '') + ' \u00b7 up ' + (h ? h + 'h ' : '') + m + 'm';
    }
    $('oLogCount').textContent = s.log_total ? s.log_total + ' kept, newest first (up to ' + s.log_cap + ')' : '';
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
    if (changed('asks', s.pending)) $('oAskList').replaceChildren(...s.pending.map(p => el('div', { class: 'ask', 'data-req': p.req },
      el('div', { class: 'row' }, el('div', { class: 'grow' }, el('div', { class: 'title', text: p.name }), el('div', { class: 'sub', text: p.agent + ' at ' + p.addr }))),
      el('p', { class: 'note', text: 'Allow only if this device shows the same code:' }),
      el('div', { class: 'code', text: p.code }),
      el('div', { class: 'row mt16' },
        el('button', { class: 'btn primary', type: 'button', 'data-act': 'allow', text: 'Allow', onclick: () => act('/api/owner/allow', { req: p.req }) }),
        el('button', { class: 'btn', type: 'button', 'data-act': 'deny', text: 'Deny', onclick: () => act('/api/owner/deny', { req: p.req }) })))));
    // watching: one row per device (a device watching every display holds a session for each)
    const byDev = [];
    for (const l of s.live) { let g = byDev.find(x => x.device === l.device); if (!g) { g = { device: l.device, list: [] }; byDev.push(g); } g.list.push(l); }
    if (changed('live', [s.live, Math.floor(Date.now() / 1000)])) $('oLiveList').replaceChildren(...(byDev.length ? byDev.map(g => { const l = g.list[0], n = g.list.length;
      return el('li', { class: 'item', 'data-live': l.id },
      el('div', { class: 'grow' },
        el('div', { class: 'title', text: l.name }),
        el('div', { class: 'sub', text: l.agent + ' at ' + l.addr + ' for ' + dur(Math.min(...g.list.map(x => x.since))) + (n > 1 ? ', ' + n + ' displays at once' : '') }),
        el('div', { class: 'stats' },
          stat(Math.max(...g.list.map(x => x.fps)), 'fps'), stat(l.width ? l.width + 'x' + l.height : '-', 'picture'),
          stat(l.lat_p50 ? l.lat_p50 + ' ms' : '-', 'link delay'), stat(g.list.some(x => x.sound) ? 'On' : 'Off', 'sound'))),
      el('button', { class: 'btn small', type: 'button', 'data-act': 'end', text: 'End', onclick: async () => { for (const x of g.list) await api('/api/owner/end', { id: x.id }).catch(() => {}); refresh(); } })); })
      : [el('li', { class: 'empty', text: 'Nobody is watching this computer.' })]));
    $('oLiveTx').textContent = byDev.length ? (byDev.length === 1 ? '1 watching' : byDev.length + ' watching') : 'Nobody watching';
    // permission and switches (only the app has them; the test server has none)
    $('oPerm').hidden = s.capture_allowed !== false;
    const sw = Object.entries(s.switches || {});
    $('oThis').hidden = !sw.length;
    const SW = { sound: ['Share this computer\'s sound', 'Viewers hear what this computer plays.'], mic: ['Share the microphone', 'Viewers hear this computer\'s microphone.'], start_at_login: ['Start at login', 'Wi-Live is ready whenever this computer is.'] };
    if (changed('sw', sw)) $('oSwitches').replaceChildren(...sw.filter(([k]) => SW[k]).map(([k, on]) => el('li', { class: 'item' },
      el('div', { class: 'grow' }, el('div', { class: 'title', text: SW[k][0] }), el('div', { class: 'sub', text: SW[k][1] })),
      el('button', { class: 'switch', role: 'switch', type: 'button', 'data-switch': k, 'aria-checked': String(!!on), 'aria-label': SW[k][0], onclick: () => act('/api/owner/switch', { name: k, on: !on }) }))));
    // where to open
    if (changed('urls', s.urls)) $('oUrls').replaceChildren(...(s.urls.length ? s.urls.map((u, i) => el('div', { class: 'where' },
        s.qr && s.qr[i] ? el('img', { class: 'qr', src: s.qr[i], alt: 'QR code for ' + u, width: '148', height: '148' }) : null,
        el('div', {}, el('a', { class: 'url', href: u, text: u, target: '_blank', rel: 'noopener' }), el('p', { class: 'note', text: 'Or point a phone\'s camera at the code.' }))))
      : [el('div', { class: 'msg', text: 'This computer is not on a home network right now.' })]));
    // devices
    if (changed('dev', [s.devices, armed, Math.floor(Date.now() / 60000)])) $('oDevList').replaceChildren(...(s.devices.length ? s.devices.map(d => el('li', { class: 'item', 'data-dev': d.id },
      el('div', { class: 'grow' }, el('div', { class: 'title', text: d.name }), el('div', { class: 'sub', text: d.agent + ', last seen ' + ago(d.last_seen) + (d.last_addr ? ' at ' + d.last_addr : '') })),
      twoStep('forget:' + d.id, 'Remove', 'Remove?', () => act('/api/owner/forget', { id: d.id }))))
      : [el('li', { class: 'empty', text: 'No devices yet. Open the address above on a phone to add one.' })]));
    // log
    if (changed('log', s.log)) $('oLog').replaceChildren(...(s.log.length ? s.log.map(e => el('li', { class: 'item' },
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
    $('oPermBtn').addEventListener('click', () => act('/api/owner/capture', {}));
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
