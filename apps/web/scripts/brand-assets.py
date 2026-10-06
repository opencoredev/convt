"""Generate the brand kit in public/brand/: SVGs, PNGs and convt-brand.zip.

The wordmark is "convt" set in Geist SemiBold and converted to outlines, so the SVGs
need no font. PNGs are rendered with convt itself (its resvg engine); point CONVT_BIN at
a convt binary, otherwise `convt` on PATH is used.

    cd apps/web
    uv run --with 'fonttools[woff]' --with uharfbuzz python scripts/brand-assets.py

The page at /brand lists these files by name; it reads the sizes from
src/components/brand/manifest.json, which this script also writes.
"""

import io
import json
import math
import os
import re
import shutil
import subprocess
import tempfile
import zipfile
from pathlib import Path

import uharfbuzz as hb
from fontTools.pens.boundsPen import BoundsPen
from fontTools.pens.svgPathPen import SVGPathPen
from fontTools.pens.transformPen import TransformPen
from fontTools.ttLib import TTFont
from fontTools.varLib.instancer import instantiateVariableFont

web = Path(__file__).resolve().parent.parent
repo = web.parent.parent
out = web / "public" / "brand"
manifest_path = web / "src" / "components" / "brand" / "manifest.json"

# The mark's colors, from design-assets/brand/README.md and the --mark-* tokens in styles.css.
ON_DARK = {"ink": "#edefee", "top": "#46d08b", "bottom": "#1fa463", "overlap": "#a6f0c8"}
ON_LIGHT = {"ink": "#0a0a0a", "top": "#1fb36c", "bottom": "#127a47", "overlap": "#0b5c34"}

# The mark: two 19-unit rounded squares on a 32-unit grid, the source file at (2, 2) and the
# converted file at (11, 11). Same geometry as src/components/logo.tsx. The files crop the
# 2-unit margin, so the mark fills a 28-unit box.
SOURCE = '<rect x="2" y="2" width="19" height="19" rx="5"'
RESULT = '<rect x="11" y="11" width="19" height="19" rx="5"'
REPO_ICON = "packaging/linux/convt.svg"

def find_font() -> Path:
    pattern = "@fontsource-variable/geist/files/geist-latin-wght-normal.woff2"
    for root in [web / "node_modules", repo / "node_modules"]:
        direct = root / pattern
        if direct.exists():
            return direct
        hits = sorted((root / ".bun").glob(f"@fontsource-variable+geist@*/node_modules/{pattern}"))
        if hits:
            return hits[-1]
    raise SystemExit("Geist not found; run `bun install` first")


def wordmark_path(text: str = "convt", weight: int = 600, tracking: float = -0.02):
    """Outline `text` in Geist at `weight`. Returns (svg path d, (xmin, ymin, xmax, ymax)) in font units, y down."""
    font_path = find_font()
    font = instantiateVariableFont(TTFont(font_path), {"wght": weight})
    glyphs = font.getGlyphSet()
    upm = font["head"].unitsPerEm

    # HarfBuzz cannot read WOFF2, so shape with the instanced font saved as plain TrueType.
    data = io.BytesIO()
    font.flavor = None
    font.save(data)
    hb_font = hb.Font(hb.Face(hb.Blob(data.getvalue())))
    buf = hb.Buffer()
    buf.add_str(text)
    buf.guess_segment_properties()
    hb.shape(hb_font, buf, {"kern": True, "liga": False})

    order = font.getGlyphOrder()
    svg_pen = SVGPathPen(glyphs)
    bounds = BoundsPen(glyphs)
    x = 0.0
    for info, pos in zip(buf.glyph_infos, buf.glyph_positions):
        name = order[info.codepoint]
        # Flip y so the result is in SVG space; baseline at y = 0.
        transform = (1, 0, 0, -1, x + pos.x_offset, -pos.y_offset)
        glyphs[name].draw(TransformPen(svg_pen, transform))
        glyphs[name].draw(TransformPen(bounds, transform))
        x += pos.x_advance + tracking * upm
    return svg_pen.getCommands(), bounds.bounds


def fmt(n: float) -> str:
    return f"{n:.4f}".rstrip("0").rstrip(".")


def mark_group(variant: str, x: float = 0, y: float = 0, size: float = 28) -> tuple[str, str]:
    """(defs, body) for the mark filling a `size` box at (x, y).

    variant: "dark" or "light" (full color, for that background), or "white" or "black"
    (one color, with the overlap cut out, as the Finder menu icon does).
    """
    scale = size / 28
    place = f'transform="translate({fmt(x - 2 * scale)} {fmt(y - 2 * scale)}) scale({fmt(scale)})"'
    if variant in ("dark", "light"):
        c = ON_DARK if variant == "dark" else ON_LIGHT
        defs = (
            f'<clipPath id="convt-source-{variant}">{SOURCE}/></clipPath>'
            f'<linearGradient id="convt-green-{variant}" x1="0" y1="0" x2="0" y2="1">'
            f'<stop offset="0" stop-color="{c["top"]}"/><stop offset="1" stop-color="{c["bottom"]}"/>'
            f"</linearGradient>"
        )
        body = (
            f'<g {place}>{SOURCE} fill="{c["ink"]}"/>{RESULT} fill="url(#convt-green-{variant})"/>'
            f'{RESULT} fill="{c["overlap"]}" clip-path="url(#convt-source-{variant})"/></g>'
        )
        return defs, body
    fill = ON_DARK["ink"] if variant == "white" else ON_LIGHT["ink"]
    defs = (
        f'<clipPath id="convt-source-{variant}">{SOURCE}/></clipPath>'
        f'<mask id="convt-cut-{variant}" maskUnits="userSpaceOnUse" x="0" y="0" width="32" height="32">'
        f'<rect width="32" height="32" fill="#fff"/>{RESULT} fill="#000" clip-path="url(#convt-source-{variant})"/></mask>'
    )
    body = f'<g {place}><g mask="url(#convt-cut-{variant})" fill="{fill}">{SOURCE}/>{RESULT}/></g></g>'
    return defs, body


