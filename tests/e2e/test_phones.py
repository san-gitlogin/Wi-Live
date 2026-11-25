"""Phones, in WebKit (Safari's engine) with an iPhone's size, touch and user agent:
upright and turned on its side with a notch. The owner's iPhone, turned sideways,
had controls that did not answer; and the camera's own Safari view forgot the phone."""
import pytest

from support.pages import OPEN, frames, owner_page, pair, wait

pytestmark = pytest.mark.e2e
NOTCH = 47  # an iPhone's sensor housing in landscape, in CSS px


@pytest.fixture
def iphone(pw, server):
    try:
        wk = pw.webkit.launch()
    except Exception as e:
        pytest.skip(f"WebKit not available: {e}")
    d = dict(pw.devices["iPhone 13"])
    d.pop("default_browser_type", None)
    # bypass_csp: only so the test can draw a pretend notch with a style tag.
    ctx = wk.new_context(**d, bypass_csp=True)
    OPEN.append(ctx)
    yield ctx
    wk.close()


def notch(v):
    v.add_style_tag(content=f":root {{ --sal: {NOTCH}px !important; --sar: {NOTCH}px !important; --sab: 21px !important; }}")


def test_the_camera_opened_page_tells_how_to_be_remembered(iphone, server):
    v = iphone.new_page()
    v.goto(server.base + "/")
    wait(v, "window.__wl.state === 'pair'")
    assert v.is_visible("#iosHint") and "Safari icon" in v.text_content("#iosHint")


def test_turned_sideways_every_control_answers_one_tap_clear_of_the_notch(chrome, iphone, server):
    o = owner_page(chrome, server)
    v = iphone.new_page()
    v.goto(server.base + "/")
    pair(o, v, "iPhone")
    assert frames(v, 1.5)["n"] >= 20
    v.set_viewport_size({"width": 844, "height": 390})
    notch(v)
    v.wait_for_timeout(3600)  # the bar fades
    assert "faded" in v.get_attribute("#bar", "class")
    boxes = v.evaluate("() => [...document.querySelectorAll('#bar button, #bar select, #liveChip')].filter(e => e.offsetParent).map(e => { const r = e.getBoundingClientRect(); return [e.id, r.left, r.right]; })")
    bad = [b for b in boxes if b[1] < NOTCH - 0.5 or b[2] > 844 - NOTCH + 0.5]
    assert not bad, f"under the notch: {bad}"
    # one tap on the faded "More" opens it; one tap on its item acts
    r = v.locator("#btnMore").bounding_box()
    v.touchscreen.tap(r["x"] + r["width"] / 2, r["y"] + r["height"] / 2)
    assert v.is_visible("#moreMenu"), "a faded control answered the first tap"
    assert v.is_visible("#mHome"), "iPhone in Safari: the Home Screen guide is offered"
    v.locator("#mHome").tap()
    assert v.is_visible("#homeSheet")
    v.locator("#btnHomeClose").tap()
    # full screen and back, then upright again: still playing
    v.locator("#btnFull").tap()
    v.wait_for_timeout(500)
    v.locator("#btnFull").tap()
    v.wait_for_timeout(500)
    v.set_viewport_size({"width": 390, "height": 844})
    v.wait_for_timeout(500)
    assert v.evaluate("document.getElementById('video').paused") is False
    assert frames(v, 1.5)["n"] >= 20
    assert v.evaluate("window.__wl.state") == "live"


def test_the_phone_is_remembered_and_goes_straight_to_the_picture(chrome, iphone, server):
    o = owner_page(chrome, server)
    v = iphone.new_page()
    v.goto(server.base + "/")
    pair(o, v, "iPhone")
    v2 = iphone.new_page()   # the same Safari, a new visit
    v2.goto(server.base + "/")
    wait(v2, "window.__wl.state === 'live'", 20)
    assert "pair" not in v2.evaluate("window.__wl.events")
