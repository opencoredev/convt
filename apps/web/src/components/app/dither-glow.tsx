import { useEffect, useRef } from "react";

import { cx } from "./ui";

/*
 * The green glow that rises behind the sign-in and desktop sign-in screens. It is drawn
 * at a third of the CSS resolution and scaled up with `image-rendering: pixelated`, then
 * ordered-dithered (8x8 Bayer) into a few alpha steps, so the falloff shows as pixel
 * grain instead of a smooth gradient. Two soft lobes drift on slow, out-of-phase cycles.
 * Reduced motion draws one still frame. The color comes from --glow in styles.css, so it
 * follows the theme.
 */

/** Size of one dither pixel in CSS pixels. */
const CELL = 3;
/** Alpha steps between transparent and the glow's peak. */
const LEVELS = 6;
/** Redraws per second; the drift is slow enough that more would only cost battery. */
const FPS = 24;

const bayer = buildBayer(8);

function buildBayer(size: number): Float32Array {
  // The recursive Bayer construction, normalized to [0, 1).
  let m = [[0]];
  while (m.length < size) {
    const n = m.length;
    const next: number[][] = Array.from({ length: n * 2 }, () => Array<number>(n * 2).fill(0));
    for (let y = 0; y < n; y++)
      for (let x = 0; x < n; x++) {
        const v = m[y][x] * 4;
        next[y][x] = v;
        next[y][x + n] = v + 2;
        next[y + n][x] = v + 3;
        next[y + n][x + n] = v + 1;
      }
    m = next;
  }
  const out = new Float32Array(size * size);
  for (let y = 0; y < size; y++)
    for (let x = 0; x < size; x++) out[y * size + x] = m[y][x] / (size * size);
  return out;
}

type Glow = { r: number; g: number; b: number; peak: number };

function readGlow(el: Element): Glow {
  const style = getComputedStyle(el);
  const [r = 31, g = 179, b = 108] = style
    .getPropertyValue("--glow")
    .trim()
    .split(/\s+/)
    .map(Number);
  const peak = Number.parseFloat(style.getPropertyValue("--glow-peak")) || 0.6;
  return { r, g, b, peak };
}

function smoothstep(edge0: number, edge1: number, x: number) {
  const t = Math.min(1, Math.max(0, (x - edge0) / (edge1 - edge0)));
  return t * t * (3 - 2 * t);
}

/** Glow strength in [0, 1] at a point; u is -0.5..0.5 across, v is 0 at the bottom. */
function intensity(u: number, v: number, t: number, aspect: number) {
  // Wider screens get a flatter arc, so the glow never becomes a narrow spotlight.
  const spread = Math.min(1.25, Math.max(0.7, aspect * 0.42));
  const main = lobe(
    u - 0.04 * Math.sin(t * 0.21),
    v,
    0.62 * spread + 0.03 * Math.sin(t * 0.33),
    0.92,
  );
  const left = lobe(
    u + 0.24 + 0.05 * Math.sin(t * 0.17),
    v,
    0.34 * spread,
    0.62 + 0.06 * Math.sin(t * 0.27),
  );
  const right = lobe(
    u - 0.26 - 0.05 * Math.cos(t * 0.19),
    v,
    0.32 * spread,
    0.55 + 0.07 * Math.cos(t * 0.23),
  );
  return Math.min(1, main + 0.42 * left + 0.38 * right);
}

function lobe(u: number, v: number, rx: number, ry: number) {
  const d = Math.sqrt((u / rx) ** 2 + (v / ry) ** 2);
  const s = 1 - smoothstep(0, 1, d);
  return s * s;
}

export function DitherGlow({ className }: { className?: string }) {
  const ref = useRef<HTMLCanvasElement>(null);

  useEffect(() => {
    const canvas = ref.current;
    const ctx = canvas?.getContext("2d");
    if (!canvas || !ctx) return;
    const reduce = window.matchMedia("(prefers-reduced-motion: reduce)");
    let glow = readGlow(canvas);
    let image: ImageData | null = null;
    let frame = 0;
    let last = 0;
    const start = performance.now() - 40_000 * Math.random();

    function resize() {
      if (!canvas || !ctx) return;
      const w = Math.max(1, Math.ceil(canvas.clientWidth / CELL));
      const h = Math.max(1, Math.ceil(canvas.clientHeight / CELL));
      if (canvas.width !== w || canvas.height !== h) {
        canvas.width = w;
        canvas.height = h;
        image = ctx.createImageData(w, h);
      }
    }

    function draw(now: number) {
      if (!canvas || !ctx || !image) return;
      const t = (now - start) / 1000;
      const { width: w, height: h } = canvas;
      const data = image.data;
      const aspect = canvas.clientWidth / Math.max(1, canvas.clientHeight);
      for (let y = 0; y < h; y++) {
        const v = 1 - (y + 0.5) / h;
        for (let x = 0; x < w; x++) {
          const u = (x + 0.5) / w - 0.5;
          const level = Math.floor(
            intensity(u, v, t, aspect) * LEVELS + bayer[(y & 7) * 8 + (x & 7)],
          );
          const i = (y * w + x) * 4;
          data[i] = glow.r;
          data[i + 1] = glow.g;
          data[i + 2] = glow.b;
          data[i + 3] = Math.round((Math.min(LEVELS, level) / LEVELS) * glow.peak * 255);
        }
      }
      ctx.putImageData(image, 0, 0);
      canvas.dataset.ready = "";
    }

    function loop(now: number) {
      frame = requestAnimationFrame(loop);
      if (now - last < 1000 / FPS) return;
      last = now;
      draw(now);
    }

    function restart() {
      cancelAnimationFrame(frame);
      resize();
      if (reduce.matches || document.hidden) draw(start + 20_000);
      else frame = requestAnimationFrame(loop);
    }

    // The theme toggles a class on <html>; pick up the new color without a reload.
    const themes = new MutationObserver(() => {
      glow = readGlow(canvas);
      if (reduce.matches) draw(start + 20_000);
    });
    themes.observe(document.documentElement, { attributes: true, attributeFilter: ["class"] });
    const sizes = new ResizeObserver(() => {
      resize();
      if (reduce.matches || document.hidden) draw(start + 20_000);
    });
    sizes.observe(canvas);
    reduce.addEventListener("change", restart);
    document.addEventListener("visibilitychange", restart);
    restart();

    return () => {
      cancelAnimationFrame(frame);
      themes.disconnect();
      sizes.disconnect();
      reduce.removeEventListener("change", restart);
      document.removeEventListener("visibilitychange", restart);
    };
  }, []);

  return (
    <canvas
      ref={ref}
      aria-hidden="true"
      className={cx(
        "pointer-events-none [image-rendering:pixelated] opacity-0 transition-opacity duration-700 ease-out data-ready:opacity-100 motion-reduce:transition-none",
        className,
      )}
    />
  );
}
