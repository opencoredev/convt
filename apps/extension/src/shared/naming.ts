// Turn an image URL into the file name the user sees in Downloads.

import { targetInfo, type Target } from "./formats.ts";

const IMAGE_EXT = /\.(png|jpe?g|jfif|pjpeg|gif|webp|avif|heic|heif|bmp|ico|tiff?|svg)$/i;
const MAX_STEM = 100;

/** `https://cdn.example.com/photos/Miso%20asleep.webp?w=800` → `Miso asleep.png` */
export function outputName(sourceUrl: string, target: Target): string {
  return `${stemFromUrl(sourceUrl)}.${targetInfo[target].ext}`;
}

export function stemFromUrl(sourceUrl: string): string {
  let url: URL;
  try {
    url = new URL(sourceUrl);
  } catch {
    return "image";
  }
  // data: and blob: URLs carry no file name.
  const named = ["http:", "https:", "file:", "chrome-extension:", "moz-extension:"];
  if (!named.includes(url.protocol)) return "image";

  // Image proxies (Next.js /_next/image, imgix-style resizers) carry the real
  // path in a query parameter. Prefer it when it names an image file.
  for (const key of ["url", "src", "image", "file"]) {
    const nested = url.searchParams.get(key);
    if (nested) {
      const path = nested.split(/[?#]/)[0] ?? "";
      const base = lastSegment(path);
      if (IMAGE_EXT.test(base)) return clean(base) ?? "image";
    }
  }
  return clean(lastSegment(url.pathname)) ?? "image";
}

function lastSegment(path: string): string {
  const segments = path.split("/").filter((s) => s.length > 0);
  const last = segments.at(-1) ?? "";
  try {
    return decodeURIComponent(last);
  } catch {
    return last;
  }
}

/** Strips the extension and characters Windows, macOS or Linux reject. Null when nothing is left. */
function clean(name: string): string | null {
  const stem = name
    .replace(IMAGE_EXT, "")
    // Drop a short trailing extension the regex above didn't know (`photo.php`).
    .replace(/\.[a-z0-9]{1,4}$/i, "")
    // eslint-disable-next-line no-control-regex
    .replace(/[\u0000-\u001f\u007f<>:"/\\|?*]+/g, " ")
    .replace(/\s+/g, " ")
    .replace(/^[\s.]+|[\s.]+$/g, "");
  if (stem.length === 0) return null;
  // Windows reserves these device names whatever the extension.
  if (/^(con|prn|aux|nul|com[0-9]|lpt[0-9])$/i.test(stem)) return `${stem}-image`;
  return [...stem].slice(0, MAX_STEM).join("").trimEnd();
}

/** `photos/Miso.png` or `C:\Users\me\Downloads\Miso.png` → `Miso.png` */
export function baseName(path: string): string {
  return path.split(/[\\/]/).at(-1) ?? path;
}

export function formatBytes(bytes: number): string {
  if (bytes < 1000) return `${bytes} B`;
  const units = ["KB", "MB", "GB"] as const;
  let value = bytes / 1000;
  let unit = 0;
  while (value >= 1000 && unit < units.length - 1) {
    value /= 1000;
    unit += 1;
  }
  const digits = value < 10 ? 1 : 0;
  return `${value.toFixed(digits)} ${units[unit]}`;
}
