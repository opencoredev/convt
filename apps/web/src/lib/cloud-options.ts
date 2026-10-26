// The conversion settings the cloud converter offers for a target, matching the
// desktop app's Quality and Size controls.

import { formats, type Format, type JobOptions } from "@convt/sdk";

export type Choice = { label: string; value: number | null };

export const QUALITY_CHOICES: readonly Choice[] = [
  { label: "Balanced", value: null },
  { label: "Smaller file", value: 60 },
  { label: "Best", value: 95 },
];

/** Which option the size control sets, its label and choices; `null` keeps the original. */
export type SizeControl = {
  field: "max_size" | "video_height";
  label: string;
  choices: readonly Choice[];
};

const IMAGE_SIZES: SizeControl = {
  field: "max_size",
  label: "Size",
  choices: [
    { label: "Original", value: null },
    { label: "4096 px", value: 4096 },
    { label: "2048 px", value: 2048 },
    { label: "1024 px", value: 1024 },
    { label: "512 px", value: 512 },
  ],
};

const VIDEO_SIZES: SizeControl = {
  field: "video_height",
  label: "Resolution",
  choices: [
    { label: "Original", value: null },
    { label: "2160p", value: 2160 },
    { label: "1080p", value: 1080 },
    { label: "720p", value: 720 },
    { label: "480p", value: 480 },
  ],
};

/**
 * FFmpeg makes a GIF from video at most 720 px wide unless a height is set
 * (`scale='min(720,iw)':-2` in crates/convt-engines/src/ffmpeg.rs), so its
 * no-option choice doesn't keep the original size.
 */
const VIDEO_GIF_SIZES: SizeControl = {
  ...VIDEO_SIZES,
  choices: [{ label: "Default (up to 720 px wide)", value: null }, ...VIDEO_SIZES.choices.slice(1)],
};

function categoryOf(target: Format) {
  return formats.find((f) => f.id === target)?.category;
}

/** WebP is left out: the image engine writes it lossless and ignores quality. */
export function qualityApplies(target: Format): boolean {
  return (
    categoryOf(target) === "video" || target === "jpeg" || target === "avif" || target === "heic"
  );
}

/** The formats a conversion goes between; `from` is unknown until a file is chosen. */
export type Pair = { from: Format | undefined; to: Format };

export function sizeControl({ from, to }: Pair): SizeControl | null {
  const category = categoryOf(to);
  if (category === "video") return VIDEO_SIZES;
  // FFmpeg makes a GIF from video and scales it by height.
  if (to === "gif" && from !== undefined && categoryOf(from) === "video") return VIDEO_GIF_SIZES;
  if (category === "image" && to !== "ico") return IMAGE_SIZES;
  return null;
}

/**
 * The options to send: only what applies to the conversion, or nothing when every
 * control is at its default. A size the conversion's control doesn't offer, left
 * from another file or target, is dropped.
 */
export function jobOptions(
  pair: Pair,
  picked: { quality: number | null; size: number | null },
): JobOptions | undefined {
  const options: JobOptions = {};
  if (picked.quality !== null && qualityApplies(pair.to)) options.quality = picked.quality;
  const size = sizeControl(pair);
  if (picked.size !== null && size?.choices.some((c) => c.value === picked.size)) {
    options[size.field] = picked.size;
  }
  return Object.keys(options).length > 0 ? options : undefined;
}
