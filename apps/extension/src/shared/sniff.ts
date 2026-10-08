// Recognize an image from its bytes. Servers often send the wrong Content-Type (or
// none), so the magic bytes decide what we tell the user and how we decode.

import type { SourceKind } from "./formats.ts";

export type Sniffed = { kind: SourceKind; animated: boolean };

export function sniff(bytes: Uint8Array): Sniffed {
  const kind = sniffKind(bytes);
  return { kind, animated: isAnimated(kind, bytes) };
}

function startsWith(bytes: Uint8Array, signature: readonly number[], offset = 0): boolean {
  if (bytes.length < offset + signature.length) return false;
  return signature.every((byte, i) => bytes[offset + i] === byte);
}

function ascii(bytes: Uint8Array, start: number, length: number): string {
  let text = "";
  for (let i = start; i < start + length && i < bytes.length; i++) {
    text += String.fromCharCode(bytes[i] ?? 0);
  }
  return text;
}

function u32be(bytes: Uint8Array, offset: number): number {
  return (
    ((bytes[offset] ?? 0) * 0x1000000 +
      ((bytes[offset + 1] ?? 0) << 16) +
      ((bytes[offset + 2] ?? 0) << 8) +
      (bytes[offset + 3] ?? 0)) >>>
    0
  );
}

function sniffKind(bytes: Uint8Array): SourceKind {
  if (startsWith(bytes, [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a])) return "png";
  if (startsWith(bytes, [0xff, 0xd8, 0xff])) return "jpeg";
  if (ascii(bytes, 0, 6) === "GIF87a" || ascii(bytes, 0, 6) === "GIF89a") return "gif";
  if (ascii(bytes, 0, 4) === "RIFF" && ascii(bytes, 8, 4) === "WEBP") return "webp";
  if (ascii(bytes, 4, 4) === "ftyp") return isobmffKind(bytes);
  if (ascii(bytes, 0, 2) === "BM") return "bmp";
  if (startsWith(bytes, [0x00, 0x00, 0x01, 0x00])) return "ico";
  if (startsWith(bytes, [0x49, 0x49, 0x2a, 0x00]) || startsWith(bytes, [0x4d, 0x4d, 0x00, 0x2a])) {
    return "tiff";
  }
  if (looksLikeSvg(bytes)) return "svg";
  return "unknown";
}

/** HEIF-family files share the `ftyp` box; the brands tell AVIF from HEIC. */
function isobmffBrands(bytes: Uint8Array): string[] {
  const boxSize = Math.min(u32be(bytes, 0), bytes.length, 256);
  const brands = [ascii(bytes, 8, 4)];
  for (let offset = 16; offset + 4 <= boxSize; offset += 4) brands.push(ascii(bytes, offset, 4));
  return brands;
}

function isobmffKind(bytes: Uint8Array): SourceKind {
  const brands = isobmffBrands(bytes);
  if (brands.some((b) => b === "avif" || b === "avis")) return "avif";
  const heic = ["heic", "heix", "hevc", "hevx", "heim", "heis", "hevm", "hevs", "mif1", "msf1"];
  if (brands.some((b) => heic.includes(b))) return "heic";
  return "unknown";
}

function looksLikeSvg(bytes: Uint8Array): boolean {
  const head = ascii(bytes, 0, 2048).replace(/^﻿|^ï»¿/, "");
  if (!/^\s*</.test(head)) return false;
  return /<svg[\s>]/i.test(head);
}

function isAnimated(kind: SourceKind, bytes: Uint8Array): boolean {
  switch (kind) {
    case "gif":
      return gifFrameCount(bytes, 2) > 1;
    case "webp":
      // Extended WebP: the VP8X chunk's flag byte has bit 1 set for animation.
      return ascii(bytes, 12, 4) === "VP8X" && ((bytes[20] ?? 0) & 0x02) !== 0;
    case "png":
      return pngHasAnimationControl(bytes);
    case "avif":
      return isobmffBrands(bytes).includes("avis");
    case "jpeg":
    case "heic":
    case "bmp":
    case "ico":
    case "tiff":
    case "svg":
    case "unknown":
      return false;
    default: {
      const _exhaustive: never = kind;
      return _exhaustive;
    }
  }
}

