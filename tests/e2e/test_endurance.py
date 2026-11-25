"""Scale, endurance and disruption: a full log and device list, viewers coming and
going for minutes, the app restarting, the network stalling. Nothing may grow
without bound, nothing may stay broken, and nobody may have to pair again."""
import os
import signal
import subprocess
import sys
import tempfile
import time

import pytest

from support.pages import floor, OPEN, frames, owner_page, pair, viewer_page, wait

pytestmark = pytest.mark.e2e
posix = pytest.mark.skipif(sys.platform == "win32", reason="needs ps and SIGSTOP (POSIX); Windows runs the rest")


def threads(pid):
    if sys.platform.startswith("linux"):
        return int(subprocess.run(["ps", "-o", "nlwp=", "-p", str(pid)], capture_output=True, text=True).stdout.strip() or 0)
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


@posix
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
            assert frames(p, 0.3)["n"] >= 2
            p.close()
        time.sleep(2.5)

    cycle(10)   # warm-up: encoders, buffers and the allocator settle
    base_thr = threads(pid)
    # macOS's allocator hands memory back in batches, so RSS swings (seen 48-74 MB
    # over 100 viewers, falling back each time): judge the trend, not two samples.
    samples = []
    for _ in range(40):
        p = ctx.new_page()
        p.goto(server.base + "/")
        wait(p, "window.__wl.state === 'live'", 20)
        p.close()
        time.sleep(0.4)
        samples.append(rss_mb(pid))
    n = len(samples)
    mx = sum(range(n)) / n
    my = sum(samples) / n
    slope = sum((i - mx) * (y - my) for i, y in enumerate(samples)) / sum((i - mx) ** 2 for i in range(n))
    time.sleep(2.5)
    thr = threads(pid)
    print(f"\n  40 viewers after warm-up: rss {min(samples):.0f}-{max(samples):.0f} MB, trend {slope:+.2f} MB per viewer, threads {base_thr} -> {thr}")
    assert slope < 0.4, f"memory trends up {slope:.2f} MB per viewer: a leak (a lost session costs megabytes)"
    assert max(samples) < 150, f"memory peaked at {max(samples):.0f} MB"
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
    assert frames(v, 1)["n"] >= floor(30)
    ev = v.evaluate("window.__wl.events")
    after = ev[max(i for i, e in enumerate(ev) if e.startswith("end:")):]
    assert "code" not in after and "pair" not in after, f"never asked to pair again: {after}"


@posix
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
    assert frames(v, 1)["n"] >= floor(30), "the picture flows again"
