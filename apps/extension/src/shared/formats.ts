// The formats the extension writes, and the ones it can recognize on the way in.
// Chrome's canvas encodes PNG, JPEG and WebP; everything else needs the desktop app.

export const TARGETS = ["png", "jpg", "webp"] as const;
export type Target = (typeof TARGETS)[number];

export const targetInfo = {
  png: { label: "PNG", mime: "image/png", ext: "png", lossy: false },
  jpg: { label: "JPG", mime: "image/jpeg", ext: "jpg", lossy: true },
  webp: { label: "WebP", mime: "image/webp", ext: "webp", lossy: true },
} as const satisfies Record<Target, { label: string; mime: string; ext: string; lossy: boolean }>;

export function isTarget(value: unknown): value is Target {
  return typeof value === "string" && (TARGETS as readonly string[]).includes(value);
}

/** What the source bytes turned out to be. `unknown` covers anything we don't sniff. */
export const SOURCE_KINDS = [
  "png",
  "jpeg",
  "gif",
  "webp",
  "avif",
  "heic",
  "bmp",
  "ico",
  "tiff",
  "svg",
  "unknown",
] as const;
export type SourceKind = (typeof SOURCE_KINDS)[number];

export function isSourceKind(value: unknown): value is SourceKind {
  return typeof value === "string" && (SOURCE_KINDS as readonly string[]).includes(value);
}

export const sourceInfo = {
  png: { label: "PNG", mime: "image/png", browserDecodes: true },
  jpeg: { label: "JPG", mime: "image/jpeg", browserDecodes: true },
  gif: { label: "GIF", mime: "image/gif", browserDecodes: true },
  webp: { label: "WebP", mime: "image/webp", browserDecodes: true },
  avif: { label: "AVIF", mime: "image/avif", browserDecodes: true },
  heic: { label: "HEIC", mime: "image/heic", browserDecodes: false },
  bmp: { label: "BMP", mime: "image/bmp", browserDecodes: true },
  ico: { label: "ICO", mime: "image/x-icon", browserDecodes: true },
  tiff: { label: "TIFF", mime: "image/tiff", browserDecodes: false },
  svg: { label: "SVG", mime: "image/svg+xml", browserDecodes: true },
  unknown: { label: "Image", mime: "application/octet-stream", browserDecodes: true },
} as const satisfies Record<SourceKind, { label: string; mime: string; browserDecodes: boolean }>;
