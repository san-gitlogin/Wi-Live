"""Real capture latency on the installed app. A visible Chrome window shows a clock
strip (the wall-clock ms, redrawn every display frame). The app captures the real
screen; a viewer on this Mac finds the strip in each presented frame and compares
it with the frame's display time. Everything is measured: ScreenCaptureKit, scaling,
encode, WebRTC, decode and display, plus up to one frame for the clock window's own
paint. The viewer keeps its key (tests/.out/viewer-profile), so it asks to be
allowed once.  Run by hand: python3 tests/real_latency.py [seconds]"""
import json, pathlib, socket, statistics, sys
from playwright.sync_api import sync_playwright

HERE = pathlib.Path(__file__).resolve().parent
OUT = HERE / ".out"; OUT.mkdir(exist_ok=True)
SECS = int(sys.argv[1]) if len(sys.argv) > 1 else 20
s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM); s.connect(("192.168.255.255", 9)); IP = s.getsockname()[0]; s.close()
URL = f"http://{IP}:47900/"
CLOCK = """<!doctype html><title>Wi-Live clock</title><body style="margin:0;background:#fff">
<canvas id=c width=1360 height=40 style="width:680px;height:20px;display:block;margin:20px"></canvas><script>
const c=document.getElementById('c'),x=c.getContext('2d'),B=40;
function f(){const t=Date.now()%4294967296;x.fillStyle='#f0f';x.fillRect(0,0,B,B);
for(let i=0;i<32;i++){x.fillStyle=(Math.floor(t/2**(31-i))%2)?'#fff':'#000';x.fillRect(B*(i+1),0,B,B);}
x.fillStyle='#ff0';x.fillRect(B*33,0,B,B);requestAnimationFrame(f);}f();</script>"""
status = lambda **k: (OUT / "real_latency.json").write_text(json.dumps(k, indent=1))

with sync_playwright() as p:
    clock = p.chromium.launch(channel="chrome", headless=False, args=["--window-position=40,80", "--window-size=760,240"])
    cp = clock.new_page(); cp.set_content(CLOCK); cp.bring_to_front()
    v = p.chromium.launch_persistent_context(str(OUT / "viewer-profile"), channel="chrome", headless=True,
                                             args=["--autoplay-policy=no-user-gesture-required"], viewport={"width": 1280, "height": 800})
    pg = v.pages[0] if v.pages else v.new_page()
    pg.goto(URL)
    pg.wait_for_function("() => window.__wl && ['pair', 'live'].includes(window.__wl.state)", timeout=20000)
    if pg.evaluate("window.__wl.state") == "pair":
        pg.fill("#devName", "Latency probe (this Mac)"); pg.click("#btnAsk")
        pg.wait_for_function("() => window.__wl.state === 'code'")
        status(phase="waiting_for_allow", code=pg.text_content("#codeTx"))
    pg.wait_for_function("() => window.__wl.state === 'live'", timeout=300000)
    status(phase="live, finding the clock")
    pg.wait_for_timeout(2500)
    r = pg.evaluate("""async (secs) => {
      const v = document.getElementById('video'), W = v.videoWidth, H = v.videoHeight;
      const cv = document.createElement('canvas'); cv.width = W; cv.height = H;
      const cx = cv.getContext('2d', {willReadFrequently: true});
      cx.drawImage(v, 0, 0); const d = cx.getImageData(0, 0, W, H).data;
      // The strip: a magenta block, 32 bit blocks, a yellow block, all of one width.
      // Colour edges are soft after H.264 (chroma is half resolution): loose thresholds,
      // and the block width from the distance between the two markers, not one edge.
      const mag = i => d[i] > 170 && d[i+1] < 110 && d[i+2] > 170, yel = i => d[i] > 170 && d[i+1] > 170 && d[i+2] < 110;
      let found = null;
      for (let y = 0; y < H && !found; y++) {
        let x = 0;
        while (x < W && !found) {
          if (!mag((y * W + x) * 4)) { x++; continue; }
          let e = x; while (e < W && mag((y * W + e) * 4)) e++;
          const run = e - x;
          if (run >= 6 && run <= 90) {
            for (let yx = x + run * 20; yx < Math.min(W, x + run * 45); yx++) {
              if (yel((y * W + yx) * 4)) { found = {x0: x, B: (yx - x) / 33, y}; break; }
            }
          }
          x = e;
        }
      }
      if (!found) return {error: 'clock not on the captured screen (covered?)', W, H, frame: cv.toDataURL('image/png')};
      const x0 = found.x0, B = found.B, cy = found.y + Math.floor(B / 3), y0 = found.y;
      const samples = [], parts = []; let bad = 0;
      const sc = document.createElement('canvas'); sc.width = Math.ceil(B * 34); sc.height = 1;
      const sx = sc.getContext('2d', {willReadFrequently: true});
      const on = (meta) => {
        sx.drawImage(v, x0, cy, sc.width, 1, 0, 0, sc.width, 1);
        const px = sx.getImageData(0, 0, sc.width, 1).data;
        const at = c => { const i = Math.floor(c * B + B / 2) * 4; return [px[i], px[i+1], px[i+2]]; };
        const m = at(0);
        if (m[0] > 200 && m[1] < 70 && m[2] > 200) {
          let t = 0; for (let c = 1; c <= 32; c++) t = t * 2 + (at(c)[1] > 128 ? 1 : 0);
          const wn = Date.now(), pn = performance.now(); let dt = (wn + meta.expectedDisplayTime - pn) % 4294967296 - t; if (dt < -2147483648) dt += 4294967296;
          if (dt > -20 && dt < 5000) { samples.push(dt); if (meta.receiveTime) parts.push((wn + meta.receiveTime - pn) % 4294967296 - t); } else bad++;
        } else bad++;
        v.requestVideoFrameCallback((n, mm) => on(mm));
      };
      v.requestVideoFrameCallback((n, mm) => on(mm));
      await new Promise(r => setTimeout(r, secs * 1000));
      return {W, H, block: B, at: [x0, y0], samples, toReceive: parts, bad};
    }""", SECS)
    clock.close(); v.close()

if r.get("frame"):
    import base64
    (OUT / "real_latency_frame.png").write_bytes(base64.b64decode(r.pop("frame").split(",", 1)[1]))
if "error" in r or not r["samples"]:
    r.setdefault("error", f"no readable frames ({r.get('bad')} unreadable)")
    status(phase="failed", **r); print(r); sys.exit(1)
s = sorted(r["samples"]); q = lambda f: round(s[min(len(s) - 1, int(f * len(s)))], 1)
rx = sorted(r["toReceive"]); qr = lambda f: round(rx[min(len(rx) - 1, int(f * len(rx)))], 1) if rx else None
res = dict(phase="done", frames=len(s), unreadable=r["bad"], seconds=SECS, fps=round(len(s) / SECS, 1), stream=f"{r['W']}x{r['H']}",
           p50=q(.5), p95=q(.95), p99=q(.99), max=round(s[-1], 1), min=round(s[0], 1),
           to_receive_p50=qr(.5), to_receive_p99=qr(.99))
status(**res); print(json.dumps(res))
