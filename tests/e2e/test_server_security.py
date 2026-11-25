"""The server's defences, over raw HTTP and a raw WebSocket (R-20..R-29)."""
import socket
import time

import pytest

from support.device import WS, Device
from support.server import lan_ip

pytestmark = pytest.mark.e2e
LAN = lan_ip()
needs_lan = pytest.mark.skipif(LAN == "127.0.0.1", reason="this computer has no home-network address now")


def test_a_device_paired_by_an_independent_implementation_is_admitted_and_gets_a_proven_fingerprint(server):
    d = Device(server)
    code = d.pair()
    st, s = server.owner_api("GET", "/api/owner/state")
    assert s["devices"][0]["name"] == "Python device"
    assert len(code) == 6
    ws, chal, ready = d.hello()
    assert ready["t"] == "ready", ready
    assert ready["fp"] == server.fp
    assert d.fp_ok(chal, ready["fp"], ready["fp_mac"]), "the fingerprint proof is made with this device's key"
    ws.close()


def test_a_replayed_proof_is_refused(server):
    d = Device(server)
    d.pair()
    ws, chal, ready = d.hello()
    assert ready["t"] == "ready"
    old = d.mac(chal)
    ws.close()
    ws2, _, r = d.hello(mac=old)
    assert r == {"t": "error", "reason": "device"}, "each connection has its own challenge"
    ws2.close()


def test_a_wrong_key_and_an_unknown_device_are_refused_and_logged(server):
    d = Device(server)
    d.pair()
    ws, _, r = d.hello(mac="00" * 32)
    assert r["reason"] == "device"
    other = Device(server)
    other.id, other.key = "ab" * 16, b"\x01" * 32
    ws2, _, r2 = other.hello()
    assert r2["reason"] == "device"
    _, s = server.owner_api("GET", "/api/owner/state")
    details = [e["detail"] for e in s["log"] if e["kind"] == "refused"]
    assert "wrong proof for this device" in details and "not an allowed device" in details


def test_saying_nothing_on_the_socket_is_refused_within_its_deadline(server):
    ws = WS(server.port)
    ws.recv()  # chal
    t0 = time.time()
    r = ws.recv()
    assert r == {"t": "error", "reason": "hello"} and 4 <= time.time() - t0 <= 7


def test_owner_controls_need_the_token(server):
    assert server.http("GET", "/api/owner/state")[0] == 403
    assert server.http("GET", "/api/owner/state", headers={"X-Wilive-Owner": "x" * len(server.owner)})[0] == 403
    assert server.http("POST", "/api/owner/pause", {"on": True})[0] == 403
    assert server.owner_api("GET", "/api/owner/state")[0] == 200


@needs_lan
def test_owner_controls_are_refused_from_the_network_even_with_the_token(server):
    st, _ = server.http("GET", "/api/owner/state", headers={"X-Wilive-Owner": server.owner}, ip=LAN)
    assert st == 403


def test_a_page_rebinding_its_own_name_to_this_computer_is_refused(server):
    assert server.http("GET", "/", host="evil.example.com")[0] == 421
    assert server.http("GET", "/api/info", host="evil.example.com:47900")[0] == 421
    assert server.http("GET", "/")[0] == 200


def test_changes_need_an_origin_that_is_this_host(server):
    body = {"name": "x", "v_pub": "00" * 32, "commit": "00" * 32}
    assert server.http("POST", "/api/pair/start", body, headers={"Origin": "http://evil.example.com"})[0] == 403
    import urllib.request
    req = urllib.request.Request(f"{server.base}/api/pair/start", data=b"{}", method="POST", headers={"Content-Type": "application/json"})
    try:
        urllib.request.urlopen(req, timeout=5)
        code = 200
    except urllib.error.HTTPError as e:
        code = e.code
    assert code == 403, "a POST without an Origin"
    assert WS(server.port, origin=False).status == 403


