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

function categoryOf(target: Format) {
  return formats.find((f) => f.id === target)?.category;
}

/** WebP is left out: the image engine writes it lossless and ignores quality. */
export function qualityApplies(target: Format): boolean {
  return (
    categoryOf(target) === "video" || target === "jpeg" || target === "avif" || target === "heic"
  );
}

export function sizeControl(target: Format): SizeControl | null {
  const category = categoryOf(target);
  if (category === "video") return VIDEO_SIZES;
  if (category === "image" && target !== "ico") return IMAGE_SIZES;
  return null;
}

/** The options to send: only what applies to `target`, or nothing when every control is at its default. */
export function jobOptions(
  target: Format,
  picked: { quality: number | null; size: number | null },
): JobOptions | undefined {
  const options: JobOptions = {};
  if (picked.quality !== null && qualityApplies(target)) options.quality = picked.quality;
  const size = sizeControl(target);
  if (picked.size !== null && size) options[size.field] = picked.size;
  return Object.keys(options).length > 0 ? options : undefined;
}
