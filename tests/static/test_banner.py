"""The README banner's text never overlaps, in the fonts GitHub's readers have.

Owner, 2026-11-26: "Hardware H.264, 60 fps" ran into "Paired devices only" - the
labels were placed by guess and checked only with one font. Each label is measured
in real Chrome with the banner's own font stack and with wider fallbacks."""
import pathlib

import pytest

ROOT = pathlib.Path(__file__).resolve().parents[2]
BANNERS = sorted((ROOT / "docs" / "assets").glob("banner-*.svg"))
# The stack as written, then what a reader without Inter or Apple's font may get.
FONTS = [None, "Helvetica, Arial, sans-serif", "Verdana, sans-serif", "'DejaVu Sans', sans-serif"]
GAP = 16  # px of clear space between neighbours, at the banner's own scale

MEASURE = """(font) => {
  const svg = document.querySelector('svg');
  const out = [];
  for (const t of svg.querySelectorAll('text')) {
    if (font) t.style.fontFamily = font;
    const b = t.getBBox();
    out.push({ s: t.textContent, x: b.x, y: b.y, w: b.width, h: b.height });
  }
  return { texts: out, width: svg.viewBox.baseVal.width };
}"""


@pytest.mark.parametrize("banner", BANNERS, ids=lambda p: p.name)
def test_banner_text_never_overlaps_or_leaves_the_banner(banner):
    sync_playwright = pytest.importorskip("playwright.sync_api").sync_playwright
    problems = []
    with sync_playwright() as p:
        b = p.chromium.launch()
        pg = b.new_page()
        for font in FONTS:
            pg.goto(banner.as_uri())
            r = pg.evaluate(MEASURE, font)
            ts = r["texts"]
            for i, a in enumerate(ts):
                if a["x"] + a["w"] > r["width"] - 24:
                    problems.append(f"{font or 'own'}: {a['s']!r} runs off the banner")
                for c in ts[i + 1:]:
                    same_row = a["y"] < c["y"] + c["h"] and c["y"] < a["y"] + a["h"]
                    if not same_row:
                        continue
                    left, right = (a, c) if a["x"] <= c["x"] else (c, a)
                    if left["x"] + left["w"] + GAP > right["x"]:
                        problems.append(f"{font or 'own'}: {left['s']!r} meets {right['s']!r}")
        b.close()
    assert not problems, "\n".join(problems)