def svg(width: float, height: float, defs: str, body: str, title: str) -> str:
    # Whole-pixel canvases, so PNG exports come out at round sizes.
    width, height = math.ceil(width), math.ceil(height)
    defs_tag = f"<defs>{defs}</defs>" if defs else ""
    return (
        f'<svg xmlns="http://www.w3.org/2000/svg" width="{fmt(width)}" height="{fmt(height)}" '
        f'viewBox="0 0 {fmt(width)} {fmt(height)}" role="img" aria-label="{title}">'
        f"<title>{title}</title>{defs_tag}{body}</svg>\n"
    )


def build_svgs() -> dict[str, tuple[str, int]]:
    """File stem -> (svg text, PNG longest edge)."""
    files: dict[str, tuple[str, int]] = {}

    for variant in ["dark", "light"]:
        defs, body = mark_group(variant, size=512)
        files[f"convt-mark-on-{variant}"] = (svg(512, 512, defs, body, "convt"), 1024)
    for variant in ["white", "black"]:
        defs, body = mark_group(variant, size=512)
        files[f"convt-mark-{variant}"] = (svg(512, 512, defs, body, "convt"), 1024)

    # The app icon is the packaging source, unchanged.
    files["convt-app-icon"] = ((repo / REPO_ICON).read_text(), 1024)

    d, (xmin, ymin, xmax, ymax) = wordmark_path()
    # Wordmark: scaled so the "t" (its tallest glyph) to the baseline is 96 px.
    s = 96 / (ymax - ymin)
    ww, wh = (xmax - xmin) * s, (ymax - ymin) * s
    word = f'transform="translate({fmt(-xmin * s)} {fmt(-ymin * s)}) scale({fmt(s)})"'
    for variant, c in [("dark", ON_DARK), ("light", ON_LIGHT)]:
        body = f'<path {word} fill="{c["ink"]}" d="{d}"/>'
        files[f"convt-wordmark-on-{variant}"] = (svg(ww, wh, "", body, "convt"), 1600)

    # Lockup, measured from design-assets/brand/lockup-on-dark.png (2400 x 595, mark 595 px):
    # the text starts 943 px in, its baseline is at 539 px and its x-height is 302 px.
    mark = 160
    unit = mark / 595
    x_height = 534  # Geist OS/2 sxHeight, in font units
    ls = 302 * unit / x_height
    baseline = 539 * unit
    left = 943 * unit
    top = min(0, baseline + ymin * ls)
    lw = left + (xmax - xmin) * ls
    lh = mark - top
    word = f'transform="translate({fmt(left - xmin * ls)} {fmt(baseline - top)}) scale({fmt(ls)})"'
    for variant, c in [("dark", ON_DARK), ("light", ON_LIGHT)]:
        defs, body = mark_group(variant, 0, -top, mark)
        text = f'<path {word} fill="{c["ink"]}" d="{d}"/>'
        files[f"convt-lockup-on-{variant}"] = (svg(lw, lh, defs, body + text, "convt"), 2400)
    return files


def convt_bin() -> str:
    found = os.environ.get("CONVT_BIN") or shutil.which("convt")
    if not found:
        raise SystemExit("convt not found; build it (cargo build -p convt-cli) and set CONVT_BIN")
    return found


def main():
    files = build_svgs()
    convt = convt_bin()
    # Build the whole kit in a staging folder and swap it in only when every step worked,
    # so a missing tool or a failed render leaves the committed kit untouched.
    with tempfile.TemporaryDirectory() as tmp:
        stage = Path(tmp) / "brand"
        stage.mkdir()
        for stem, (text, edge) in files.items():
            src = stage / f"{stem}.svg"
            src.write_text(text)
            # resvg renders at 96 dpi by default; scale up so the longest edge is `edge` px.
            result = subprocess.run(
                [convt, str(src), "--to", "png", "--out-dir", str(stage), "--dpi", "960", "--max-size", str(edge)],
                capture_output=True,
                text=True,
            )
            if result.returncode != 0:
                raise SystemExit(f"convt could not render {stem}.png:\n{result.stderr or result.stdout}")

        with zipfile.ZipFile(stage / "convt-brand.zip", "w", zipfile.ZIP_DEFLATED) as z:
            for stem in files:
                for ext in ["svg", "png"]:
                    z.write(stage / f"{stem}.{ext}", f"convt-brand/{ext}/{stem}.{ext}")

        # Sizes for the page's download labels.
        def png_size(path: Path) -> list[int]:
            head = path.read_bytes()[16:24]
            return [int.from_bytes(head[:4], "big"), int.from_bytes(head[4:], "big")]

        manifest = {
            "zipBytes": (stage / "convt-brand.zip").stat().st_size,
            "assets": {stem: {"png": png_size(stage / f"{stem}.png")} for stem in files},
        }
        text = json.dumps(manifest, indent=2)

        out.mkdir(parents=True, exist_ok=True)
        for old in out.glob("convt-*"):
            old.unlink()
        for new in stage.iterdir():
            shutil.copy2(new, out / new.name)
    # Keep [width, height] on one line, as oxfmt formats it.
    manifest_path.write_text(re.sub(r"\[\s+(\d+),\s+(\d+)\s+\]", r"[\1, \2]", text) + "\n")
    print(f"Wrote {len(files)} assets and convt-brand.zip to {out}")


if __name__ == "__main__":
    main()
