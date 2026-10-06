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

GREEN_TOP = "#2fbf78"
GREEN_BOTTOM = "#1f9a5c"
INK = "#0a0b0b"
WHITE = "#ffffff"

# The mark: the favicon's 32-unit rounded square with the two convert arrows.
ARROWS = "M8 12.5h14M18.5 9l3.5 3.5-3.5 3.5M24 19.5H10M13.5 16 10 19.5l3.5 3.5"
ARROW_STROKE = 'fill="none" stroke-width="2.6" stroke-linecap="round" stroke-linejoin="round"'


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


def wordmark_path(text: str = "convt", weight: int = 600, tracking: float = -0.03):
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
    return f"{n:.2f}".rstrip("0").rstrip(".")


def mark_group(variant: str, x: float = 0, y: float = 0, scale: float = 1) -> tuple[str, str]:
    """(defs, body) for the mark at (x, y). variant: color, black or white."""
    place = f'transform="translate({fmt(x)} {fmt(y)}) scale({fmt(scale)})"'
    if variant == "color":
        defs = (
            f'<linearGradient id="convt-g" x1="0" y1="0" x2="0" y2="1">'
            f'<stop offset="0" stop-color="{GREEN_TOP}"/><stop offset="1" stop-color="{GREEN_BOTTOM}"/>'
            f"</linearGradient>"
        )
        body = (
            f'<g {place}><rect width="32" height="32" rx="8" fill="url(#convt-g)"/>'
            f'<path d="{ARROWS}" stroke="{WHITE}" {ARROW_STROKE}/></g>'
        )
        return defs, body
    # One color: the square in that color with the arrows cut out, so it works on any background.
    fill = INK if variant == "black" else WHITE
    defs = (
        f'<mask id="convt-m-{variant}" maskUnits="userSpaceOnUse" x="0" y="0" width="32" height="32">'
        f'<rect width="32" height="32" rx="8" fill="#fff"/>'
        f'<path d="{ARROWS}" stroke="#000" {ARROW_STROKE}/></mask>'
    )
    body = f'<g {place}><rect width="32" height="32" rx="8" fill="{fill}" mask="url(#convt-m-{variant})"/></g>'
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

    for variant in ["color", "black", "white"]:
        defs, body = mark_group(variant, scale=16)
        stem = "convt-mark" if variant == "color" else f"convt-mark-{variant}"
        files[stem] = (svg(512, 512, defs, body, "convt"), 1024)

    d, (xmin, ymin, xmax, ymax) = wordmark_path()
    # Wordmark: 1000-unit glyphs scaled to a 96 px tall box (ascender of "t" to baseline).
    pad = 0
    s = 96 / (ymax - ymin)
    ww, wh = (xmax - xmin) * s + 2 * pad, (ymax - ymin) * s + 2 * pad
    word = f'transform="translate({fmt(pad - xmin * s)} {fmt(pad - ymin * s)}) scale({fmt(s)})"'
    for variant, fill in [("black", INK), ("white", WHITE)]:
        body = f'<path {word} fill="{fill}" d="{d}"/>'
        files[f"convt-wordmark-{variant}"] = (svg(ww, wh, "", body, "convt"), 1600)

    # Lockup: the mark beside the wordmark. The wordmark's x-height is centered on the mark
    # and the gap is a quarter of the mark.
    mark = 128
    x_height = 534  # Geist OS/2 sxHeight
    ls = mark * 0.5 / x_height  # wordmark x-height = half the mark
    gap = mark * 0.25
    baseline = mark / 2 + x_height * ls / 2
    lw = mark + gap + (xmax - xmin) * ls
    word = f'transform="translate({fmt(mark + gap - xmin * ls)} {fmt(baseline)}) scale({fmt(ls)})"'
    # The "t" rises above the mark only if the type is set large; keep the canvas the mark's height.
    top = min(0, baseline + ymin * ls)
    lh = mark - top
    for variant, fill in [("black", INK), ("white", WHITE)]:
        defs, body = mark_group("color", 0, -top, mark / 32)
        text = f'<g transform="translate(0 {fmt(-top)})"><path {word} fill="{fill}" d="{d}"/></g>'
        files[f"convt-lockup-{variant}"] = (svg(lw, lh, defs, body + text, "convt"), 2000)
    return files


def convt_bin() -> str:
    found = os.environ.get("CONVT_BIN") or shutil.which("convt")
    if not found:
        raise SystemExit("convt not found; build it (cargo build -p convt-cli) and set CONVT_BIN")
    return found


def main():
    out.mkdir(parents=True, exist_ok=True)
    for old in out.glob("convt-*"):
        old.unlink()

    files = build_svgs()
    convt = convt_bin()
    with tempfile.TemporaryDirectory() as tmp:
        for stem, (text, edge) in files.items():
            (out / f"{stem}.svg").write_text(text)
            src = Path(tmp) / f"{stem}.svg"
            src.write_text(text)
            # resvg renders at 96 dpi by default; scale up so the longest edge is `edge` px.
            subprocess.run(
                [convt, str(src), "--to", "png", "--out-dir", str(out), "--dpi", "960", "--max-size", str(edge)],
                check=True,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
            )

    with zipfile.ZipFile(out / "convt-brand.zip", "w", zipfile.ZIP_DEFLATED) as z:
        for stem in files:
            for ext in ["svg", "png"]:
                z.write(out / f"{stem}.{ext}", f"convt-brand/{ext}/{stem}.{ext}")

    # Sizes for the page's download labels.
    def png_size(path: Path) -> list[int]:
        head = path.read_bytes()[16:24]
        return [int.from_bytes(head[:4], "big"), int.from_bytes(head[4:], "big")]

    manifest = {
        "zipBytes": (out / "convt-brand.zip").stat().st_size,
        "assets": {stem: {"png": png_size(out / f"{stem}.png")} for stem in files},
    }
    text = json.dumps(manifest, indent=2)
    # Keep [width, height] on one line, as oxfmt formats it.
    manifest_path.write_text(re.sub(r"\[\s+(\d+),\s+(\d+)\s+\]", r"[\1, \2]", text) + "\n")
    print(f"Wrote {len(files)} assets and convt-brand.zip to {out}")


if __name__ == "__main__":
    main()
