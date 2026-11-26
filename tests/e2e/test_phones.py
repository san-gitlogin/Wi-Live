"""Phones, in WebKit (Safari's engine) with an iPhone's size, touch and user agent:
upright and turned on its side with a notch. The owner's iPhone, turned sideways,
had controls that did not answer; and the camera's own Safari view forgot the phone."""
import pytest

from support.pages import floor, OPEN, frames, owner_page, pair, wait

pytestmark = pytest.mark.e2e
NOTCH = 47  # an iPhone's sensor housing in landscape, in CSS px


@pytest.fixture
def iphone(pw, server):
    import sys
    if sys.platform == "win32":
        pytest.skip("Playwright's WebKit for Windows has no WebRTC video (runs on macOS and Linux)")
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
    assert frames(v, 1.5)["n"] >= floor(20)
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
    assert frames(v, 1.5)["n"] >= floor(20)
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


def test_picture_in_picture_never_fails_silently_on_an_iphone(chrome, iphone, server):
    """Owner, 2026-11-26: picture in picture did nothing in the page the camera opened.
    Whatever the browser allows, the person sees it work or is told why."""
    # An iPhone's Safari has Apple's picture-in-picture API (Linux's WebKit does not).
    iphone.add_init_script("if (!HTMLVideoElement.prototype.webkitSetPresentationMode) { HTMLVideoElement.prototype.webkitSetPresentationMode = function () {}; HTMLVideoElement.prototype.webkitSupportsPresentationMode = function () { return false; }; }")
    o = owner_page(chrome, server)
    v = iphone.new_page()
    v.goto(server.base + "/")
    pair(o, v, "iPhone")
    # 1. a view that allows neither API (the camera's mini browser): told what to do
    v.evaluate("""() => { const p = HTMLVideoElement.prototype; delete p.webkitSupportsPresentationMode;
        document.getElementById('video').webkitSupportsPresentationMode = undefined;
        document.getElementById('video').requestPictureInPicture = () => Promise.reject(new Error('no')); }""")
    v.locator("#btnMore").tap()
    v.locator("#mPip").tap()
    v.wait_for_selector("#toast:not([hidden])", timeout=4000)
    assert "Safari" in v.text_content("#toast") and "Home Screen" in v.text_content("#toast")
    # 2. iOS's own API available: it is used, and no error is shown
    v.evaluate("""() => { const vid = document.getElementById('video'); document.getElementById('toast').hidden = true;
        vid.webkitSupportsPresentationMode = m => m === 'picture-in-picture';
        vid.webkitSetPresentationMode = m => { vid._mode = m; };
        Object.defineProperty(vid, 'webkitPresentationMode', { get: () => vid._mode || 'inline', configurable: true }); }""")
    v.locator("#btnMore").tap()
    v.locator("#mPip").tap()
    v.wait_for_timeout(1500)
    assert v.evaluate("document.getElementById('video').webkitPresentationMode") == "picture-in-picture"
    assert v.is_hidden("#toast"), "no false error once it is in picture in picture"
    assert v.evaluate("document.getElementById('video').paused") is False


def test_a_refused_standard_request_falls_back_to_the_webkit_api(chrome, server):
    o = owner_page(chrome, server)
    from support.pages import viewer_page
    v = viewer_page(chrome, server)
    pair(o, v)
    v.evaluate("""() => { const vid = document.getElementById('video');
        vid.requestPictureInPicture = () => Promise.reject(new Error('MediaStream not supported'));
        vid.webkitSupportsPresentationMode = m => m === 'picture-in-picture';
        vid.webkitSetPresentationMode = m => { vid._mode = m; };
        Object.defineProperty(vid, 'webkitPresentationMode', { get: () => vid._mode || 'inline', configurable: true }); }""")
    v.click("#btnMore")
    v.click("#mPip")
    v.wait_for_timeout(1500)
    assert v.evaluate("document.getElementById('video').webkitPresentationMode") == "picture-in-picture"
    assert v.is_hidden("#toast")


def test_in_chrome_picture_in_picture_works_or_says_why(chrome, server):
    o = owner_page(chrome, server)
    from support.pages import viewer_page
    v = viewer_page(chrome, server)
    pair(o, v)
    v.click("#btnMore")
    v.click("#mPip")
    v.wait_for_timeout(1500)
    entered = v.evaluate("document.pictureInPictureElement === document.getElementById('video')")
    told = v.is_visible("#toast")
    assert entered or told, "picture in picture neither started nor explained"
