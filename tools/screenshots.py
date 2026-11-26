#!/usr/bin/env python3
"""The README's screenshots (docs/assets/screens), made the same way every time.

The "screen" being shared is a made-up desktop (docs/assets/demo-desktop.html, rendered
to a PNG and served by the test server's WILIVE_DEMO_IMAGE) - never anyone's real screen.
Phones are WebKit with an iPhone's size; the owner's window is Chrome.

    cargo build --release -p wilive-server --example serve && python3 tools/screenshots.py
"""
import pathlib
import sys

ROOT = pathlib.Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "tests"))
from playwright.sync_api import sync_playwright  # noqa: E402
from support.pages import pair, wait  # noqa: E402
from support.server import Server  # noqa: E402

OUT = ROOT / "docs" / "assets" / "screens"
DEMO = ROOT / "docs" / "assets" / "demo-desktop.png"


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    with sync_playwright() as p:
        chrome = p.chromium.launch(channel="chrome", args=["--autoplay-policy=no-user-gesture-required"])
        pg = chrome.new_page(viewport={"width": 1920, "height": 1080})
        pg.goto((ROOT / "docs" / "assets" / "demo-desktop.html").as_uri())
        pg.wait_for_timeout(500)
        pg.screenshot(path=str(DEMO))
        pg.close()
        s = Server(args=("1920", "1080", "30"), env={"WILIVE_DEMO_IMAGE": str(DEMO), "WILIVE_TEST_HOST": "1",
                                                      "WILIVE_NAME": "Studio MacBook"})
        try:
            s.owner_api("POST", "/api/owner/capture", {})  # the test host starts refused
            webkit = p.webkit.launch()
            iphone = dict(p.devices["iPhone 13"])
            iphone.pop("default_browser_type", None)
            for scheme in ("light", "dark"):
                octx = chrome.new_context(viewport={"width": 1100, "height": 860}, color_scheme=scheme, device_scale_factor=2)
                octx.add_init_script(f"window.WILIVE_OWNER = {s.owner!r};")
                o = octx.new_page()
                o.goto(s.base + "/")
                wait(o, "!!window.__wl.owner")
                phone = webkit.new_context(**iphone, color_scheme=scheme)
                v = phone.new_page()
                v.goto(s.base + "/")
                wait(v, "window.__wl.state === 'pair'")
                v.fill("#devName", "My iPhone")
                if scheme == "light":
                    v.screenshot(path=str(OUT / "01-phone-ask.png"))
                v.click("#btnAsk")
                wait(v, "window.__wl.state === 'code'")
                o.wait_for_selector(".ask")
                o.wait_for_timeout(400)
                if scheme == "light":
                    v.screenshot(path=str(OUT / "02-phone-code.png"))
                    o.screenshot(path=str(OUT / "03-owner-asks.png"))
                o.click(".ask [data-act=allow]")
                wait(v, "window.__wl.state === 'live'", 20)
                v.set_viewport_size({"width": 844, "height": 390})
                v.wait_for_function("() => document.getElementById('video').videoWidth >= 1280", timeout=20000)
                v.wait_for_timeout(1200)
                v.mouse.move(400, 200)
                v.screenshot(path=str(OUT / f"04-phone-watching-{scheme}.png"))
                v.click("#btnMore")
                v.wait_for_timeout(300)
                if scheme == "dark":
                    v.screenshot(path=str(OUT / "05-phone-menu.png"))
                o.wait_for_timeout(1500)
                o.screenshot(path=str(OUT / f"06-owner-{scheme}.png"))
                phone.close()
                octx.close()
            tab = chrome.new_context(viewport={"width": 1180, "height": 820}, device_scale_factor=2)
            t = tab.new_page()
            octx = chrome.new_context()
            octx.add_init_script(f"window.WILIVE_OWNER = {s.owner!r};")
            o = octx.new_page()
            o.goto(s.base + "/")
            t.goto(s.base + "/")
            pair(o, t, "Living room tablet")
            t.wait_for_function("() => document.getElementById('video').videoWidth >= 1280", timeout=20000)
            t.wait_for_timeout(1200)
            t.mouse.move(600, 300)
            t.screenshot(path=str(OUT / "07-tablet-watching.png"))
            webkit.close()
        finally:
            s.stop()
        chrome.close()
    for f in sorted(OUT.glob("*.png")):
        print(f.relative_to(ROOT))


if __name__ == "__main__":
    main()
