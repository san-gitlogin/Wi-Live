"""Scale, endurance and disruption: a full log and device list, viewers coming and
going for minutes, the app restarting, the network stalling. Nothing may grow
without bound, nothing may stay broken, and nobody may have to pair again."""
import os
import signal
import subprocess
import tempfile
import time

import pytest

from support.pages import OPEN, frames, owner_page, pair, viewer_page, wait

pytestmark = pytest.mark.e2e


def threads(pid):
    out = subprocess.run(["ps", "-M", "-p", str(pid)], capture_output=True, text=True).stdout
    return max(0, len(out.strip().splitlines()) - 1)


def rss_mb(pid):
    return int(subprocess.run(["ps", "-o", "rss=", "-p", str(pid)], capture_output=True, text=True).stdout.strip() or 0) / 1024


def test_a_full_log_and_device_list_scroll_and_stay_fast(chrome, make_server):
    s = make_server(WILIVE_FILL_LOG=600, WILIVE_FILL_DEVICES=32)
    o = owner_page(chrome, s)
    o.wait_for_selector("#oLog .item")
    assert o.locator("#oLog .item").count() == 100, "the newest 100 shown"
    assert "500 kept" in o.text_content("#oLogCount"), "capped at 500, the oldest dropped"
    sc = o.evaluate("(() => { const l = document.getElementById('oLog'); return [l.scrollHeight, l.clientHeight]; })()")
    assert sc[0] > sc[1] + 100 and sc[1] <= 362, f"the activity list scrolls in its own box: {sc}"
    assert o.locator("#oDevList .item").count() == 32
    # a refresh with nothing new is cheap, and keeps the list where it was scrolled
    o.evaluate("() => { document.getElementById('oLog').scrollTop = 400; }")
    t = o.evaluate("""async () => { const t0 = performance.now();
        for (let i = 0; i < 20; i++) { const r = await fetch('/api/owner/state', {headers: {'X-Wilive-Owner': window.WILIVE_OWNER}}); await r.json(); }
        return (performance.now() - t0) / 20; }""")
    o.wait_for_timeout(2200)
    assert o.evaluate("document.getElementById('oLog').scrollTop") == 400, "redraws do not jump the scroll"
    assert t < 40, f"owner state takes {t:.1f} ms"
    h = s.owner_api("GET", "/api/owner/state")[1]["app"]
    assert h["rss_mb"] and h["rss_mb"] < 200, h


def test_viewers_coming_and_going_leak_nothing(chrome, server):
    """Measured 2026-11-26: 36 MB at start, ~54 MB once caches warm (first 10 viewers),
    then +0.1 MB per viewer at most. A leak shows as a slope after the warm-up."""
    o = owner_page(chrome, server)
    v = viewer_page(chrome, server)
    pair(o, v, "Comes and goes")
    ctx = v.context
    v.close()
    pid = server.proc.pid

    def cycle(n):
        for _ in range(n):
            p = ctx.new_page()
            p.goto(server.base + "/")
            wait(p, "window.__wl.state === 'live'", 20)
            assert frames(p, 0.3)["n"] > 3
            p.close()
        time.sleep(2.5)

    cycle(10)   # warm-up: encoders, buffers and the allocator settle
    base_rss, base_thr = rss_mb(pid), threads(pid)
    cycle(25)
    rss, thr = rss_mb(pid), threads(pid)
    print(f"\n  after warm-up: rss {base_rss:.1f} -> {rss:.1f} MB over 25 viewers, threads {base_thr} -> {thr}")
    assert rss - base_rss < 8, f"memory grew {rss - base_rss:.1f} MB over 25 viewers after warm-up"
    assert thr <= base_thr + 2, f"threads {base_thr} -> {thr}: something is left running"
    assert server.owner_api("GET", "/api/owner/state")[1]["live"] == [], "nobody listed as watching"


def test_a_restarted_computer_is_found_again_without_pairing(chrome, make_server):
    home = tempfile.mkdtemp(prefix="wilive-home-")
    s = make_server(WILIVE_HOME=home)
    port = s.port
    o = owner_page(chrome, s)
    v = viewer_page(chrome, s)
    pair(o, v, "Survives restarts")
    s.stop()
    wait(v, "window.__wl.state.startsWith('end:')", 10)
    time.sleep(1)
    s2 = make_server(WILIVE_HOME=home, WILIVE_PORT=port)
    assert s2.port == port
    wait(v, "window.__wl.state === 'live'", 30)
    assert frames(v, 1)["n"] >= 30
    ev = v.evaluate("window.__wl.events")
    after = ev[max(i for i, e in enumerate(ev) if e.startswith("end:")):]
    assert "code" not in after and "pair" not in after, f"never asked to pair again: {after}"


def test_a_network_stall_recovers_by_itself(chrome, server):
    o = owner_page(chrome, server)
    v = viewer_page(chrome, server)
    pair(o, v)
    os.kill(server.proc.pid, signal.SIGSTOP)   # the computer stops answering: a stalled network
    try:
        time.sleep(6)
    finally:
        os.kill(server.proc.pid, signal.SIGCONT)
    wait(v, "window.__wl.state === 'live'", 40)
    v.wait_for_timeout(1500)
    assert frames(v, 1)["n"] >= 30, "the picture flows again"
