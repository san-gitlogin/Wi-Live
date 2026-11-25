"""Someone in the middle of the signaling: the viewer must refuse, and show no picture.
The page's WebSocket is intercepted (Playwright route_web_socket) and one message
changed on its way from the computer to the page."""
import json
import re

import pytest

from support.pages import owner_page, pair, viewer_page, wait

pytestmark = pytest.mark.e2e


def tamper(page, change):
    def handler(ws):
        server = ws.connect_to_server()
        ws.on_message(lambda m: server.send(m))

        def down(m):
            try:
                o = json.loads(m)
            except ValueError:
                return ws.send(m)
            ws.send(json.dumps(change(o)))
        server.on_message(down)
    page.route_web_socket(re.compile(r".*/v1/signal"), handler)


def watched_then_reloaded(chrome, server, change):
    owner = owner_page(chrome, server)
    v = viewer_page(chrome, server)
    pair(owner, v)
    tamper(v, change)
    v.reload()
    wait(v, "window.__wl.state.startsWith('end:')", 15)
    return v


def test_a_swapped_fingerprint_is_refused(chrome, server):
    v = watched_then_reloaded(chrome, server, lambda o: dict(o, fp="ab" * 32) if o.get("t") == "ready" else o)
    assert v.evaluate("window.__wl.state") == "end:fingerprint"
    assert "does not look like" in v.text_content("#overlayMsg")
    assert v.evaluate("document.getElementById('video').videoWidth") == 0


def test_a_forged_fingerprint_proof_is_refused(chrome, server):
    v = watched_then_reloaded(chrome, server, lambda o: dict(o, fp_mac="00" * 32) if o.get("t") == "ready" else o)
    assert v.evaluate("window.__wl.state") == "end:fingerprint"


def test_an_answer_from_another_computer_is_refused(chrome, server):
    def swap(o):
        if o.get("t") == "answer":
            o["sdp"] = re.sub(r"a=fingerprint:sha-256 [0-9A-F:]+", "a=fingerprint:sha-256 " + ":".join(["AB"] * 32), o["sdp"])
        return o
    v = watched_then_reloaded(chrome, server, swap)
    assert v.evaluate("window.__wl.state") == "end:fingerprint"
    assert "live" not in v.evaluate("window.__wl.events"), "never live after the swap (events start again on reload)"
