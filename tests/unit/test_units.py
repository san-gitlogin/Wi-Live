"""Every Rust unit test, and the page's crypto against the RFC vectors and the Rust bytes."""
import pathlib
import subprocess

import pytest

pytestmark = pytest.mark.unit
ROOT = pathlib.Path(__file__).resolve().parents[2]


def test_rust_workspace_tests_pass():
    r = subprocess.run(["cargo", "test", "--workspace", "-q"], cwd=ROOT, capture_output=True, text=True)
    assert r.returncode == 0, (r.stdout + r.stderr)[-5000:]


def test_page_crypto_matches_rfc_vectors_and_rust():
    r = subprocess.run(["node", str(ROOT / "tests/unit/crypto_check.js")], capture_output=True, text=True)
    assert r.returncode == 0, r.stdout[-3000:]


@pytest.mark.parametrize("bug", [
    ("'wl-commit'", "'wl-comit'"),
    ("return sha256(cat(op, sha256(cat(ip, msg))));", "return sha256(cat(ip, sha256(cat(op, msg))));"),
    ("k[0] &= 248;", "k[0] &= 255;"),
    ("% 1000000", "% 999999"),
    ("utf8('wilive-device-v1')", "utf8('wilive-device-v2')"),
    ("'wl-hello|' + id + '|' + chal", "'wl-hello|' + chal + '|' + id"),
    ("if (a.length !== b.length) return false;", ""),
])
def test_a_planted_crypto_bug_is_caught(tmp_path, bug):
    """The checks must fail when the crypto is wrong, not only pass when it is right."""
    src = (ROOT / "web/crypto.js").read_text()
    assert src.count(bug[0]) == 1, f"'{bug[0]}' must appear exactly once"
    bad = tmp_path / "crypto.js"
    bad.write_text(src.replace(bug[0], bug[1]))
    r = subprocess.run(["node", str(ROOT / "tests/unit/crypto_check.js")], capture_output=True, text=True, env={"WLC_PATH": str(bad), "PATH": "/usr/bin:/bin:/opt/homebrew/bin:/usr/local/bin"})
    assert r.returncode != 0, f"planted bug survived: {bug}"
