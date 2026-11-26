"""Updating the app itself, for real, on Windows (crates/wilive/src/updater.rs), in the
Windows VM (tests/windows). A test build (feature test-key) at C:\\wilive\\upd\\app runs
as "0.0.9"; a release folder served here offers "9.9.9", signed with the published TEST
key, whose Windows file is a real Wi-Live build. It must see the offer, install it (the
.exe swapped, the old kept beside it), and start the new copy, which takes over."""
import hashlib, http.server, json, os, pathlib, shutil, subprocess, sys, tempfile, threading, time, urllib.request
from cryptography.hazmat.primitives import hashes
from cryptography.hazmat.primitives.asymmetric import ec

BASE = pathlib.Path(r"C:\wilive\upd")
APP, NEW = BASE / "app" / "wilive.exe", BASE / "new" / "wilive.exe"
TOKEN = "e2e" + "0" * 45
ok = True
def check(name, cond, detail=""):
    global ok
    ok &= bool(cond); print(("ok  " if cond else "FAIL"), name, detail, flush=True)

rel = pathlib.Path(tempfile.mkdtemp())
shutil.copy(NEW, rel / "wilive-windows-x86_64.exe")
(rel / "Wi-Live-9.9.9-macos.dmg").write_bytes(b"not used on Windows")
h = lambda p: hashlib.sha256(p.read_bytes()).hexdigest()
sums = "".join(f"{h(rel / n)}  {n}\n" for n in ("Wi-Live-9.9.9-macos.dmg", "wilive-windows-x86_64.exe"))
(rel / "SHA256SUMS").write_text(sums, newline="\n")
scalar = bytes(0x57 ^ ((i * 29) & 0xFF) for i in range(32))
key = ec.derive_private_key(int.from_bytes(scalar, "big"), ec.SECP256R1())
(rel / "SHA256SUMS.sig").write_bytes(key.sign(sums.encode(), ec.ECDSA(hashes.SHA256())))
handler = lambda *a, **k: http.server.SimpleHTTPRequestHandler(*a, directory=str(rel), **k)
httpd = http.server.ThreadingHTTPServer(("127.0.0.1", 0), handler)
httpd.RequestHandlerClass.log_message = lambda *a: None
threading.Thread(target=httpd.serve_forever, daemon=True).start()

home = pathlib.Path(tempfile.mkdtemp())
env = dict(os.environ, WILIVE_HOME=str(home), WILIVE_NO_AUTOSTART="1", WILIVE_RELEASES=f"http://127.0.0.1:{httpd.server_port}/",
           WILIVE_TEST_VERSION="0.0.9", WILIVE_TEST_OWNER=TOKEN, WILIVE_TEST_NO_WINDOW="1")
p = subprocess.Popen([str(APP)], env=env)
port = None
for _ in range(150):
    time.sleep(0.2)
    log = (home / "wilive.log").read_text() if (home / "wilive.log").exists() else ""
    if "listening on port" in log:
        port = int(log.split("listening on port")[-1].split()[0]); break
check("the test build starts", port, f"port {port}")
def owner(path, body=None):
    r = urllib.request.Request(f"http://127.0.0.1:{port}{path}", data=None if body is None else json.dumps(body).encode(),
        method="GET" if body is None else "POST", headers={"X-Wilive-Owner": TOKEN, "Origin": f"http://127.0.0.1:{port}", "Content-Type": "application/json"})
    with urllib.request.urlopen(r, timeout=5) as x:
        return json.loads(x.read())
u = {}
for _ in range(60):
    u = owner("/api/owner/state")["update"]
    if u["state"] in ("available", "failed"): break
    time.sleep(0.5)
check("9.9.9 is offered (signature checked)", u.get("state") == "available" and u.get("detail") == "9.9.9", u)
want = h(NEW)
owner("/api/owner/update", {"act": "install"})
try:
    code = p.wait(90)
except subprocess.TimeoutExpired:
    code = None
check("the old copy leaves after installing", code == 0, f"exit {code}")
check("the new .exe is in place", h(APP) == want)
check("the old one is kept beside it", (APP.parent / "wilive.exe.old").exists())
check("installed, in the log", "installed 9.9.9" in (home / "wilive.log").read_text())
started = ""
for _ in range(40):
    out = subprocess.run(["powershell", "-NoProfile", "-Command",
        "Get-CimInstance Win32_Process -Filter \"Name='wilive.exe'\" | ForEach-Object { $_.ProcessId.ToString() + ' ' + $_.CommandLine }"], capture_output=True, text=True).stdout
    started = next((l for l in out.splitlines() if "--after-update" in l and str(APP.parent).lower() in l.lower()), "")
    if started: break
    time.sleep(0.5)
check("the new copy started by itself", started, started[:120])
time.sleep(3)
check("it took over (its own start in the log)", "just updated" in (home / "wilive.log").read_text())
if started:
    subprocess.run(["taskkill", "/F", "/PID", started.split()[0]], capture_output=True)
httpd.shutdown()
print("ALL PASSED" if ok else "FAILED"); sys.exit(0 if ok else 1)
