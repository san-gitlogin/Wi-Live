"""A WebRTC viewer in Python (aiortc), for the Linux run: there is no Google Chrome for
Linux on ARM and Playwright's Chromium has no H.264 decoder, so this one decodes it.

It pairs like a real device (tests/support/device.py, an independent crypto
implementation), proves itself, checks the fingerprint proof, then watches the real
X11 capture of a two-monitor virtual desktop (left red 1920x1080, right blue
1280x1200): both displays listed, the left one red and moving, the right one blue
at its own shape after switching.
"""
import asyncio, json, pathlib, sys, time
import numpy as np
from aiortc import RTCPeerConnection, RTCSessionDescription

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
from support.device import Device, WS  # noqa: E402

info = json.loads(open(sys.argv[1]).readline())
ok = True


def check(name, cond, detail=""):
    global ok
    ok &= bool(cond)
    print(("ok  " if cond else "FAIL"), name, detail, flush=True)


class Srv:  # what Device needs from tests/support/server.py, for a server already running
    def __init__(self, port, owner):
        self.port, self.owner = port, owner

    def http(self, method, path, body=None, headers=None, host=None, ip="127.0.0.1", timeout=5):
        import urllib.request, urllib.error
        h = {"Host": host or f"{ip}:{self.port}"}
        if body is not None:
            h["Content-Type"] = "application/json"
            h["Origin"] = f"http://{h['Host']}"
        h.update(headers or {})
        req = urllib.request.Request(f"http://{ip}:{self.port}{path}", data=None if body is None else json.dumps(body).encode(), method=method, headers=h)
        try:
            with urllib.request.urlopen(req, timeout=timeout) as r:
                return r.status, json.loads(r.read() or b"{}")
        except urllib.error.HTTPError as e:
            return e.code, json.loads(e.read() or b"{}")

    def owner_api(self, method, path, body=None):
        return self.http(method, path, body, headers={"X-Wilive-Owner": self.owner})


frames = []
# aiortc sends no transport-wide congestion feedback, so the app's bandwidth estimate
# never climbs for it and rate control rightly keeps it on a smaller size. What
# proves the right display is its shape and its colour, at any of the three sizes.
def is_size(w, h, fw, fh):
    return (w, h) in {(fw, fh), (fw // 2 & ~1, fh // 2 & ~1), (fw // 4 & ~1, fh // 4 & ~1)}


async def main():
    d = Device(Srv(info["port"], info["owner"]), "Linux viewer")
    code = d.pair()
    check("paired by numeric comparison, allowed as the owner", len(code) == 6)
    ws, chal, ready = d.hello()
    check("proved itself; the app is ready", ready.get("t") == "ready", ready.get("t"))
    check("the fingerprint comes with a proof made with this device's key", d.fp_ok(chal, ready["fp"], ready["fp_mac"]))
    displays = ready.get("displays", [])
    check("both displays are listed", len(displays) >= 2, [(x["name"], x["width"], x["height"]) for x in displays])
    pc = RTCPeerConnection()
    pc.addTransceiver("video", direction="recvonly")
    got = asyncio.Event()

    @pc.on("track")
    def on_track(track):
        async def pull():
            while True:
                try:
                    f = await track.recv()
                except Exception:
                    return
                a = f.to_ndarray(format="rgb24")
                frames.append((time.time(), a.shape[1], a.shape[0], a.reshape(-1, 3).mean(axis=0)))
                got.set()
        asyncio.ensure_future(pull())

    await pc.setLocalDescription(await pc.createOffer())
    ws.send({"t": "offer", "sdp": pc.localDescription.sdp})
    while True:
        m = ws.recv()
        if m.get("t") == "answer":
            break
    fp = [l for l in m["sdp"].splitlines() if l.startswith("a=fingerprint:sha-256 ")][0].split()[1].replace(":", "").lower()
    check("the answer carries the proven fingerprint", fp == ready["fp"])
    await pc.setRemoteDescription(RTCSessionDescription(m["sdp"], "answer"))
    await asyncio.wait_for(got.wait(), 30)
    await asyncio.sleep(4)
    left = [f for f in frames if is_size(f[1], f[2], 1920, 1080)]
    check("the main display arrives, in its shape", bool(left), sorted({(f[1], f[2]) for f in frames}))
    if left:
        r, g, b = left[-1][3]
        check("and it is the red one", r > 120 and b < 80, f"rgb {r:.0f},{g:.0f},{b:.0f}")
        span = left[-1][0] - left[0][0]
        check("frames keep coming", len(left) >= 20, f"{len(left)} frames in {span:.1f} s")
    right = next(x for x in displays if not x["main"])
    n0 = len(frames)
    ws.send({"t": "display", "id": right["id"]})
    for _ in range(60):
        await asyncio.sleep(0.25)
        if any(is_size(f[1], f[2], 1280, 1200) for f in frames[n0:]):
            break
    await asyncio.sleep(1.5)  # a few frames of the new display, not just the first one
    rf = [f for f in frames[n0:] if is_size(f[1], f[2], 1280, 1200)]
    check("switching shows the right display at its own shape", bool(rf), sorted({(f[1], f[2]) for f in frames[n0:]})[:4])
    if rf:
        r, g, b = rf[-1][3]
        check("and it is the blue one", b > 120 and r < 80, f"rgb {r:.0f},{g:.0f},{b:.0f}")
        # CI (ARM, 2026-11-26) once read red off the first frame at the new shape. A frame
        # in the new display's shape showing the OLD display would be a visible flash.
        stale = [f for f in rf if f[3][0] > 120 and f[3][2] < 80]
        check("no frame in the new shape shows the old display", not stale, f"{len(stale)} of {len(rf)} red")
    ws.send({"t": "bye"})
    await pc.close()


asyncio.run(main())
sys.exit(0 if ok else 1)
