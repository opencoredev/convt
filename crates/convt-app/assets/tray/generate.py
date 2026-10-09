"""Draws the tray icons from the convt mark.

    python3 crates/convt-app/assets/tray/generate.py

The mark is two overlapping rounded squares on a 32-unit grid: the file you
have (top left) and the file you get (bottom right). Each platform gets the
form its tray expects:

- macOS, `menubar.png` and `menubar-busy.png`: 36x36 template images (18 pt
  at 2x). Only the alpha counts; macOS tints them for a light or dark menu
  bar and for the highlighted item. The file you have is an outline, the
  file you get is solid, with a gap where they cross. While converting, the
  file you get is an outline filling from the bottom.
- Linux, `convt-symbolic.svg` and `convt-busy-symbolic.svg`: the same glyphs
  as symbolic icons in `currentColor`, which the tray host recolors for its
  panel. `tray32.png` and `tray64.png` (and `-busy`) are the pixmaps a host
  falls back to.
- Windows, `tray32.png` and `tray32-busy.png`: the app icon's dark tile with
  the colored mark, which reads on a light and a dark taskbar. Busy adds a
  green dot in the corner.

Needs Pillow; the output is the same on every run.
"""

from pathlib import Path

from PIL import Image, ImageDraw

HERE = Path(__file__).resolve().parent
SS = 16  # supersampling factor

# The mark on its 32-unit grid.
BACK = (2, 2, 21, 21)
FRONT = (11, 11, 30, 30)
RADIUS = 5


def template(busy: bool, px: int = 36) -> Image.Image:
    """The menu bar glyph: black with alpha, on a px-square canvas."""
    # 18 pt canvas; the glyph is 15 pt tall, centered, as menu bar extras are.
    glyph = px * 15 / 18
    k = glyph / 28 * SS
    off = (px * SS - 28 * k) / 2 - 2 * k
    stroke = 1.5 / 18 * px * SS  # 1.5 pt
    gap = 1.25 / 18 * px * SS

    def box(r, grow=0.0):
        x0, y0, x1, y1 = r
        return (off + x0 * k - grow, off + y0 * k - grow, off + x1 * k + grow, off + y1 * k + grow)

    size = px * SS
    alpha = Image.new("L", (size, size), 0)
    d = ImageDraw.Draw(alpha)
    rad = RADIUS * k
    # The file you have: an outline.
    d.rounded_rectangle(box(BACK), rad, outline=255, width=round(stroke))
    # Cut a gap around the file you get, then draw it.
    d.rounded_rectangle(box(FRONT, gap), rad + gap, fill=0)
    if busy:
        d.rounded_rectangle(box(FRONT), rad, outline=255, width=round(stroke))
        # Filling up from the bottom: the lower half, solid.
        fill = Image.new("L", (size, size), 0)
        ImageDraw.Draw(fill).rounded_rectangle(box(FRONT), rad, fill=255)
        x0, y0, x1, y1 = box(FRONT)
        half = Image.new("L", (size, size), 0)
        ImageDraw.Draw(half).rectangle((0, (y0 + y1) / 2, size, size), fill=255)
        alpha.paste(255, mask=Image.composite(fill, half, half))
    else:
        d.rounded_rectangle(box(FRONT), rad, fill=255)
    alpha = alpha.resize((px, px), Image.LANCZOS)
    out = Image.new("RGBA", (px, px), (0, 0, 0, 0))
    out.putalpha(alpha)
    return out


def rounded(x: float, y: float, w: float, h: float, r: float, clockwise: bool = True) -> str:
    """A rounded rectangle as path data, drawn either way round."""
    f = lambda v: f"{v:.3f}".rstrip("0").rstrip(".")  # noqa: E731
    r = min(r, w / 2, h / 2)
    if clockwise:
        return (
            f"M{f(x + r)} {f(y)}H{f(x + w - r)}A{f(r)} {f(r)} 0 0 1 {f(x + w)} {f(y + r)}"
            f"V{f(y + h - r)}A{f(r)} {f(r)} 0 0 1 {f(x + w - r)} {f(y + h)}"
            f"H{f(x + r)}A{f(r)} {f(r)} 0 0 1 {f(x)} {f(y + h - r)}"
            f"V{f(y + r)}A{f(r)} {f(r)} 0 0 1 {f(x + r)} {f(y)}Z"
        )
    return (
        f"M{f(x + r)} {f(y)}A{f(r)} {f(r)} 0 0 0 {f(x)} {f(y + r)}"
        f"V{f(y + h - r)}A{f(r)} {f(r)} 0 0 0 {f(x + r)} {f(y + h)}"
        f"H{f(x + w - r)}A{f(r)} {f(r)} 0 0 0 {f(x + w)} {f(y + h - r)}"
        f"V{f(y + r)}A{f(r)} {f(r)} 0 0 0 {f(x + w - r)} {f(y)}Z"
    )


