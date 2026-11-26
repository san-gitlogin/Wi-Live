"""The owner's window: permission, switches, and what it shows before anyone pairs."""
import time

import pytest

from support.pages import ask, owner_page, pair, playing, viewer_page, wait

pytestmark = pytest.mark.e2e


def test_before_anyone_pairs_it_says_where_to_open_it_and_that_nobody_watches(chrome, server):
    o = owner_page(chrome, server)
    assert o.text_content("#oLiveTx") == "Nobody watching"
    assert "Open the address above" in o.text_content("#oDevList")
    assert o.locator("#oUrls a").count() >= 1, "an address to type on a device"
    assert o.is_hidden("#oThis") and o.is_hidden("#oPerm"), "the test server has no switches"
    assert not o.errors


def test_screen_recording_is_asked_from_the_window_and_the_banner_goes(chrome, make_server):
    s = make_server(WILIVE_TEST_HOST=1)
    o = owner_page(chrome, s)
    o.wait_for_selector("#oPerm", state="visible")
    o.click("#oPermBtn")
    o.wait_for_selector("#oPerm", state="hidden", timeout=5000)


def test_switches_show_their_state_and_change_on_the_computer(chrome, make_server):
    s = make_server(WILIVE_TEST_HOST=1)
    o = owner_page(chrome, s)
    o.wait_for_selector("#oThis", state="visible")
    assert o.get_attribute("[data-switch=mic]", "aria-checked") == "false"
    assert o.get_attribute("[data-switch=sound]", "aria-checked") == "true"
    o.click("[data-switch=mic]")
    o.wait_for_selector("[data-switch=mic][aria-checked=true]", timeout=5000)
    _, st = s.owner_api("GET", "/api/owner/state")
    assert st["switches"]["mic"] is True
    assert s.owner_api("POST", "/api/owner/switch", {"name": "nope", "on": True})[0] == 404


def test_the_window_counts_viewers_live_and_the_log_reads_as_sentences(chrome, server):
    o = owner_page(chrome, server)
    v = viewer_page(chrome, server)
    pair(o, v, "Kitchen iPad")
    o.wait_for_selector("#oLiveList [data-live] .stat")
    stats = o.text_content("#oLiveList")
    assert "Kitchen iPad" in stats and "fps" in stats and "sound" in stats
    # The picture's size, even when it never changes (it was told only on a change: "-").
    o.wait_for_function("() => /\\d+x\\d+/.test(document.querySelector('#oLiveList').textContent)", timeout=10000)
    _, st = s_api(server)
    assert st["live"][0]["width"] > 0 and st["live"][0]["height"] > 0
    # The newest line, once the session's own event is logged (a slow machine logs it a moment later).
    o.wait_for_function("() => document.querySelector('#oLog .item').textContent.includes('Kitchen iPad started watching from 127.0.0.1')", timeout=10000)


def s_api(server):
    return server.owner_api("GET", "/api/owner/state")


def test_the_qr_code_on_screen_decodes_to_the_address_shown(chrome, server):
    """Read by an independent decoder (OpenCV) from the pixels the window shows."""
    cv2 = pytest.importorskip("cv2")
    import numpy as np
    o = owner_page(chrome, server)
    o.wait_for_selector("#oUrls img.qr")
    url = o.text_content("#oUrls a.url")
    png = o.locator("#oUrls img.qr").first.screenshot()
    img = cv2.imdecode(np.frombuffer(png, np.uint8), cv2.IMREAD_COLOR)
    img = cv2.resize(img, None, fx=3, fy=3, interpolation=cv2.INTER_NEAREST)
    text, _, _ = cv2.QRCodeDetector().detectAndDecode(img)
    assert text == url, f"decoded {text!r}, shown {url!r}"


def test_without_screen_recording_a_device_waits_calmly_and_starts_when_allowed(chrome, make_server):
    """Owner, 2026-11-26: with Screen Recording not allowed (a grant macOS kept for another
    signature of the app), a Windows PC's browser was admitted, failed and ended every
    few seconds - the owner's window flipped "watching" on and off and the PC flipped
    between live and "not allowed". Now: refused before it is ever listed, one steady
    message, and the picture starts by itself once it is allowed."""
    s = make_server(WILIVE_TEST_HOST=1)
    o = owner_page(chrome, s)
    v = viewer_page(chrome, s)
    code = ask(v, "Windows PC")
    o.wait_for_selector(f".ask:has-text('{code}')", timeout=5000)
    o.click(f".ask:has-text('{code}') [data-act=allow]")
    v.wait_for_function("() => document.body.innerText.includes('allow screen recording')", timeout=15000)
    overlays, listed, end = set(), [], time.time() + 12  # two retries and more
    while time.time() < end:
        _, st = s.owner_api("GET", "/api/owner/state")
        listed += st["live"]
        overlays.add(v.evaluate("() => document.getElementById('overlay').hidden ? '' : document.getElementById('overlay').innerText"))
        assert v.evaluate("window.__wl.state") != "live"
        time.sleep(0.25)
    assert not listed, f"listed as watching while it cannot watch: {listed}"
    assert all("allow screen recording" in t for t in overlays), f"the message flickered: {overlays}"
    assert "nobody" in o.text_content("#oLiveTx").lower()
    o.click("#oPermBtn")
    wait(v, "window.__wl.state === 'live'", 15)
    o.wait_for_selector("#oLiveList [data-live]", timeout=10000)
    assert not v.errors


def test_a_viewer_that_closes_its_page_is_logged_as_left_never_as_a_capture_failure(chrome, server):
    """2026-11-26: every closed tab was logged "stopped watching ... (capture)" - our own
    unsubscribe on the way out was read as the screen capture failing."""
    o = owner_page(chrome, server)
    v = viewer_page(chrome, server)
    pair(o, v, "Closing phone")
    playing(v, 5)
    v.close()
    end = time.time() + 10
    ended = []
    while time.time() < end and not ended:
        _, st = server.owner_api("GET", "/api/owner/state")
        ended = [e for e in st["log"] if e["kind"] == "ended" and e["who"] == "Closing phone"]
        time.sleep(.3)
    assert ended, "the end of the session was logged"
    assert "(capture)" not in ended[0]["detail"], ended[0]["detail"]
    assert "(left)" in ended[0]["detail"] or "(network)" in ended[0]["detail"], ended[0]["detail"]
