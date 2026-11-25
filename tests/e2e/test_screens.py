"""Every screen at every size: phones both ways up, tablets, laptops, big monitors,
light and dark. Nothing scrolls sideways, every control is on screen and no two
controls overlap."""
import pytest

from support.pages import OPEN, owner_page, pair, wait

pytestmark = pytest.mark.e2e
# Phones both ways up (iPhone SE, 8, 8 Plus, 13), tablets, laptops, big monitors.
SIZES = [(320, 568), (568, 320), (375, 667), (667, 375), (414, 736), (736, 414), (390, 844), (844, 390), (768, 1024), (1024, 768), (1280, 800), (1920, 1080), (2560, 1440)]

CHECK = """(sel) => {
  const W = innerWidth, H = innerHeight, out = [];
  if (document.documentElement.scrollWidth > W + 1) out.push('scrolls sideways: ' + document.documentElement.scrollWidth + ' > ' + W);
  const boxes = [...document.querySelectorAll(sel)].filter(e => e.offsetParent !== null && getComputedStyle(e).visibility !== 'hidden')
    .map(e => [e.id || e.dataset.act || e.textContent.trim().slice(0, 20), e.getBoundingClientRect()]);
  for (const [n, r] of boxes) {
    if (r.left < -1 || r.right > W + 1) out.push(n + ' off screen sideways');
    if (r.width < 24 || r.height < 24) out.push(n + ' too small to tap: ' + Math.round(r.width) + 'x' + Math.round(r.height));
  }
  for (let i = 0; i < boxes.length; i++) for (let j = i + 1; j < boxes.length; j++) {
    const a = boxes[i][1], b = boxes[j][1];
    if (a.left < b.right - 1 && b.left < a.right - 1 && a.top < b.bottom - 1 && b.top < a.bottom - 1) out.push(boxes[i][0] + ' overlaps ' + boxes[j][0]);
  }
  return out;
}"""


def ctx(chrome, size, scheme, owner_token=None):
    c = chrome.new_context(viewport={"width": size[0], "height": size[1]}, color_scheme=scheme)
    OPEN.append(c)
    if owner_token:
        c.add_init_script(f"window.WILIVE_OWNER = {owner_token!r};")
    return c


@pytest.mark.parametrize("scheme", ["light", "dark"])
def test_the_pairing_screens_fit_every_size(chrome, server, scheme):
    problems = []
    for size in SIZES:
        p = ctx(chrome, size, scheme).new_page()
        p.goto(server.base + "/")
        wait(p, "window.__wl.state === 'pair'")
        problems += [f"{size} ask: {x}" for x in p.evaluate(CHECK, "#vPair button:not([hidden]), #vPair input")]
        p.close()
    assert not problems, "\n".join(problems)


@pytest.mark.parametrize("scheme", ["light", "dark"])
def test_the_owner_window_fits_every_size(chrome, make_server, scheme):
    s = make_server(WILIVE_TEST_HOST=1)
    problems = []
    for size in SIZES:
        p = ctx(chrome, size, scheme, s.owner).new_page()
        p.goto(s.base + "/")
        wait(p, "!!window.__wl.owner")
        problems += [f"{size}: {x}" for x in p.evaluate(CHECK, "#vOwner button, #vOwner a")]
        p.close()
    assert not problems, "\n".join(problems)


def test_the_watching_bar_fits_every_size_and_the_picture_fills_the_screen(chrome, make_server):
    s = make_server("1280", "720", "60", WILIVE_DISPLAYS=2)
    owner = owner_page(chrome, s)
    v = ctx(chrome, (1280, 800), "light").new_page()
    v.goto(s.base + "/")
    pair(owner, v)
    problems = []
    for size in SIZES:
        v.set_viewport_size({"width": size[0], "height": size[1]})
        v.wait_for_timeout(150)
        problems += [f"{size}: {x}" for x in v.evaluate(CHECK, "#bar button:not([hidden]), #bar select:not([hidden]), #liveChip")]
        r = v.evaluate("(() => { const b = document.getElementById('video').getBoundingClientRect(); return [b.width, b.height, innerWidth, innerHeight]; })()")
        if abs(r[0] - r[2]) > 1 or abs(r[1] - r[3]) > 1:
            problems.append(f"{size}: the picture's box {r[0]}x{r[1]} is not the screen")
    assert not problems, "\n".join(problems)


def test_all_displays_fit_every_size_in_every_layout(chrome, make_server):
    s = make_server("1280", "720", "60", WILIVE_DISPLAYS=4)
    owner = owner_page(chrome, s)
    v = ctx(chrome, (1280, 800), "dark").new_page()
    v.goto(s.base + "/")
    pair(owner, v)
    v.click("#btnWall")
    v.wait_for_function("() => document.querySelectorAll('#wall .tile').length === 4")
    problems = []
    for layout in ["grid", "row", "column", "focus"]:
        while v.get_attribute("#wall", "data-layout") != layout:
            v.click("#btnLayout")
        for size in SIZES:
            v.set_viewport_size({"width": size[0], "height": size[1]})
            v.wait_for_timeout(120)
            problems += [f"{layout} {size}: {x}" for x in v.evaluate(CHECK, "#bar button:not([hidden]), #bar select:not([hidden]), #liveChip")]
            tiles = v.evaluate("""() => [...document.querySelectorAll('#wall .screen')].map(t => { const r = t.getBoundingClientRect(); return [r.left, r.top, r.right, r.bottom]; })""")
            shapes = v.evaluate("""() => [...document.querySelectorAll('#wall .tile')].map(t => { const r = t.firstChild.getBoundingClientRect(); return Math.abs(r.width / r.height - Number(t.dataset.ar)); })""")
            problems += [f"{layout} {size}: screen {i} is not its display's shape" for i, d in enumerate(shapes) if d > 0.03 * max(1, d)]
            W, H = size
            for i, (l, t, r, b) in enumerate(tiles):
                if l < -1 or t < -1 or r > W + 1 or b > H + 1:
                    problems.append(f"{layout} {size}: tile {i} off screen")
                thumb = layout == "focus" and i > 0   # a preview beside the big one: tappable is enough
                if min(r - l, b - t) < 40 or (not thumb and (r - l) * (b - t) < 4000):
                    problems.append(f"{layout} {size}: tile {i} too small ({r - l:.0f}x{b - t:.0f})")
            for i in range(len(tiles)):
                for j in range(i + 1, len(tiles)):
                    a, c = tiles[i], tiles[j]
                    if a[0] < c[2] - 1 and c[0] < a[2] - 1 and a[1] < c[3] - 1 and c[1] < a[3] - 1:
                        problems.append(f"{layout} {size}: tiles {i} and {j} overlap")
    assert not problems, "\n".join(problems[:40])
