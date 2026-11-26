"""The test server's made-up moving screen (WILIVE_DEMO_FRAMES), used to film Wi-Live.

2026-11-26: the film's "Auto" take showed a 3840x2160 picture, which the product never
sends (Auto caps the long edge at 1920 in the real capture). An "auto|sharp" entry now
names the same screen at two sizes, and the source opens the one the real capture would."""
import struct
import zlib

import pytest

from support.pages import owner_page, pair, viewer_page

pytestmark = pytest.mark.e2e


def png(path, w, h, rgb):
    raw = b"".join(b"\x00" + bytes(rgb) * w for _ in range(h))
    def chunk(t, d):
        return struct.pack(">I", len(d)) + t + d + struct.pack(">I", zlib.crc32(t + d) & 0xFFFFFFFF)
    path.write_bytes(b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0))
                     + chunk(b"IDAT", zlib.compress(raw, 1)) + chunk(b"IEND", b""))


def frames(d, w, h, n=6):
    d.mkdir()
    for i in range(n):
        png(d / f"f{i:05d}.png", w, h, (40 * i % 255, 90, 160))
    return d


def size(v):
    return v.evaluate("() => [document.getElementById('video').videoWidth, document.getElementById('video').videoHeight]")


def test_auto_stays_at_1920_and_sharp_sends_the_full_4k_screen(chrome, make_server, tmp_path):
    auto = frames(tmp_path / "auto", 1920, 1080)
    sharp = frames(tmp_path / "sharp", 3840, 2160)
    s = make_server("3840", "2160", "30", WILIVE_DEMO_FRAMES=f"{auto}|{sharp}", WILIVE_DISPLAY_SIZES="3840x2160")
    o = owner_page(chrome, s)
    v = viewer_page(chrome, s)
    pair(o, v, "Film phone")
    v.wait_for_function("() => document.getElementById('video').videoWidth > 0", timeout=15000)
    assert size(v) == [1920, 1080], f"Auto must cap the long edge at 1920, got {size(v)}"
    v.evaluate("() => document.querySelector('#qualitySeg [data-q=sharp]').click()")
    v.wait_for_function("() => document.getElementById('video').videoWidth === 3840", timeout=20000)
    assert size(v) == [3840, 2160]
    v.evaluate("() => document.querySelector('#qualitySeg [data-q=auto]').click()")
    v.wait_for_function("() => document.getElementById('video').videoWidth === 1920", timeout=20000)
