#!/usr/bin/env python3
"""THE test command for Wi-Live.

    python tests/run.py            static + unit (no browser, a minute or two)
    python tests/run.py --all      + e2e and latency in headless Google Chrome
    python tests/run.py --tier e2e one tier: static | unit | e2e | latency

Headless Chrome draws nothing on the desktop and types nothing: --all is safe to run
while someone uses this computer. Results of the latency tier are appended to
tests/.out/latency.jsonl.
"""
import argparse
import pathlib
import subprocess
import sys
import time

ROOT = pathlib.Path(__file__).resolve().parents[1]
TIERS = {"static": "static", "unit": "unit", "e2e": "e2e and not latency", "latency": "latency"}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--all", action="store_true")
    ap.add_argument("--tier", choices=TIERS)
    a, rest = ap.parse_known_args()
    tiers = [a.tier] if a.tier else (list(TIERS) if a.all else ["static", "unit"])
    rows, ok = [], True
    for t in tiers:
        t0 = time.time()
        r = subprocess.run([sys.executable, "-m", "pytest", "-q", "-p", "no:cacheprovider", "-m", TIERS[t], *rest], cwd=ROOT)
        rows.append((t, r.returncode, time.time() - t0))
        ok &= r.returncode in (0, 5)
    print("\n tier       result   time")
    for t, rc, s in rows:
        print(f" {t:<10} {'PASS' if rc == 0 else 'empty' if rc == 5 else 'FAIL':<8} {s:5.0f} s")
    print(" PASSED" if ok else " FAILED")
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
