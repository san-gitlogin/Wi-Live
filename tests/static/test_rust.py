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
