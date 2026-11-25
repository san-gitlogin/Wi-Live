"""Watching, beyond the picture: full screen in and out, a paused stream that must
play on, the watchdog, every display at once (arranged and remembered), zoom,
keyboard, the watermark. Plus the same viewer in WebKit, Safari's engine."""
import pytest

from support.pages import floor, OPEN, SOFTWARE, frames, owner_page, pair, viewer_page, wait

pytestmark = pytest.mark.e2e


def live_frames(v, secs=1.0):
    return frames(v, secs)["n"]


def test_full_screen_in_and_out_never_stops_the_picture(chrome, server):
    o, v = owner_page(chrome, server), viewer_page(chrome, server)
    pair(o, v)
    for _ in range(3):
        v.click("#btnFull")
        v.wait_for_timeout(400)
        v.keyboard.press("f")  # and out again by key
        v.wait_for_timeout(400)
        assert v.evaluate("document.getElementById('video').paused") is False
        assert live_frames(v, 1) >= floor(30), "frames keep coming after leaving full screen"
    assert v.evaluate("window.__wl.state") == "live"


def test_a_paused_stream_plays_on_by_itself(chrome, server):
    """Safari pauses the video when its own player closes (the owner's report): a
    live stream is never left paused."""
    o, v = owner_page(chrome, server), viewer_page(chrome, server)
    pair(o, v)
    v.evaluate("() => document.getElementById('video').pause()")
    v.wait_for_timeout(300)
    assert v.evaluate("document.getElementById('video').paused") is False
    assert live_frames(v, 1) >= floor(30)


def test_the_watchdog_heals_a_picture_that_stopped_showing(chrome, server):
    o, v = owner_page(chrome, server), viewer_page(chrome, server)
    pair(o, v)
    # A picture that decodes but is not shown: the frame callbacks stop.
    v.evaluate("() => { const vid = document.getElementById('video'); vid.requestVideoFrameCallback = () => 0; vid.style.visibility = 'hidden'; }")
    wait(v, "(window.__wl.healed || 0) > 0", 15)
    v.evaluate("() => { const vid = document.getElementById('video'); delete vid.requestVideoFrameCallback; vid.style.visibility = ''; }")
    wait(v, "window.__wl.state === 'live'", 15)
    assert live_frames(v, 1) >= floor(30)


def test_every_display_at_once_arranged_and_remembered(chrome, make_server):
    s = make_server("1280", "720", "60", WILIVE_DISPLAYS=3)
    o, v = owner_page(chrome, s), viewer_page(chrome, s)
    pair(o, v, "Wall")
    v.click("#btnWall")
    # Each starts at half size and steps up as the link estimate allows (rate control).
    want = "1280,1280,1080" if not SOFTWARE else None
    v.wait_for_function("(want) => { const w = [...document.querySelectorAll('#wall .tile video')].map(x => x.videoWidth); return want ? w.join() === want : w.every(x => x > 0); }", arg=want, timeout=20000)
    sizes = v.evaluate("() => [...document.querySelectorAll('#wall .tile')].map(t => [t.dataset.display, t.querySelector('video').videoWidth, t.querySelector('video').videoHeight])")
    if SOFTWARE:   # the right display in each tile, by shape
        assert [s[0] for s in sizes] == ["1", "2", "3"] and sizes[2][2] > sizes[2][1] and sizes[0][1] > sizes[0][2], sizes
    else:
        assert sizes == [["1", 1280, 720], ["2", 1280, 1024], ["3", 1080, 1920]], sizes
    o.wait_for_selector("#oLiveList :text('3 displays at once')")
    assert o.text_content("#oLiveTx") == "1 watching", "one device, however many displays"
    # drag the first onto the last
    a, c = v.locator("#wall .tile").nth(0).bounding_box(), v.locator("#wall .tile").nth(2).bounding_box()
    v.mouse.move(a["x"] + a["width"] / 2, a["y"] + a["height"] / 2)
    v.mouse.down()
    v.mouse.move(a["x"] + 60, a["y"] + 60, steps=4)
    v.mouse.move(c["x"] + c["width"] / 2, c["y"] + c["height"] / 2, steps=8)
    v.mouse.up()
    order = lambda: v.evaluate("() => [...document.querySelectorAll('#wall .tile')].map(t => t.dataset.display)")
    assert order() == ["2", "3", "1"]
    # layouts
    for want in ["row", "column", "focus", "grid"]:
        v.click("#btnLayout")
        assert v.get_attribute("#wall", "data-layout") == want
    # double-click makes one the big one
    v.locator("#wall .tile").nth(1).dblclick()
    assert v.get_attribute("#wall", "data-layout") == "focus" and order()[0] == "3"
    # remembered after a reload, and every tile is live again
    v.reload()
    wait(v, "window.__wl.state === 'live'")
    v.click("#btnWall")
    v.wait_for_function("() => [...document.querySelectorAll('#wall .tile video')].filter(x => x.videoWidth > 0).length === 3", timeout=20000)
    assert order() == ["3", "2", "1"] and v.get_attribute("#wall", "data-layout") == "focus"
    # back to one display: the one that was first
    v.click("#btnWall")
    wait(v, "document.getElementById('video').videoHeight > 0")
    portrait = v.evaluate("(() => { const x = document.getElementById('video'); return x.videoHeight > x.videoWidth; })()")
    assert v.is_hidden("#wall") and portrait, "display 3 (the portrait one) was first, so it is the one shown"
    o.wait_for_function("() => !document.getElementById('oLiveList').textContent.includes('displays at once')", timeout=8000)