def symbolic_svg(busy: bool) -> str:
    """The template glyph as a 16x16 symbolic SVG in currentColor.

    Symbolic icons may only use fills: GTK recolors one by forcing `fill` on
    every shape, which would fill a stroked outline. So the outlines are
    rings (an outer and an inner rounded rectangle, wound opposite ways) and
    the gap is a clip path.
    """
    glyph = 16 * 15 / 18
    k = glyph / 28
    off = (16 - 28 * k) / 2 - 2 * k
    stroke = 1.5 / 18 * 16
    gap = 1.25 / 18 * 16
    rad = RADIUS * k

    def rect(r, inset=0.0):
        x0, y0, x1, y1 = r
        return (
            off + x0 * k + inset,
            off + y0 * k + inset,
            (x1 - x0) * k - 2 * inset,
            (y1 - y0) * k - 2 * inset,
        )

    def ring(r):
        return rounded(*rect(r), rad) + rounded(*rect(r, stroke), rad - stroke, clockwise=False)

    f = lambda v: f"{v:.3f}".rstrip("0").rstrip(".")  # noqa: E731
    gx, gy, gw, gh = rect(FRONT, -gap)
    fx, fy, fw, fh = rect(FRONT)
    # Everything but the gap around the file you get.
    outside_gap = "M0 0H16V16H0Z" + rounded(gx, gy, gw, gh, rad + gap, clockwise=False)
    parts = [
        '<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 16 16">',
        "<style>.ColorScheme-Text{color:#232629}</style>",
        "<defs>",
        f'<clipPath id="gap"><path d="{outside_gap}"/></clipPath>',
    ]
    if busy:
        parts.append(
            f'<clipPath id="half"><path d="M0 {f(fy + fh / 2)}H16V16H0Z"/></clipPath>'
        )
    parts.append("</defs>")
    parts.append('<g class="ColorScheme-Text" fill="currentColor">')
    parts.append(f'<path d="{ring(BACK)}" clip-path="url(#gap)"/>')
    if busy:
        parts.append(f'<path d="{ring(FRONT)}"/>')
        parts.append(f'<path d="{rounded(fx, fy, fw, fh, rad)}" clip-path="url(#half)"/>')
    else:
        parts.append(f'<path d="{rounded(fx, fy, fw, fh, rad)}"/>')
    parts.append("</g></svg>\n")
    return "\n".join(parts)


def tile(px: int, busy: bool) -> Image.Image:
    """The app icon's dark tile with the colored mark, for Windows and as the
    Linux fallback pixmap."""
    size = px * SS
    img = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    # A tile that fills the icon, as tray icons do, with a hairline rim so
    # it holds its edge on a near-black taskbar.
    d.rounded_rectangle((0, 0, size - 1, size - 1), size * 0.22, fill=(23, 25, 24, 255))
    d.rounded_rectangle(
        (0, 0, size - 1, size - 1), size * 0.22, outline=(70, 74, 72, 255), width=max(SS, size // 32)
    )
    k = size * 0.72 / 28
    off = (size - 28 * k) / 2 - 2 * k

    def box(r):
        x0, y0, x1, y1 = r
        return (off + x0 * k, off + y0 * k, off + x1 * k, off + y1 * k)

    rad = RADIUS * k
    d.rounded_rectangle(box(BACK), rad, fill=(237, 239, 238, 255))
    # The file you get, a vertical green gradient.
    grad = Image.new("RGBA", (size, size))
    gd = ImageDraw.Draw(grad)
    x0, y0, x1, y1 = box(FRONT)
    top, bottom = (0x46, 0xD0, 0x8B), (0x1F, 0xA4, 0x63)
    for y in range(size):
        t = min(max((y - y0) / (y1 - y0), 0), 1)
        gd.line([(0, y), (size, y)], fill=tuple(round(a + (b - a) * t) for a, b in zip(top, bottom)) + (255,))
    mask = Image.new("L", (size, size), 0)
    ImageDraw.Draw(mask).rounded_rectangle(box(FRONT), rad, fill=255)
    img.paste(grad, mask=mask)
    # Where they cross: the light overlap shade.
    over = Image.new("L", (size, size), 0)
    ImageDraw.Draw(over).rounded_rectangle(box(BACK), rad, fill=255)
    both = Image.composite(over, Image.new("L", (size, size), 0), mask)
    img.paste((0xA6, 0xF0, 0xC8, 255), mask=both)
    if busy:
        # A green dot with a dark ring in the bottom right corner.
        r = size * 0.13
        cx = cy = size - r - size * 0.06
        d.ellipse((cx - r - size * 0.05, cy - r - size * 0.05, cx + r + size * 0.05, cy + r + size * 0.05), fill=(23, 25, 24, 255))
        d.ellipse((cx - r, cy - r, cx + r, cy + r), fill=(0x46, 0xD0, 0x8B, 255))
    return img.resize((px, px), Image.LANCZOS)


def main() -> None:
    template(False).save(HERE / "menubar.png", optimize=True)
    template(True).save(HERE / "menubar-busy.png", optimize=True)
    (HERE / "convt-symbolic.svg").write_text(symbolic_svg(False))
    (HERE / "convt-busy-symbolic.svg").write_text(symbolic_svg(True))
    for px in (32, 64):
        tile(px, False).save(HERE / f"tray{px}.png", optimize=True)
        tile(px, True).save(HERE / f"tray{px}-busy.png", optimize=True)


if __name__ == "__main__":
    main()
