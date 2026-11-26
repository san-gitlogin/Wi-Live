"""Updating the app itself, for real, on macOS (crates/wilive/src/updater.rs).

A test build of the app (feature test-key: it also takes WILIVE_TEST_VERSION and
WILIVE_TEST_OWNER, and never opens its window) runs from a temporary Wi-Live.app,
pretending to be 0.0.9. A local release folder serves the real signed v0.1.0 release
(dist/v0.1.0 from tools/make_release.sh). It must:
    see 0.1.0 offered (the signature checked against the built-in release key),
    install it when the owner asks: the app in its place carries Wi-Live's certificate,
    refuse a tampered list, and a download that does not match it, leaving itself whole.
Skipped without dist/v0.1.0 (made by tools/make_release.sh) or on another OS.
"""
import http.server
import json
import os
import pathlib
import plistlib
import shutil
import subprocess
import sys
import tempfile
import threading
import time
import urllib.request

import pytest

pytestmark = [pytest.mark.e2e, pytest.mark.skipif(sys.platform != "darwin", reason="macOS update path")]
ROOT = pathlib.Path(__file__).resolve().parents[2]
REL = ROOT / "dist" / "v0.1.0"
TOKEN = "e2e" + "0" * 45


@pytest.fixture(scope="module")
def test_app():
    if not (REL / "Wi-Live-0.1.0-macos.dmg").exists():
        pytest.skip("no dist/v0.1.0 (tools/make_release.sh 0.1.0 <CI artifacts>)")
    subprocess.run(["cargo", "build", "--release", "-q", "-p", "wilive", "--features", "test-key"], cwd=ROOT, check=True)
    exe = ROOT / "target" / "release" / "wilive"
    data = exe.read_bytes()
    yield data
    # put the normal build back, so nothing later ships or runs the test build
    subprocess.run(["cargo", "build", "--release", "-q", "-p", "wilive"], cwd=ROOT, check=True)


def make_bundle(where, exe_bytes):
    app = where / "Wi-Live.app"
    (app / "Contents" / "MacOS").mkdir(parents=True)
    (app / "Contents" / "MacOS" / "wilive").write_bytes(exe_bytes)
    os.chmod(app / "Contents" / "MacOS" / "wilive", 0o755)
    plistlib.dump({"CFBundleIdentifier": "live.wilive.app", "CFBundleExecutable": "wilive", "CFBundleName": "Wi-Live",
                   "CFBundleShortVersionString": "0.0.9", "LSUIElement": True}, open(app / "Contents" / "Info.plist", "wb"))
    subprocess.run(["codesign", "--force", "--sign", "-", str(app)], check=True, capture_output=True)
    return app


def serve(folder):
    h = lambda *a, **k: http.server.SimpleHTTPRequestHandler(*a, directory=str(folder), **k)
    httpd = http.server.ThreadingHTTPServer(("127.0.0.1", 0), h)
    httpd.RequestHandlerClass.log_message = lambda *a: None
    threading.Thread(target=httpd.serve_forever, daemon=True).start()
    return httpd


def owner(port, path, body=None):
    req = urllib.request.Request(f"http://127.0.0.1:{port}{path}", data=None if body is None else json.dumps(body).encode(),
                                 method="GET" if body is None else "POST",
                                 headers={"X-Wilive-Owner": TOKEN, "Host": f"127.0.0.1:{port}", "Origin": f"http://127.0.0.1:{port}",
                                          "Content-Type": "application/json"})
    with urllib.request.urlopen(req, timeout=5) as r:
        return json.loads(r.read())