/** Counts image descriptors, stopping once `limit` is reached. Truncated files count what they have. */
export function gifFrameCount(bytes: Uint8Array, limit = Number.POSITIVE_INFINITY): number {
  let offset = 13;
  const screenFlags = bytes[10] ?? 0;
  if (screenFlags & 0x80) offset += 3 * 2 ** ((screenFlags & 0x07) + 1);
  let frames = 0;
  const skipSubBlocks = () => {
    while (offset < bytes.length) {
      const size = bytes[offset] ?? 0;
      offset += 1;
      if (size === 0) return;
      offset += size;
    }
  };
  while (offset < bytes.length && frames < limit) {
    const block = bytes[offset];
    if (block === 0x3b) break;
    if (block === 0x21) {
      offset += 2;
      skipSubBlocks();
    } else if (block === 0x2c) {
      frames += 1;
      const flags = bytes[offset + 9] ?? 0;
      offset += 10;
      if (flags & 0x80) offset += 3 * 2 ** ((flags & 0x07) + 1);
      offset += 1;
      skipSubBlocks();
    } else {
      break;
    }
  }
  return frames;
}

/** APNG puts an `acTL` chunk before the first `IDAT`. */
function pngHasAnimationControl(bytes: Uint8Array): boolean {
  let offset = 8;
  while (offset + 8 <= bytes.length) {
    const length = u32be(bytes, offset);
    const type = ascii(bytes, offset + 4, 4);
    if (type === "acTL") return true;
    if (type === "IDAT" || type === "IEND") return false;
    offset += 12 + length;
  }
  return false;
}

export type Size = { width: number; height: number };

/**
 * The size an SVG asks to be drawn at: its width and height attributes in absolute
 * units, else its viewBox. Null when it gives neither (the caller picks a default).
 */
export function svgIntrinsicSize(text: string): Size | null {
  const tag = /<svg\b[^>]*>/i.exec(text)?.[0];
  if (!tag) return null;
  const attr = (name: string) =>
    new RegExp(`\\s${name}\\s*=\\s*(["'])(.*?)\\1`, "i").exec(tag)?.[2]?.trim() ?? null;
  const length = (value: string | null): number | null => {
    if (value === null) return null;
    const match = /^([0-9]*\.?[0-9]+)\s*(px)?$/i.exec(value);
    if (!match?.[1]) return null;
    const n = Number(match[1]);
    return n > 0 ? n : null;
  };
  const viewBox = attr("viewBox")
    ?.split(/[\s,]+/)
    .map(Number)
    .filter((n) => Number.isFinite(n));
  const box =
    viewBox && viewBox.length === 4 && (viewBox[2] ?? 0) > 0 && (viewBox[3] ?? 0) > 0
      ? { width: viewBox[2] ?? 0, height: viewBox[3] ?? 0 }
      : null;
  const width = length(attr("width"));
  const height = length(attr("height"));
  if (width !== null && height !== null) return { width, height };
  if (box && width !== null) return { width, height: (width * box.height) / box.width };
  if (box && height !== null) return { width: (height * box.width) / box.height, height };
  return box;
}

/**
 * How big to rasterize a vector image. Small icons come out at 2x, so a 24px icon
 * gives a usable 48px PNG; nothing grows past 4096px on its long side.
 */
export function svgRasterSize(intrinsic: Size | null): Size {
  const base = intrinsic ?? { width: 512, height: 512 };
  const longest = Math.max(base.width, base.height);
  const scale = Math.min(longest < 1024 ? 2 : 1, 4096 / longest);
  return {
    width: Math.max(1, Math.round(base.width * scale)),
    height: Math.max(1, Math.round(base.height * scale)),
  };
}
