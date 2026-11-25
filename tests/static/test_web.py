"""The dashboard's source: what must hold before it ever reaches a browser."""
import pathlib
import re
import subprocess

import pytest

pytestmark = pytest.mark.static
WEB = pathlib.Path(__file__).resolve().parents[2] / "web"
HTML = (WEB / "index.html").read_text()
JS = (WEB / "app.js").read_text()
CSS = (WEB / "app.css").read_text()


@pytest.mark.parametrize("f", ["app.js", "crypto.js"])
def test_scripts_parse(f):
    """A syntax error is a blank page with nothing on screen to say why."""
    r = subprocess.run(["node", "--check", str(WEB / f)], capture_output=True, text=True)
    assert r.returncode == 0, r.stderr


def test_nothing_inline_that_the_csp_would_block():
    """The CSP allows only the page's own files: no style or on* attributes, no
    inline script."""
    assert not re.search(r"\sstyle\s*=", HTML), "style= attribute"
    assert not re.search(r"\son[a-z]+\s*=", HTML), "on*= handler attribute"
    for m in re.finditer(r"<script([^>]*)>(.*?)</script>", HTML, re.S):
        assert "src=" in m.group(1) and not m.group(2).strip(), "inline script"
    assert "innerHTML" not in JS and "insertAdjacentHTML" not in JS and "eval(" not in JS and "new Function" not in JS


def test_nothing_is_loaded_from_the_internet():
    for name in ("index.html", "app.js", "app.css", "crypto.js"):
        src = (WEB / name).read_text()
        urls = [u for u in re.findall(r"https?://[^\s'\")]+", src) if not u.startswith("http://www.w3.org/")]
        assert not urls, f"{name}: {urls}"


def test_every_id_the_script_uses_exists():
    ids = set(re.findall(r'\sid="([^"]+)"', HTML))
    used = set(re.findall(r"\$\('([^']+)'\)", JS))
    assert not used - ids, f"missing in index.html: {sorted(used - ids)}"


def test_every_icon_used_is_defined():
    defined = set(re.findall(r'<symbol id="([^"]+)"', HTML))
    used = set(re.findall(r"#(i-[a-z0-9-]+)", HTML + JS)) | {"i-" + n for n in re.findall(r"icon\('([a-z0-9-]+)'\)", JS)}
    assert not used - defined, sorted(used - defined)


def _lum(h):
    c = [int(h[i:i + 2], 16) / 255 for i in (1, 3, 5)]
    c = [x / 12.92 if x <= 0.03928 else ((x + 0.055) / 1.055) ** 2.4 for x in c]
    return 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]


def _ratio(a, b):
    la, lb = _lum(a), _lum(b)
    return (max(la, lb) + 0.05) / (min(la, lb) + 0.05)


def _themes():
    light = dict(re.findall(r"--([a-z0-9-]+):\s*(#[0-9A-Fa-f]{6})", CSS.split("@media")[0]))
    dark_block = re.search(r':root\[data-theme="dark"\]\s*\{(.*?)\}', CSS, re.S).group(1)
    dark = dict(light, **dict(re.findall(r"--([a-z0-9-]+):\s*(#[0-9A-Fa-f]{6})", dark_block)))
    return {"light": light, "dark": dark}


@pytest.mark.parametrize("theme", ["light", "dark"])
def test_both_themes_meet_wcag_aa(theme):
    t = _themes()[theme]
    text = [("text", b) for b in ("bg", "surface", "surface-2")] + [("text-2", b) for b in ("bg", "surface", "surface-2")] \
        + [("accent-text", b) for b in ("bg", "surface")] + [("on-accent", "accent"), ("on-accent", "accent-hover")] \
        + [("ok", b) for b in ("bg", "surface")] + [("warn", b) for b in ("bg", "surface")]
    bad = [f"{f} on {b}: {_ratio(t[f], t[b]):.2f}" for f, b in text if _ratio(t[f], t[b]) < 4.5]
    bad += [f"live on {b}: {_ratio(t['live'], t[b]):.2f}" for b in ("bg", "surface") if _ratio(t["live"], t[b]) < 3]
    assert not bad, bad


def test_the_media_query_dark_theme_matches_the_forced_one():
    """Dark by system setting and dark by choice must be the same colours."""
    auto = re.search(r'@media \(prefers-color-scheme: dark\)\s*\{\s*:root:not\(\[data-theme="light"\]\)\s*\{(.*?)\}', CSS, re.S).group(1)
    forced = re.search(r':root\[data-theme="dark"\]\s*\{(.*?)\}', CSS, re.S).group(1)
    norm = lambda s: sorted(x.strip() for x in s.split(";") if x.strip())
    assert norm(auto) == norm(forced)
