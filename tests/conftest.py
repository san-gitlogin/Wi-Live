import os
import sys
import pathlib

import pytest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
from support.server import Server  # noqa: E402


@pytest.fixture(scope="session")
def pw():
    from playwright.sync_api import sync_playwright
    with sync_playwright() as p:
        yield p


@pytest.fixture(scope="session")
def chrome(pw):
    # Real Google Chrome: Playwright's own Chromium has no H.264 decoder.
    b = pw.chromium.launch(channel="chrome", args=["--autoplay-policy=no-user-gesture-required"])
    yield b
    b.close()


@pytest.fixture
def server():
    s = Server()
    yield s
    s.stop()


@pytest.fixture
def make_server():
    made = []

    def make(*args, **env):
        s = Server(args=args or ("1280", "720", "60"), env={k: str(v) for k, v in env.items()})
        made.append(s)
        return s
    yield make
    for s in made:
        s.stop()


@pytest.fixture(autouse=True)
def _close_pages():
    from support import pages
    yield
    while pages.OPEN:
        try:
            pages.OPEN.pop().close()
        except Exception:
            pass
