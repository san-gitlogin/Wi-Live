"""Starting the real server (the `serve` example: the whole app with a test picture)."""
import json
import os
import pathlib
import socket
import subprocess
import time
import urllib.request

ROOT = pathlib.Path(__file__).resolve().parents[2]
# WILIVE_SERVE_EXE: a server built elsewhere (the Windows VM gets one built on the Mac).
EXE = pathlib.Path(os.environ["WILIVE_SERVE_EXE"]) if os.environ.get("WILIVE_SERVE_EXE") else \
    ROOT / "target" / "release" / "examples" / ("serve.exe" if os.name == "nt" else "serve")
_built = [bool(os.environ.get("WILIVE_SERVE_EXE"))]


def build():
    if not _built[0]:
        subprocess.run(["cargo", "build", "--release", "-q", "-p", "wilive-server", "--example", "serve"], cwd=ROOT, check=True)
        _built[0] = True


def lan_ip():
    """This computer's home-network address, or loopback when it has none."""
    s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    try:
        s.connect(("192.168.255.255", 9))
        ip = s.getsockname()[0]
    except OSError:
        ip = "127.0.0.1"
    finally:
        s.close()
    import ipaddress
    a = ipaddress.ip_address(ip)
    return ip if a.is_private and not ip.startswith("192.0.0.") else "127.0.0.1"


class Server:
    def __init__(self, args=("1280", "720", "60"), env=None):
        build()
        e = dict(os.environ, **(env or {}))
        out = ROOT / "tests" / ".out"
        out.mkdir(exist_ok=True)
        # The server's log, kept for the run that just failed (tests/.out/serve.log).
        self.log_path = out / "serve.log"
        self._log = open(self.log_path, "w")
        self.proc = subprocess.Popen([str(EXE), *args], cwd=ROOT, env=e, stdin=subprocess.PIPE,
                                     stdout=subprocess.PIPE, stderr=self._log, text=True)
        line = self.proc.stdout.readline()
        if not line:
            raise RuntimeError("serve did not start: " + self.log_path.read_text()[-2000:])
        info = json.loads(line)
        self.port, self.owner, self.fp = info["port"], info["owner"], info["fp"]
        self.base = f"http://127.0.0.1:{self.port}"

    def say(self, line):
        self.proc.stdin.write(line + "\n")
        self.proc.stdin.flush()

    def http(self, method, path, body=None, headers=None, host=None, ip="127.0.0.1", timeout=5):
        """(status, json-or-text). Sends a matching Origin on POST unless told otherwise."""
        h = {"Host": host or f"{ip}:{self.port}"}
        if body is not None:
            h["Content-Type"] = "application/json"
            h.setdefault("Origin", f"http://{h['Host']}")
        h.update(headers or {})
        data = None if body is None else (body if isinstance(body, bytes) else json.dumps(body).encode())
        req = urllib.request.Request(f"http://{ip}:{self.port}{path}", data=data, method=method, headers=h)
        try:
            with urllib.request.urlopen(req, timeout=timeout) as r:
                raw, status = r.read(), r.status
        except urllib.error.HTTPError as e:
            raw, status = e.read(), e.code
        try:
            return status, json.loads(raw)
        except ValueError:
            return status, raw.decode("utf-8", "replace")

    def owner_api(self, method, path, body=None):
        return self.http(method, path, body, headers={"X-Wilive-Owner": self.owner})

    def cpu(self):
        if os.name == "nt":
            return 0.0
        out = subprocess.run(["ps", "-o", "%cpu=", "-p", str(self.proc.pid)], capture_output=True, text=True).stdout.strip()
        return float(out or 0)

    def log(self):
        self._log.flush()
        return self.log_path.read_text()

    def stop(self):
        self.proc.terminate()
        try:
            self.proc.wait(5)
        except subprocess.TimeoutExpired:
            self.proc.kill()
