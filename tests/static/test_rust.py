"""The Rust source, checked exactly as CI checks it."""
import pathlib
import re
import subprocess

import pytest

pytestmark = pytest.mark.static
ROOT = pathlib.Path(__file__).resolve().parents[2]


def test_rust_is_formatted():
    r = subprocess.run(["cargo", "fmt", "--all", "--check"], cwd=ROOT, capture_output=True, text=True)
    assert r.returncode == 0, r.stdout[-3000:]


def test_clippy_is_clean_as_ci_runs_it():
    """HID-Fi v4.5's first CI run failed on two lints nobody ran locally."""
    r = subprocess.run(["cargo", "clippy", "--workspace", "--all-targets", "-q", "--", "-D", "warnings"], cwd=ROOT, capture_output=True, text=True)
    assert r.returncode == 0, r.stderr[-4000:]


def test_no_secret_is_ever_logged():
    """Keys, proofs, codes and the owner token never go to a log line (R-29)."""
    bad = []
    for f in (ROOT / "crates").rglob("*.rs"):
        for i, line in enumerate(f.read_text().splitlines(), 1):
            if re.search(r"\b(log::\w+!|println!|eprintln!)", line) and re.search(r"owner_token|\btoken\b|\bkey\b|DeviceKey|\bmac\b|fp_mac|\bcode\b", line):
                if "keyframe" in line or "examples" in f.parts:
                    continue
                bad.append(f"{f.relative_to(ROOT)}:{i}: {line.strip()}")
    assert not bad, "\n".join(bad)


def _installed(target):
    r = subprocess.run(["rustup", "target", "list", "--installed"], capture_output=True, text=True)
    return target in r.stdout


@pytest.mark.parametrize("target", ["x86_64-pc-windows-msvc", "x86_64-unknown-linux-gnu"])
def test_clippy_is_clean_for_the_other_platforms(target):
    """Windows and Linux code compiles only there; checked from here so a Mac-only
    change cannot break them unseen (Linux uses Zig as its C compiler, tools/)."""
    if not _installed(target):
        pytest.skip(f"rustup target {target} not installed")
    host = subprocess.run(["rustc", "-vV"], capture_output=True, text=True).stdout
    if f"host: {target}" in host:
        pytest.skip("this machine's own target: the plain clippy test covers it")
    import os
    env = dict(os.environ)
    if "linux" in target:
        env["CC_x86_64_unknown_linux_gnu"] = str(ROOT / "tools/zigcc-linux.sh")
        env["AR_x86_64_unknown_linux_gnu"] = str(ROOT / "tools/zigar.sh")
    r = subprocess.run(["cargo", "clippy", "--workspace", "--all-targets", "--target", target, "-q", "--", "-D", "warnings"], cwd=ROOT, capture_output=True, text=True, env=env)
    assert r.returncode == 0, r.stderr[-4000:]
