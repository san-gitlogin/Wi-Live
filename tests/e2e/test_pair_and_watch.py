"""Pairing and watching, end to end: the real server, real Google Chrome, real
hardware H.264. The owner's window and each device are separate pages."""
import pytest

from support.pages import floor, SOFTWARE, ask, frames, owner_page, pair, sizes_ok, until, viewer_page, wait

pytestmark = pytest.mark.e2e


def test_a_new_device_pairs_by_code_and_watches(chrome, server):
    owner = owner_page(chrome, server)
    v = viewer_page(chrome, server)
    code = pair(owner, v, "Living room tablet")
    f = frames(v, 2)
    assert sizes_ok(f["w"], f["h"], 1280, 720), (f["w"], f["h"])
    assert f["n"] >= (20 if SOFTWARE else 60), f"{f['n']} frames in 2 s (60 fps sent)"
    owner.wait_for_selector("#oLiveList [data-live]:has-text('Living room tablet')")
    assert owner.text_content("#oLiveTx") == "1 watching"
    owner.wait_for_selector("#oDevList [data-dev]:has-text('Living room tablet')")
    log = owner.text_content("#oLog")
    assert "asked to watch" in log and "was allowed" in log and "started watching" in log
    assert code and code.replace(" ", "").isdigit() and len(code) == 7
    assert not v.errors and not owner.errors, (v.errors, owner.errors)


def test_the_device_is_remembered_and_comes_back_without_asking(chrome, server):
    owner = owner_page(chrome, server)
    v = viewer_page(chrome, server)
    pair(owner, v)
    v.reload()
    wait(v, "window.__wl.state === 'live'", 15)
    assert "pair" not in v.evaluate("window.__wl.events"), "it never showed pairing again"


def test_a_denied_device_is_told_and_cannot_watch(chrome, server):
    owner = owner_page(chrome, server)
    v = viewer_page(chrome, server)
    code = ask(v)
    owner.wait_for_selector(f".ask:has-text('{code}')")
    owner.click(f".ask:has-text('{code}') [data-act=deny]")
    wait(v, "window.__wl.state === 'pair'")
    assert v.text_content("#pairErr") == "The computer said no."
    assert "was refused" in owner.text_content("#oLog")


def test_ending_a_viewer_does_not_let_it_straight_back(chrome, server):
    owner = owner_page(chrome, server)
    v = viewer_page(chrome, server)
    pair(owner, v)
    owner.click("#oLiveList [data-act=end]")
    wait(v, "window.__wl.state === 'end:ended'")
    assert v.is_visible("#btnAgain")
    assert "ended your view" in v.text_content("#overlayMsg")
    v.wait_for_timeout(3000)
    assert v.evaluate("window.__wl.state") == "end:ended", "no reconnect by itself"
    owner.wait_for_selector("#oLiveList .empty")
    v.click("#btnAgain")
    wait(v, "window.__wl.state === 'live'")


def test_a_removed_device_goes_back_to_asking(chrome, server):
    owner = owner_page(chrome, server)
    v = viewer_page(chrome, server)
    pair(owner, v)
    owner.click("#oDevList [data-act^=forget]")
    assert owner.text_content("#oDevList [data-act^=forget]") == "Remove?"
    owner.click("#oDevList [data-act^=forget]")
    wait(v, "window.__wl.state === 'pair'", 10)
    assert "removed on the computer" in v.text_content("#pairErr")
    assert v.evaluate("localStorage.getItem('wilive.device.v1')") is None, "its key is forgotten too"


def test_pausing_ends_everyone_and_resuming_brings_them_back(chrome, server):
    owner = owner_page(chrome, server)
    v = viewer_page(chrome, server)
    pair(owner, v)
    owner.click("#oPause")
    wait(v, "window.__wl.state.startsWith('end:paused')")
    assert "paused" in v.text_content("#overlayMsg")
    assert owner.is_visible("#oPausedMsg")
    owner.click("#oPause")
    wait(v, "window.__wl.state === 'live'", 15)


def test_two_devices_watch_at_once_and_the_owner_sees_both(chrome, server):
    owner = owner_page(chrome, server)
    a, b = viewer_page(chrome, server), viewer_page(chrome, server)
    pair(owner, a, "Phone A")
    pair(owner, b, "Phone B")
    owner.wait_for_function("() => document.getElementById('oLiveTx').textContent === '2 watching'")
    assert frames(a, 1.5)["n"] >= floor(40) and frames(b, 1.5)["n"] >= floor(40)


def test_several_displays_each_viewer_chooses_and_an_unplugged_one_falls_back(chrome, make_server):
    s = make_server("1280", "720", "60", WILIVE_DISPLAYS=2)
    owner = owner_page(chrome, s)
    v = viewer_page(chrome, s)
    pair(owner, v)
    assert v.is_visible("#displaySel")
    v.select_option("#displaySel", "2")
    until(lambda: frames(v, 0.5)["w"] == 1280 and v.evaluate("document.getElementById('video').videoHeight") == 1024, 10)
    assert v.evaluate("document.getElementById('video').videoHeight") == 1024
    s.say("unplug 2")
    until(lambda: v.evaluate("document.getElementById('video').videoHeight") == 720, 12)
    assert v.evaluate("document.getElementById('video').videoHeight") == 720
    assert v.evaluate("document.getElementById('displaySel').hidden")
