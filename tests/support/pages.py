"""The owner's window and viewers, as pages in real Chrome."""
import time

# Every context a test opens, closed after it (conftest): a page left open keeps
# reconnecting to the next test's server on the same port.
OPEN = []


def owner_page(chrome, server):
    """This computer's window: the dashboard on loopback with the owner token, as
    the app's web view gets it."""
    ctx = chrome.new_context()
    OPEN.append(ctx)
    ctx.add_init_script(f"window.WILIVE_OWNER = {server.owner!r};")
    pg = ctx.new_page()
    errors = []
    pg.on("pageerror", lambda e: errors.append(str(e)))
    pg.goto(server.base + "/")
    pg.wait_for_function("() => window.__wl && window.__wl.owner")
    pg.errors = errors
    return pg


def viewer_page(chrome, server, host=None, probe=False, viewport=None):
    """A device on the network: its own context, so its own storage (its own key)."""
    ctx = chrome.new_context(viewport=viewport or {"width": 1280, "height": 800})
    OPEN.append(ctx)
    pg = ctx.new_page()
    errors = []
    pg.on("pageerror", lambda e: errors.append(str(e)))
    pg.goto(f"http://{host or '127.0.0.1'}:{server.port}/" + ("?probe" if probe else ""))
    pg.errors = errors
    return pg


def wait(pg, js, timeout=15):
    """Waits for a JS expression. Passed as a function: the page's CSP forbids eval,
    and a function goes through the protocol instead."""
    pg.wait_for_function(f"() => !!window.__wl && ({js})", timeout=timeout * 1000)


def ask(viewer, name="Test phone"):
    wait(viewer, "window.__wl.state === 'pair'")
    viewer.fill("#devName", name)
    viewer.click("#btnAsk")
    wait(viewer, "window.__wl.state === 'code'")
    return viewer.text_content("#codeTx")


def pair(owner, viewer, name="Test phone"):
    """Asks on the viewer, checks the owner sees the same code, allows it, and
    waits until the viewer is live."""
    code = ask(viewer, name)
    owner.wait_for_selector(f".ask:has-text('{code}')", timeout=5000)
    owner.click(f".ask:has-text('{code}') [data-act=allow]")
    wait(viewer, "window.__wl.state === 'live'", 20)
    return code


def frames(viewer, secs=2.0):
    """Frames presented in `secs`, and the picture's size."""
    return viewer.evaluate("""async (secs) => {
        const v = document.getElementById('video'); let n = 0;
        const cb = () => { n++; v.requestVideoFrameCallback(cb); };
        v.requestVideoFrameCallback(cb);
        await new Promise(r => setTimeout(r, secs * 1000));
        return {n, w: v.videoWidth, h: v.videoHeight};
    }""", secs)


def until(f, timeout=10, step=0.2):
    end = time.time() + timeout
    while time.time() < end:
        v = f()
        if v:
            return v
        time.sleep(step)
    return f()


import os as _os
# WILIVE_SOFTWARE_ENCODER=1: a machine without a hardware encoder (the Windows VM
# under emulation). Rate control rightly keeps it on a smaller size there; the
# tests then accept any of the three sizes, in the right shape.
SOFTWARE = _os.environ.get("WILIVE_SOFTWARE_ENCODER") == "1"


def sizes_ok(w, h, full_w, full_h):
    if (w, h) == (full_w, full_h):
        return True
    return SOFTWARE and (w, h) in {(full_w // 2 & ~1, full_h // 2 & ~1), (full_w // 4 & ~1, full_h // 4 & ~1)}
