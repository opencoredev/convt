import { describe, expect, test } from "bun:test";

import { gifFrameCount, sniff, svgIntrinsicSize, svgRasterSize } from "../../src/shared/sniff.ts";

const ascii = (text: string) => [...text].map((c) => c.charCodeAt(0));
const u32 = (n: number) => [(n >>> 24) & 255, (n >>> 16) & 255, (n >>> 8) & 255, n & 255];
const bytes = (...parts: number[][]) => new Uint8Array(parts.flat());

/** A 1x1 GIF with `frames` image descriptors, each with a graphic control extension. */
function gif(frames: number): Uint8Array {
  const frame = [
    [0x21, 0xf9, 0x04, 0x00, 0x0a, 0x00, 0x00, 0x00],
    [0x2c, 0, 0, 0, 0, 1, 0, 1, 0, 0x00],
    [0x02, 0x02, 0x44, 0x01, 0x00],
  ].flat();
  return bytes(
    ascii("GIF89a"),
    [1, 0, 1, 0, 0x80, 0, 0],
    [0, 0, 0, 255, 255, 255],
    ...Array.from({ length: frames }, () => frame),
    [0x3b],
  );
}

function png(chunks: string[]): Uint8Array {
  return bytes(
    [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a],
    ...chunks.map((type) => [...u32(4), ...ascii(type), 0, 0, 0, 0, 0, 0, 0, 0]),
  );
}

function webp(flags: number): Uint8Array {
  return bytes(ascii("RIFF"), [0, 0, 0, 0], ascii("WEBP"), ascii("VP8X"), [10, 0, 0, 0, flags]);
}

function ftyp(major: string, compatible: string[]): Uint8Array {
  const size = 16 + compatible.length * 4;
  return bytes(u32(size), ascii("ftyp"), ascii(major), [0, 0, 0, 0], ...compatible.map(ascii));
}

describe("sniff", () => {
  test("recognizes still formats", () => {
    expect(sniff(png(["IHDR", "IDAT", "IEND"]))).toEqual({ kind: "png", animated: false });
    expect(sniff(bytes([0xff, 0xd8, 0xff, 0xe0]))).toEqual({ kind: "jpeg", animated: false });
    expect(sniff(gif(1))).toEqual({ kind: "gif", animated: false });
    expect(sniff(webp(0x10))).toEqual({ kind: "webp", animated: false });
    expect(sniff(bytes(ascii("BM"), [0, 0]))).toEqual({ kind: "bmp", animated: false });
    expect(sniff(bytes([0, 0, 1, 0, 1, 0]))).toEqual({ kind: "ico", animated: false });
    expect(sniff(bytes([0x49, 0x49, 0x2a, 0]))).toEqual({ kind: "tiff", animated: false });
  });

  test("tells AVIF from HEIC by brand", () => {
    expect(sniff(ftyp("avif", ["mif1", "miaf"])).kind).toBe("avif");
    expect(sniff(ftyp("mif1", ["avif"])).kind).toBe("avif");
    expect(sniff(ftyp("heic", ["mif1", "heic"])).kind).toBe("heic");
    expect(sniff(ftyp("mif1", ["heic"])).kind).toBe("heic");
    expect(sniff(ftyp("isom", ["mp41"])).kind).toBe("unknown");
  });

  test("detects animation", () => {
    expect(sniff(gif(2)).animated).toBe(true);
    expect(sniff(webp(0x02 | 0x10)).animated).toBe(true);
    expect(sniff(png(["IHDR", "acTL", "IDAT"])).animated).toBe(true);
    expect(sniff(png(["IHDR", "IDAT", "acTL"])).animated).toBe(false);
    expect(sniff(ftyp("avis", ["avif", "msf1"]))).toEqual({ kind: "avif", animated: true });
  });

  test("counts GIF frames, and survives truncation", () => {
    expect(gifFrameCount(gif(3))).toBe(3);
    expect(gifFrameCount(gif(3), 2)).toBe(2);
    expect(gifFrameCount(gif(2).subarray(0, 30))).toBe(1);
  });

  test("recognizes SVG text, with or without a prolog", () => {
    const enc = (s: string) => new TextEncoder().encode(s);
    expect(sniff(enc('<svg xmlns="http://www.w3.org/2000/svg"/>')).kind).toBe("svg");
    expect(sniff(enc('﻿<?xml version="1.0"?>\n<!-- x -->\n<svg>')).kind).toBe("svg");
    expect(sniff(enc("<!doctype html><html><body>Sign in</body></html>")).kind).toBe("unknown");
    expect(sniff(enc("")).kind).toBe("unknown");
  });
});

describe("svg sizing", () => {
  test("reads absolute width and height", () => {
    expect(svgIntrinsicSize('<svg width="24" height="16px">')).toEqual({ width: 24, height: 16 });
  });

  test("falls back to the viewBox, keeping its aspect ratio", () => {
    expect(svgIntrinsicSize('<svg viewBox="0 0 300 150">')).toEqual({ width: 300, height: 150 });
    expect(svgIntrinsicSize('<svg width="100%" viewBox="0,0,40,20">')).toEqual({
      width: 40,
      height: 20,
    });
    expect(svgIntrinsicSize('<svg width="80" viewBox="0 0 40 20">')).toEqual({
      width: 80,
      height: 40,
    });
  });

  test("gives up on relative units with no viewBox", () => {
    expect(svgIntrinsicSize('<svg width="10em" height="2em">')).toBeNull();
    expect(svgIntrinsicSize("no svg here")).toBeNull();
  });

  test("rasterizes small art at 2x and caps the long side", () => {
    expect(svgRasterSize({ width: 24, height: 24 })).toEqual({ width: 48, height: 48 });
    expect(svgRasterSize({ width: 2000, height: 1000 })).toEqual({ width: 2000, height: 1000 });
    expect(svgRasterSize({ width: 10000, height: 5000 })).toEqual({ width: 4096, height: 2048 });
    expect(svgRasterSize(null)).toEqual({ width: 1024, height: 1024 });
  });
});
