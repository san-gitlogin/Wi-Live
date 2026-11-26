"""Capture-to-present latency, measured and gated (docs/DESIGN.md "Latency", D-03).

The test picture carries the wall-clock ms of the moment each frame goes to the
encoder, as a strip of blocks. The viewer page (?probe) reads the strip back from
every frame it presents (requestVideoFrameCallback + canvas) and compares it with
the frame's expected display time on the same clock. Same computer, so one clock:
what is measured is encode + WebRTC + jitter buffer + decode + compositing.

Budgets (loopback): p50 <= 35 ms, p99 <= 60 ms. Raising one needs the owner.
"""
import json
import os
import pathlib

import pytest

from support.pages import owner_page, pair, viewer_page, wait

pytestmark = [pytest.mark.e2e, pytest.mark.latency]
P50_MS, P99_MS = 35, 60
WARMUP_S, MEASURE_S = 3, 12
OUT = pathlib.Path(__file__).resolve().parents[1] / ".out"


def measure(chrome, server, label):
    owner = owner_page(chrome, server)
    v = viewer_page(chrome, server, probe=True)
    pair(owner, v)
    v.wait_for_timeout(WARMUP_S * 1000)
    v.evaluate("() => window.__wl.probe.reset()")
    v.wait_for_timeout(MEASURE_S * 1000)
    r = v.evaluate("""() => { const p = window.__wl.probe, s = p.samples.slice().sort((a, b) => a - b);
        const q = x => s[Math.min(s.length - 1, Math.floor(x * s.length))];
        return {n: s.length, bad: p.bad(), ahead: p.ahead(), p50: q(.5), p95: q(.95), p99: q(.99), max: s[s.length - 1], min: s[0],
                fps: (window.__wl.stats || {}).fps, est: (window.__wl.stats || {}).est,
                parts: [0, 1, 2].map(i => { const a = p.parts.map(x => x[i]).sort((a, b) => a - b); return a[a.length >> 1]; })}; }""")
    r["label"] = label
    OUT.mkdir(exist_ok=True)
    with open(OUT / "latency.jsonl", "a") as f:
        f.write(json.dumps(r) + "\n")
    print(f"\n  {label}: n={r['n']} bad={r['bad']} p50={r['p50']:.1f} p95={r['p95']:.1f} p99={r['p99']:.1f} max={r['max']:.1f} ms, {r['fps']} fps;"
          f" median to receive {r['parts'][0]} + decode {r['parts'][1]} + to display {r['parts'][2]} ms")
    return r


# 3024x1964: a 14-inch MacBook Pro's Retina display, at Auto (the long edge capped at
# 1920: the fast default). Sharp, at its own size, is measured and reported below.
@pytest.mark.parametrize("w,h", [(1280, 720), (1920, 1080), (3024, 1964)])
def test_capture_to_present_latency_is_within_budget(chrome, make_server, w, h):
    # 59 fps against a 60 Hz display: the frame-to-refresh alignment sweeps through
    # every phase once a second, so p50 is the average wait and p99 the worst - not
    # one random alignment per run (at 60 fps it is fixed for a whole run: 1080p
    # p50 came out 22 to 39 ms from run to run).
    s = make_server(str(w), str(h), "59", WILIVE_CLOCK=1)
    r = measure(chrome, s, f"{w}x{h}@59")
    assert r["n"] >= MEASURE_S * 48, f"too few frames read: {r['n']} (bad {r['bad']})"
    assert r["bad"] <= r["n"] * 0.02, f"{r['bad']} frames without a readable strip"
    assert r["ahead"] <= r["n"] * 0.02, f"{r['ahead']} frames read ahead of their timings (canvas held a newer frame)"
    assert r["p50"] <= P50_MS, f"p50 {r['p50']:.1f} ms > {P50_MS}"
    assert r["p99"] <= P99_MS, f"p99 {r['p99']:.1f} ms > {P99_MS}"


def test_sharp_at_retina_size_is_measured_and_reported(chrome, make_server):
    """Sharp trades speed for sharp text, by the viewer's choice: measured and written
    down (tests/.out/latency.jsonl), gated only against breaking (it keeps moving)."""
    s = make_server("3024", "1964", "59", WILIVE_CLOCK=1)
    owner = owner_page(chrome, s)
    v = viewer_page(chrome, s, probe=True)
    pair(owner, v)
    v.click("#btnMore")
    v.click("#qualitySeg [data-q=sharp]")
    v.keyboard.press("Escape")
    v.wait_for_function("() => document.getElementById('video').videoWidth === 3024", timeout=20000)
    v.wait_for_timeout(WARMUP_S * 1000)
    v.evaluate("() => window.__wl.probe.reset()")
    v.wait_for_timeout(MEASURE_S * 1000)
    r = v.evaluate("""() => { const p = window.__wl.probe, s = p.samples.slice().sort((a, b) => a - b);
        const q = x => s[Math.min(s.length - 1, Math.floor(x * s.length))];
        return {n: s.length, p50: q(.5), p99: q(.99)}; }""")
    with open(OUT / "latency.jsonl", "a") as f:
        f.write(json.dumps(dict(r, label="3024x1964 Sharp")) + "\n")
    print(f"\n  3024x1964 Sharp: n={r['n']} p50={r['p50']:.1f} p99={r['p99']:.1f} ms ({r['n'] / MEASURE_S:.0f} fps read)")
    assert r["n"] >= MEASURE_S * 10, "it keeps moving at full size"
