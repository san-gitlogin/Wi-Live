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
        return {n: s.length, bad: p.bad(), p50: q(.5), p95: q(.95), p99: q(.99), max: s[s.length - 1], min: s[0],
                fps: (window.__wl.stats || {}).fps, est: (window.__wl.stats || {}).est,
                parts: [0, 1, 2].map(i => { const a = p.parts.map(x => x[i]).sort((a, b) => a - b); return a[a.length >> 1]; })}; }""")
    r["label"] = label
    OUT.mkdir(exist_ok=True)
    with open(OUT / "latency.jsonl", "a") as f:
        f.write(json.dumps(r) + "\n")
    print(f"\n  {label}: n={r['n']} bad={r['bad']} p50={r['p50']:.1f} p95={r['p95']:.1f} p99={r['p99']:.1f} max={r['max']:.1f} ms, {r['fps']} fps;"
          f" median to receive {r['parts'][0]} + decode {r['parts'][1]} + to display {r['parts'][2]} ms")
    return r


@pytest.mark.parametrize("w,h", [(1280, 720), (1920, 1080)])
def test_capture_to_present_latency_is_within_budget(chrome, make_server, w, h):
    s = make_server(str(w), str(h), "60", WILIVE_CLOCK=1)
    r = measure(chrome, s, f"{w}x{h}@60")
    assert r["n"] >= MEASURE_S * 50, f"too few frames read: {r['n']} (bad {r['bad']})"
    assert r["bad"] <= r["n"] * 0.02, f"{r['bad']} frames without a readable strip"
    assert r["min"] >= 0, f"a frame shown before it was made ({r['min']} ms): the strip is misread"
    assert r["p50"] <= P50_MS, f"p50 {r['p50']:.1f} ms > {P50_MS}"
    assert r["p99"] <= P99_MS, f"p99 {r['p99']:.1f} ms > {P99_MS}"
