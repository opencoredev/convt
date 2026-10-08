// Decode an image and encode it as another format. Runs where there's a DOM (Chrome's
// offscreen document), because only the DOM's <img> can rasterize SVG.

import { sourceInfo, targetInfo, type SourceKind, type Target } from "../shared/formats.ts";
import { svgIntrinsicSize, svgRasterSize } from "../shared/sniff.ts";

/** Chrome refuses canvases past 16384px a side or about 268 megapixels. */
const MAX_SIDE = 16384;
const MAX_AREA = 268_435_456;
const THUMB_SIDE = 96;

export type Transcoded =
  | { ok: true; blob: Blob; thumb: string }
  | { ok: false; problem: "unreadable" | "too-large" | "encode" };

export async function transcode(input: {
  bytes: Uint8Array;
  from: SourceKind;
  to: Target;
  quality: number;
}): Promise<Transcoded> {
  const source = await decode(input.bytes, input.from);
  if (source === null) return { ok: false, problem: "unreadable" };
  const { width, height } = source;
  if (width > MAX_SIDE || height > MAX_SIDE || width * height > MAX_AREA) {
    close(source);
    return { ok: false, problem: "too-large" };
  }

  const { mime, lossy } = targetInfo[input.to];
  // JPG has no transparency; paint white underneath so transparent areas aren't black.
  const background = input.to === "jpg" ? "#ffffff" : null;
  try {
    const canvas = draw(source, width, height, background);
    const blob = await canvas.convertToBlob({
      type: mime,
      ...(lossy ? { quality: input.quality } : {}),
    });
    // Chrome quietly falls back to PNG for types it can't encode.
    if (blob.type !== mime) return { ok: false, problem: "encode" };
    const thumb = await thumbnail(source, width, height, background);
    return { ok: true, blob, thumb };
  } catch {
    return { ok: false, problem: "too-large" };
  } finally {
    close(source);
  }
}

type Source = { image: CanvasImageSource; width: number; height: number; bitmap: boolean };

function close(source: Source) {
  if (source.bitmap && source.image instanceof ImageBitmap) source.image.close();
}

async function decode(bytes: Uint8Array, from: SourceKind): Promise<Source | null> {
  if (!sourceInfo[from].browserDecodes) return null;
  const blob = new Blob([new Uint8Array(bytes)], { type: sourceInfo[from].mime });
  if (from === "svg") return decodeSvg(blob, new TextDecoder().decode(bytes));
  try {
    const image = await createImageBitmap(blob);
    return { image, width: image.width, height: image.height, bitmap: true };
  } catch {
    // createImageBitmap rejects a few formats <img> still reads (some ICOs, odd BMPs).
    return decodeWithElement(blob, null);
  }
}

async function decodeSvg(blob: Blob, text: string): Promise<Source | null> {
  return decodeWithElement(blob, svgRasterSize(svgIntrinsicSize(text)));
}

async function decodeWithElement(
  blob: Blob,
  size: { width: number; height: number } | null,
): Promise<Source | null> {
  const url = URL.createObjectURL(blob);
  const image = new Image();
  if (size) {
    image.width = size.width;
    image.height = size.height;
  }
  image.src = url;
  try {
    await image.decode();
    const width = size?.width ?? image.naturalWidth;
    const height = size?.height ?? image.naturalHeight;
    if (width === 0 || height === 0) return null;
    return { image, width, height, bitmap: false };
  } catch {
    return null;
  } finally {
    URL.revokeObjectURL(url);
  }
}

function draw(
  source: Source,
  width: number,
  height: number,
  background: string | null,
): OffscreenCanvas {
  const canvas = new OffscreenCanvas(width, height);
  const context = canvas.getContext("2d");
  if (!context) throw new Error("2d context unavailable");
  if (background) {
    context.fillStyle = background;
    context.fillRect(0, 0, width, height);
  }
  context.imageSmoothingQuality = "high";
  context.drawImage(source.image, 0, 0, width, height);
  return canvas;
}

/** A small WebP of the result for the popup's recent list. */
async function thumbnail(
  source: Source,
  width: number,
  height: number,
  background: string | null,
): Promise<string> {
  const scale = Math.min(1, THUMB_SIDE / Math.max(width, height));
  const w = Math.max(1, Math.round(width * scale));
  const h = Math.max(1, Math.round(height * scale));
  const blob = await draw(source, w, h, background).convertToBlob({
    type: "image/webp",
    quality: 0.8,
  });
  return blobToDataUrl(blob);
}

function blobToDataUrl(blob: Blob): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () =>
      typeof reader.result === "string" ? resolve(reader.result) : reject(new Error("no result"));
    reader.onerror = () => reject(reader.error);
    reader.readAsDataURL(blob);
  });
}
