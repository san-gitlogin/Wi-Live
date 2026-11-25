"""The installed app, for real: a headless Chrome viewer on this Mac connects over
the home-network address, asks to watch (the owner taps Allow in the window), then
watches the real screen and reports what it sees. Writes tests/.out/real_app.json
as it goes. Run by hand: python3 tests/real_app.py [seconds]"""
import json, pathlib, socket, sys, time
from playwright.sync_api import sync_playwright

OUT = pathlib.Path(__file__).resolve().parent / ".out" / "real_app.json"
SECS = int(sys.argv[1]) if len(sys.argv) > 1 else 20
s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM); s.connect(("192.168.255.255", 9)); IP = s.getsockname()[0]; s.close()
URL = f"http://{IP}:47900/"
state = {"url": URL, "phase": "starting"}
def save(**k):
    state.update(k); OUT.write_text(json.dumps(state, indent=1))

with sync_playwright() as p:
    b = p.chromium.launch(channel="chrome", args=["--autoplay-policy=no-user-gesture-required"])
    ctx = b.new_context(viewport={"width": 1280, "height": 800})
    pg = ctx.new_page()
    pg.goto(URL)
    pg.wait_for_function("() => window.__wl && window.__wl.state === 'pair'")
    pg.fill("#devName", "Test viewer (this Mac)")
    pg.click("#btnAsk")
    pg.wait_for_function("() => window.__wl.state === 'code'")
    save(phase="waiting_for_allow", code=pg.text_content("#codeTx"))
    pg.wait_for_function("() => ['live', 'end:capture', 'end:permission'].includes(window.__wl.state) || window.__wl.state.startsWith('end:')", timeout=300000)
    save(phase="connected", viewer_state=pg.evaluate("window.__wl.state"))
    t0 = time.time(); samples = []
    while time.time() - t0 < SECS:
        pg.wait_for_timeout(1000)
        r = pg.evaluate("""() => { const v = document.getElementById('video');
            return {state: window.__wl.state, w: v.videoWidth, h: v.videoHeight, stats: window.__wl.stats || null,
                    overlay: document.getElementById('overlay').hidden ? '' : document.getElementById('overlayMsg').textContent}; }""")
        samples.append(r); save(phase="watching", last=r, n=len(samples))
    pg.screenshot(path=str(OUT.parent / "real_app_view.png"))
    save(phase="done", samples=samples[-5:])
    b.close()
