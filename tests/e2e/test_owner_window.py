"""The owner's window: permission, switches, and what it shows before anyone pairs."""
import pytest

from support.pages import owner_page, pair, viewer_page

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
    # The newest line, once the session's own event is logged (a slow machine logs it a moment later).
    o.wait_for_function("() => document.querySelector('#oLog .item').textContent.includes('Kitchen iPad started watching from 127.0.0.1')", timeout=10000)


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
