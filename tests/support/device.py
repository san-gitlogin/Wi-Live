"""A device written in Python with an independent crypto library (`cryptography`):
pairs and proves itself over raw HTTP and a raw WebSocket. A second implementation
of the protocol, so the Rust and JS sides are checked against something that is
neither of them."""
import base64
import hashlib
import hmac
import json
import os
import socket
import struct

from cryptography.hazmat.primitives import hashes
from cryptography.hazmat.primitives.asymmetric.x25519 import X25519PrivateKey, X25519PublicKey
from cryptography.hazmat.primitives.kdf.hkdf import HKDF
from cryptography.hazmat.primitives.serialization import Encoding, PublicFormat

H = lambda *p: hashlib.sha256(b"".join(p)).digest()


class WS:
    """Just enough RFC 6455 for text frames."""

    def __init__(self, port, ip="127.0.0.1", origin=True, host=None, timeout=8):
        self.s = socket.create_connection((ip, port), timeout=timeout)
        key = base64.b64encode(os.urandom(16)).decode()
        host = host or f"{ip}:{port}"
        req = f"GET /v1/signal HTTP/1.1\r\nHost: {host}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\n"
        if origin:
            req += f"Origin: http://{host}\r\n"
        self.s.sendall((req + "\r\n").encode())
        head = b""
        while b"\r\n\r\n" not in head:
            c = self.s.recv(1)
            if not c:
                break
            head += c
        self.status = int(head.split(b" ")[1]) if head else 0
        self.buf = b""

    def send(self, obj):
        data = json.dumps(obj).encode()
        mask = os.urandom(4)
        n = len(data)
        hdr = bytes([0x81]) + (bytes([0x80 | n]) if n < 126 else bytes([0x80 | 126]) + struct.pack(">H", n))
        self.s.sendall(hdr + mask + bytes(b ^ mask[i % 4] for i, b in enumerate(data)))

    def _read(self, n):
        while len(self.buf) < n:
            c = self.s.recv(65536)
            if not c:
                raise ConnectionError("closed")
            self.buf += c
        out, self.buf = self.buf[:n], self.buf[n:]
        return out

    def recv(self):
        while True:
            b0, b1 = self._read(2)
            n = b1 & 0x7F
            if n == 126:
                n = struct.unpack(">H", self._read(2))[0]
            elif n == 127:
                n = struct.unpack(">Q", self._read(8))[0]
            data = self._read(n)
            op = b0 & 0x0F
            if op == 1:
                return json.loads(data)
            if op == 8:
                raise ConnectionError("closed")

    def close(self):
        try:
            self.s.close()
        except OSError:
            pass


class Device:
    def __init__(self, server, name="Python device", ip="127.0.0.1"):
        self.server, self.name, self.ip = server, name, ip
        self.id = self.key = None

    def ask(self):
        """Starts and reveals; returns (req, code). The owner must still allow."""
        sk = X25519PrivateKey.generate()
        self.v_pub = sk.public_key().public_bytes(Encoding.Raw, PublicFormat.Raw)
        n_v = os.urandom(16)
        st, r = self.server.http("POST", "/api/pair/start", {"name": self.name, "v_pub": self.v_pub.hex(), "commit": H(b"wl-commit", self.v_pub, n_v).hex()}, ip=self.ip)
        assert st == 200, (st, r)
        a_pub, n_a = bytes.fromhex(r["a_pub"]), bytes.fromhex(r["n_a"])
        st, r2 = self.server.http("POST", "/api/pair/reveal", {"req": r["req"], "n_v": n_v.hex()}, ip=self.ip)
        assert st == 200, (st, r2)
        shared = sk.exchange(X25519PublicKey.from_public_bytes(a_pub))
        self.key = HKDF(hashes.SHA256(), 32, H(self.v_pub, a_pub, n_v, n_a), b"wilive-device-v1").derive(shared)
        d = H(b"wl-sas", self.v_pub, a_pub, n_v, n_a)
        self.req = r["req"]
        return r["req"], f"{int.from_bytes(d[:4], 'big') % 1000000:06d}"

    def pair(self):
        req, code = self.ask()
        st, _ = self.server.owner_api("POST", "/api/owner/allow", {"req": req})
        assert st == 200
        st, r = self.server.http("GET", f"/api/pair/status?req={req}", ip=self.ip)
        assert st == 200 and r["state"] == "allowed", r
        self.id = r["device"]
        return code

    def mac(self, chal, id=None, key=None):
        return hmac.new(key or self.key, f"wl-hello|{id or self.id}|{chal}".encode(), hashlib.sha256).hexdigest()

    def hello(self, ws=None, mac=None):
        """Opens the signaling socket and proves itself; returns (ws, chal, reply)."""
        ws = ws or WS(self.server.port, self.ip)
        chal = ws.recv()["c"]
        ws.send({"t": "hello", "id": self.id, "mac": mac or self.mac(chal)})
        return ws, chal, ws.recv()

    def fp_ok(self, chal, fp, fp_mac):
        return hmac.compare_digest(hmac.new(self.key, f"wl-fp|{chal}|{fp}".encode(), hashlib.sha256).hexdigest(), fp_mac)
