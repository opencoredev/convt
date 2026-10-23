// A tiny element builder. Text always goes in through textContent, never as HTML.

type Attrs = Record<string, string | boolean | undefined>;
type Child = Node | string | null | false;

export function h<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  attrs: Attrs = {},
  children: Child[] = [],
): HTMLElementTagNameMap[K] {
  const element = document.createElement(tag);
  for (const [name, value] of Object.entries(attrs)) {
    if (value === undefined || value === false) continue;
    element.setAttribute(name, value === true ? "" : value);
  }
  for (const child of children) {
    if (child === null || child === false) continue;
    element.append(child);
  }
  return element;
}

const SVG_NS = "http://www.w3.org/2000/svg";

/** Builds an icon from path data so no markup strings are parsed. */
export function icon(
  paths: readonly string[],
  options: { size?: number; stroke?: number; viewBox?: string } = {},
): SVGSVGElement {
  const size = options.size ?? 16;
  const svg = document.createElementNS(SVG_NS, "svg");
  svg.setAttribute("width", String(size));
  svg.setAttribute("height", String(size));
  svg.setAttribute("viewBox", options.viewBox ?? "0 0 24 24");
  svg.setAttribute("fill", "none");
  svg.setAttribute("stroke", "currentColor");
  svg.setAttribute("stroke-width", String(options.stroke ?? 2));
  svg.setAttribute("stroke-linecap", "round");
  svg.setAttribute("stroke-linejoin", "round");
  svg.setAttribute("aria-hidden", "true");
  for (const d of paths) {
    const path = document.createElementNS(SVG_NS, "path");
    path.setAttribute("d", d);
    svg.append(path);
  }
  return svg;
}

export const icons = {
  arrow: ["M5 12h14", "M13 6l6 6-6 6"],
  check: ["M5 12.5l4.5 4.5L19 7.5"],
  alert: ["M12 7v6", "M12 17h.01"],
  close: ["M6 6l12 12", "M18 6L6 18"],
  back: ["M15 18l-6-6 6-6"],
  settings: ["M4 7h10", "M18 7h2", "M4 17h4", "M12 17h8", "M16 5v4", "M10 15v4"],
  image: [
    "M5 4h14a1 1 0 0 1 1 1v14a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1V5a1 1 0 0 1 1-1z",
    "M4 16l5-5 4 4 3-3 4 4",
  ],
  folder: ["M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"],
  external: ["M14 5h5v5", "M19 5l-8 8", "M18 14v4a1 1 0 0 1-1 1H6a1 1 0 0 1-1-1V7a1 1 0 0 1 1-1h4"],
} as const;

/** The convt mark: the source file (ink) and the converted file (green) overlapping. */
export function mark(size: number): SVGSVGElement {
  const svg = document.createElementNS(SVG_NS, "svg");
  svg.setAttribute("width", String(size));
  svg.setAttribute("height", String(size));
  svg.setAttribute("viewBox", "0 0 32 32");
  svg.setAttribute("aria-hidden", "true");
  const id = `m${Math.random().toString(36).slice(2, 8)}`;
  const defs = document.createElementNS(SVG_NS, "defs");
  const clip = document.createElementNS(SVG_NS, "clipPath");
  clip.id = `${id}-clip`;
  clip.append(rect(2));
  const gradient = document.createElementNS(SVG_NS, "linearGradient");
  gradient.id = `${id}-green`;
  gradient.setAttribute("x1", "0");
  gradient.setAttribute("y1", "0");
  gradient.setAttribute("x2", "0");
  gradient.setAttribute("y2", "1");
  for (const [offset, color] of [
    ["0", "var(--mark-green-top)"],
    ["1", "var(--mark-green-bottom)"],
  ] as const) {
    const stop = document.createElementNS(SVG_NS, "stop");
    stop.setAttribute("offset", offset);
    stop.setAttribute("stop-color", color);
    gradient.append(stop);
  }
  defs.append(clip, gradient);
  const source = rect(2);
  source.setAttribute("fill", "var(--mark-source)");
  const converted = rect(11);
  converted.setAttribute("fill", `url(#${id}-green)`);
  const overlap = rect(11);
  overlap.setAttribute("fill", "var(--mark-overlap)");
  overlap.setAttribute("clip-path", `url(#${id}-clip)`);
  svg.append(defs, source, converted, overlap);
  return svg;
}

function rect(at: number): SVGRectElement {
  const r = document.createElementNS(SVG_NS, "rect");
  r.setAttribute("x", String(at));
  r.setAttribute("y", String(at));
  r.setAttribute("width", "19");
  r.setAttribute("height", "19");
  r.setAttribute("rx", "5");
  return r;
}