def test_zoom_with_keys_and_wheel_and_back(chrome, server):
    o, v = owner_page(chrome, server), viewer_page(chrome, server)
    pair(o, v)
    v.keyboard.press("+")
    v.keyboard.press("+")
    assert v.text_content("#zoomChip") == "156%"
    v.mouse.move(640, 400)
    v.keyboard.down("Control")
    v.mouse.wheel(0, -200)
    v.keyboard.up("Control")
    assert int(v.text_content("#zoomChip").rstrip("%")) > 156
    v.keyboard.press("0")
    assert v.is_hidden("#zoomChip") and v.evaluate("document.getElementById('video').style.transform") == ""


def test_the_shortcuts_sheet_and_the_more_menu(chrome, server):
    o, v = owner_page(chrome, server), viewer_page(chrome, server)
    pair(o, v)
    v.keyboard.press("?")
    assert v.is_visible("#helpSheet")
    v.keyboard.press("Escape")
    assert v.is_hidden("#helpSheet")
    v.click("#btnMore")
    assert v.is_visible("#moreMenu")
    v.click("#mStats")
    assert v.is_hidden("#moreMenu") and v.is_visible("#hud")
    wait(v, "document.getElementById('hud').textContent.includes('fps')", 5)


def test_the_watermark_is_small_faint_and_never_in_the_way(chrome, server):
    o, v = owner_page(chrome, server), viewer_page(chrome, server)
    pair(o, v)
    m = v.evaluate("() => { const e = document.querySelector('.mark'), r = e.getBoundingClientRect(), cs = getComputedStyle(e); return {w: r.width, h: r.height, op: +cs.opacity, pe: cs.pointerEvents, left: r.left, bottom: innerHeight - r.bottom}; }")
    assert m["w"] < 90 and m["h"] < 20 and m["op"] < 0.5 and m["pe"] == "none", m
    assert m["left"] < 20 and m["bottom"] < 20, "tucked into a corner"


def test_safari_engine_watches_and_plays_on_after_a_pause(pw, server):
    """WebKit (Safari's engine, via Playwright). Its H.264 and WebRTC are the OS's."""
    try:
        wk = pw.webkit.launch()
    except Exception as e:
        pytest.skip(f"WebKit not available: {e}")
    try:
        o = owner_page(pw.chromium.launch(channel="chrome"), server)
        ctx = wk.new_context(viewport={"width": 1024, "height": 768})
        OPEN.append(ctx)
        v = ctx.new_page()
        v.goto(server.base + "/")
        pair(o, v, "Safari")
        n = frames(v, 2)
        assert n["n"] >= floor(20) and n["w"] == 1280, n
        v.evaluate("() => document.getElementById('video').pause()")
        v.wait_for_timeout(400)
        assert v.evaluate("document.getElementById('video').paused") is False
    finally:
        wk.close()
