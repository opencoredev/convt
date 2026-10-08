"""Draws the onboarding window's dithered green glow.

    python3 crates/convt-app/assets/onboarding/generate.py

Writes four PNGs next to this script: `glow-*` rises from the bottom edge of
every onboarding screen, `bloom-*` fills the window while convt sets itself
up. Each is a soft radial falloff in the brand green, quantized to a few
levels with an 8x8 Bayer matrix on 3px cells, so the grain stays visible.
The app draws them at their pixel size, unscaled, so the cells stay square.
Needs Pillow and NumPy; the output is the same on every run.
"""

from pathlib import Path

import numpy as np
from PIL import Image

HERE = Path(__file__).resolve().parent
CELL = 3
LEVELS = 5

BAYER = np.array(
    [
        [0, 32, 8, 40, 2, 34, 10, 42],
        [48, 16, 56, 24, 50, 18, 58, 26],
        [12, 44, 4, 36, 14, 46, 6, 38],
        [60, 28, 52, 20, 62, 30, 54, 22],
        [3, 35, 11, 43, 1, 33, 9, 41],
        [51, 19, 59, 27, 49, 17, 57, 25],
        [15, 47, 7, 39, 13, 45, 5, 37],
        [63, 31, 55, 23, 61, 29, 53, 21],
    ],
    dtype=np.float64,
) / 64.0

# (outer color, core color, strongest alpha) per appearance. The core is the
# web's --green family: deeper toward the light edge, glowing in the dark.
THEMES = {
    "light": ((0xA8, 0xE6, 0xC4), (0x1F, 0xB3, 0x6C), 0.92),
    "dark": ((0x0E, 0x4D, 0x2E), (0x3F, 0xCB, 0x84), 0.85),
}


def field(w, h, cx, cy, rx, ry, power):
    """Intensity 0..1 on the cell grid: 1 at (cx, cy), 0 past the ellipse."""
    ys, xs = np.mgrid[0:h, 0:w].astype(np.float64)
    d = np.sqrt(((xs + 0.5 - cx) / rx) ** 2 + ((ys + 0.5 - cy) / ry) ** 2)
    return np.clip(1.0 - d, 0.0, 1.0) ** power


def dither(intensity):
    """Quantizes to LEVELS steps with the Bayer threshold."""
    h, w = intensity.shape
    threshold = np.tile(BAYER, (h // 8 + 1, w // 8 + 1))[:h, :w]
    q = np.floor(intensity * (LEVELS - 1) + threshold)
    return np.clip(q, 0, LEVELS - 1) / (LEVELS - 1)


def render(name, size, center, radii, power, theme):
    outer, core, alpha = THEMES[theme]
    w, h = size[0] // CELL, size[1] // CELL
    q = dither(field(w, h, center[0] * w, center[1] * h, radii[0] * w, radii[1] * h, power))
    outer, core = np.array(outer, dtype=np.float64), np.array(core, dtype=np.float64)
    rgb = outer + (core - outer) * q[..., None]
    a = q * alpha * 255
    pixels = np.dstack([rgb, a]).round().astype(np.uint8)
    image = Image.fromarray(pixels, "RGBA").resize((w * CELL, h * CELL), Image.NEAREST)
    image.save(HERE / f"{name}-{theme}.png", optimize=True)


for theme in THEMES:
    # A wide band of light rising from below the bottom edge.
    render("glow", (2400, 720), (0.5, 1.12), (0.42, 1.05), 1.35, theme)
    # The whole window, brightest low and center.
    render("bloom", (2400, 1500), (0.5, 0.78), (0.55, 0.95), 1.1, theme)