def test_sizes_are_capped(server):
    s = socket.create_connection(("127.0.0.1", server.port), timeout=5)
    s.sendall(b"GET / HTTP/1.1\r\nHost: 127.0.0.1\r\nX: " + b"a" * 9000 + b"\r\n\r\n")
    assert s.recv(64).startswith(b"HTTP/1.1 413")
    s.close()
    assert server.http("POST", "/api/pair/start", b"{" + b" " * 5000 + b"}")[0] == 413
    assert server.http("POST", "/api/pair/start", {"name": "x"})[0] == 400
    assert server.http("POST", "/api/pair/start", {"name": "x", "v_pub": "zz" * 32, "commit": "00" * 32})[0] == 400


def test_a_silent_connection_is_dropped_and_never_blocks_others(server):
    s = socket.create_connection(("127.0.0.1", server.port), timeout=10)
    s.sendall(b"GET / HTTP/1.1\r\n")  # and nothing more
    t0 = time.time()
    assert server.http("GET", "/api/info")[0] == 200, "others are served meanwhile"
    got = s.recv(64)
    assert got.startswith(b"HTTP/1.1 400") and time.time() - t0 <= 7
    s.close()


@needs_lan
def test_one_address_cannot_hold_every_connection(server):
    held = []
    for _ in range(24):
        held.append(socket.create_connection((LAN, server.port), timeout=5))
    extra = socket.create_connection((LAN, server.port), timeout=5)
    extra.sendall(f"GET /api/info HTTP/1.1\r\nHost: {LAN}\r\n\r\n".encode())
    extra.settimeout(3)
    try:
        refused = extra.recv(64) == b""
    except ConnectionResetError:
        refused = True
    assert refused, "the 25th connection from one address is closed"
    assert server.http("GET", "/api/info")[0] == 200, "loopback (the owner's window) is not held up by it"
    for h in held:
        h.close()
    time.sleep(0.3)
    assert server.http("GET", "/api/info", ip=LAN)[0] == 200, "and served again once they close"


def test_pairing_requests_are_bounded_and_a_wrong_reveal_ends_one(server):
    a, b = Device(server), Device(server)
    a.ask()
    b.ask()
    st, r = server.http("POST", "/api/pair/start", {"name": "c", "v_pub": "11" * 32, "commit": "22" * 32})
    assert (st, r) == (429, {"error": "busy"}), "two waiting from one address"
    server.owner_api("POST", "/api/owner/deny", {"req": a.req})
    st, r = server.http("POST", "/api/pair/start", {"name": "c", "v_pub": "11" * 32, "commit": "22" * 32})
    assert st == 200
    st, _ = server.http("POST", "/api/pair/reveal", {"req": r["req"], "n_v": "00" * 16})
    assert st == 403
    assert server.http("GET", f"/api/pair/status?req={r['req']}")[0] == 404, "no second guess"


def test_a_paused_computer_takes_no_new_requests_and_lets_nobody_watch(server):
    d = Device(server)
    d.pair()
    server.owner_api("POST", "/api/owner/pause", {"on": True})
    assert server.http("POST", "/api/pair/start", {"name": "x", "v_pub": "11" * 32, "commit": "22" * 32})[0] == 503
    ws, _, r = d.hello()
    assert r == {"t": "error", "reason": "paused"}
    server.owner_api("POST", "/api/owner/pause", {"on": False})
    ws2, _, r2 = d.hello()
    assert r2["t"] == "ready"


@needs_lan
def test_the_answer_to_a_request_is_only_for_the_address_that_asked(server):
    d = Device(server, ip=LAN)
    req, _ = d.ask()
    assert server.http("GET", f"/api/pair/status?req={req}")[0] == 404
    assert server.http("GET", f"/api/pair/status?req={req}", ip=LAN)[0] == 200


def test_every_response_carries_the_security_headers(server):
    import urllib.request
    with urllib.request.urlopen(server.base + "/") as r:
        h = r.headers
    assert "default-src 'self'" in h["Content-Security-Policy"] and "frame-ancestors 'none'" in h["Content-Security-Policy"]
    assert h["X-Content-Type-Options"] == "nosniff" and h["X-Frame-Options"] == "DENY" and h["Referrer-Policy"] == "no-referrer"
