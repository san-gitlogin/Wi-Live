"""The owner's window in a browser (Linux has no web view of its own): a one-time link
from the tray becomes an owner session cookie, for this computer only."""
import http.client

import pytest

from support.pages import OPEN, wait
from support.server import lan_ip

pytestmark = pytest.mark.e2e
LAN = lan_ip()


def get(port, path, cookie=None, ip="127.0.0.1"):
    c = http.client.HTTPConnection(ip, port, timeout=5)
    h = {"Host": f"{ip}:{port}"}
    if cookie:
        h["Cookie"] = cookie
    c.request("GET", path, headers=h)
    r = c.getresponse()
    return r.status, dict(r.getheaders()), r.read()


def test_a_link_logs_in_once_with_a_cookie_no_page_script_can_read(server):
    st, h, _ = get(server.port, server.owner_link)
    assert st == 302 and h["Location"] == "/#owner"
    ck = h["Set-Cookie"]
    assert "HttpOnly" in ck and "SameSite=Strict" in ck and ck.startswith("wl_owner=")
    cookie = ck.split(";")[0]
    assert get(server.port, "/api/owner/state", cookie)[0] == 200
    assert get(server.port, "/api/owner/state")[0] == 403, "no cookie"
    assert get(server.port, "/api/owner/state", "wl_owner=" + "0" * 64)[0] == 403, "a made-up session"
    assert get(server.port, server.owner_link)[0] == 403, "the link works once"


@pytest.mark.skipif(LAN == "127.0.0.1", reason="no home-network address now")
def test_the_owner_session_is_for_this_computer_only(server):
    st, h, _ = get(server.port, server.owner_link)
    cookie = h["Set-Cookie"].split(";")[0]
    assert get(server.port, "/api/owner/state", cookie, ip=LAN)[0] == 403
    assert get(server.port, server.owner_link, ip=LAN)[0] == 403


def test_the_link_opens_the_owner_window_in_a_browser(chrome, server):
    ctx = chrome.new_context()
    OPEN.append(ctx)
    p = ctx.new_page()
    p.goto(server.base + server.owner_link)
    wait(p, "window.__wl.state === 'owner'")
    assert p.url.endswith("/#owner")
    assert p.text_content("#oLiveTx") == "Nobody watching"
    p.click("#oPause")
    p.wait_for_selector("#oPausedMsg", state="visible")
    assert p.evaluate("document.cookie") == "", "the session cookie is invisible to the page"
    p2 = ctx.new_page()
    p2.goto(server.base + "/#owner")
    wait(p2, "window.__wl.state === 'owner'")


def test_without_a_session_the_owner_page_says_how_to_get_one(chrome, server):
    ctx = chrome.new_context()
    OPEN.append(ctx)
    p = ctx.new_page()
    p.goto(server.base + "/#owner")
    p.wait_for_selector("#oDenied", state="visible")
    assert "icon on this computer" in p.text_content("#oDenied")