def run_app(app, releases, home):
    env = dict(os.environ, WILIVE_HOME=str(home), WILIVE_NO_AUTOSTART="1", WILIVE_RELEASES=releases,
               WILIVE_TEST_VERSION="0.0.9", WILIVE_TEST_OWNER=TOKEN, WILIVE_TEST_NO_WINDOW="1")
    p = subprocess.Popen([str(app / "Contents" / "MacOS" / "wilive")], env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    port = None
    for _ in range(100):
        time.sleep(0.2)
        log = (home / "wilive.log").read_text() if (home / "wilive.log").exists() else ""
        if "listening on port" in log:
            port = int(log.split("listening on port")[-1].split()[0])
            break
    assert port, "the app did not start"
    return p, port


def wait_update(port, want, secs=30):
    end = time.time() + secs
    u = {}
    while time.time() < end:
        u = owner(port, "/api/owner/state")["update"]
        if u["state"] == want:
            return u
        time.sleep(0.5)
    raise AssertionError(f"update state {u}, wanted {want}")


def designated(app):
    out = subprocess.run(["codesign", "-d", "-r-", str(app)], capture_output=True, text=True)
    return out.stdout + out.stderr


def test_a_signed_release_is_offered_installed_and_carries_wi_lives_certificate(test_app, tmp_path):
    app = make_bundle(tmp_path, test_app)
    httpd = serve(REL)
    p, port = run_app(app, f"http://127.0.0.1:{httpd.server_port}/", tmp_path / "home")
    try:
        u = wait_update(port, "available")
        assert u["detail"] == "0.1.0" and u["current"] == "0.0.9"
        owner(port, "/api/owner/update", {"act": "install"})
        assert p.wait(60) == 0, "the old copy leaves after installing"
    finally:
        if p.poll() is None:
            p.kill()
        httpd.shutdown()
    assert 'certificate root = H"7ec828ea86b744681e2c1456c700112e77a9086c"' in designated(app), "Wi-Live's certificate on the installed app"
    assert plistlib.load(open(app / "Contents" / "Info.plist", "rb"))["CFBundleShortVersionString"] == "0.1.0"
    assert (tmp_path / "Wi-Live.app.old").exists(), "the copy before the update stays beside it until the next start"
    assert "installed 0.1.0" in (tmp_path / "home" / "wilive.log").read_text()
    # The new copy starts by itself (`open -n ... --after-update`): found, then stopped.
    new = None
    for _ in range(40):
        out = subprocess.run(["pgrep", "-f", f"{app}/Contents/MacOS/wilive --after-update"], capture_output=True, text=True).stdout.split()
        if out:
            new = out[0]
            break
        time.sleep(0.5)
    assert new, "the updated app started by itself"
    subprocess.run(["kill", new])


def test_a_tampered_list_is_refused_and_nothing_is_offered(test_app, tmp_path):
    app = make_bundle(tmp_path, test_app)
    rel = tmp_path / "rel"
    shutil.copytree(REL, rel)
    sums = (rel / "SHA256SUMS").read_text().replace("a", "b", 1)
    (rel / "SHA256SUMS").write_text(sums)
    httpd = serve(rel)
    p, port = run_app(app, f"http://127.0.0.1:{httpd.server_port}/", tmp_path / "home")
    try:
        u = wait_update(port, "failed")
        assert "BadSignature" in u["detail"]
    finally:
        p.kill()
        httpd.shutdown()


def test_a_download_that_does_not_match_the_signed_list_installs_nothing(test_app, tmp_path):
    app = make_bundle(tmp_path, test_app)
    before = (app / "Contents" / "MacOS" / "wilive").read_bytes()
    rel = tmp_path / "rel"
    shutil.copytree(REL, rel)
    dmg = rel / "Wi-Live-0.1.0-macos.dmg"
    b = bytearray(dmg.read_bytes())
    b[len(b) // 2] ^= 0xFF
    dmg.write_bytes(bytes(b))
    httpd = serve(rel)
    p, port = run_app(app, f"http://127.0.0.1:{httpd.server_port}/", tmp_path / "home")
    try:
        wait_update(port, "available")
        owner(port, "/api/owner/update", {"act": "install"})
        u = wait_update(port, "failed", 60)
        assert "does not match" in u["detail"]
        assert p.poll() is None, "still running: nothing was swapped"
    finally:
        p.kill()
        httpd.shutdown()
    assert (app / "Contents" / "MacOS" / "wilive").read_bytes() == before, "the app is untouched"
