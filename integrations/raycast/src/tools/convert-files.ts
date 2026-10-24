import type { Tool } from "@raycast/api";
import { convertFiles } from "../lib/convt";
import { getBinary, getJobs } from "../lib/preferences";

type Input = {
  /** Exact absolute paths to existing local files or folders supplied by the user. Folder files that can't convert to the target are skipped. */
  files: string[];
  /** Output format ID returned by get-targets, such as jpeg, webp, mp4, or pdf. */
  target: string;
  /** Existing output folder. Omit to save beside each original. */
  outputDirectory?: string;
  /** Also convert files in subfolders of folder inputs. Omit unless the user asks to include subfolders. */
  recursive?: boolean;
  /** Name of a saved Convt preset returned by get-targets. Options passed here override the preset's. */
  preset?: string;
  /** Lossy quality, a whole number from 1 to 100. Omit for Convt's default. */
  quality?: number;
  /** Maximum image edge in pixels, a positive whole number. */
  maxSize?: number;
  /** PDF page or range, such as 2, 1-3, or 4-. Omit for all pages. */
  pages?: string;
  /** PDF or SVG rendering resolution, a whole number from 18 to 1200. */
  dpi?: number;
  /** For image targets: what transparent areas become. transparent, white, black, or a color such as #ff8800. JPEG can't be transparent. */
  background?: string;
  /** For video targets: maximum output height in pixels, an even number such as 2160, 1440, 1080, 720, 480, or 360. Never enlarges. */
  videoHeight?: number;
  /** For MP4, MOV, or MKV targets: h264 (default, plays everywhere) or hevc (smaller files). */
  videoCodec?: "h264" | "hevc";
  /** For audio or video targets: audio bitrate in kbit/s, from 8 to 640, such as 320, 192, or 128. */
  audioBitrate?: number;
  /** For video targets: leave the audio out. */
  noAudio?: boolean;
};

export const confirmation: Tool.Confirmation<Input> = async (input) => ({
  message:
    "Convert these files locally with Convt? New files will be saved; originals and existing outputs are preserved.",
  info: [
    { name: "Files", value: input.files.join("\n") },
    { name: "Output Format", value: input.target },
    { name: "Output Folder", value: input.outputDirectory || "Beside each original" },
    ...(input.recursive ? [{ name: "Subfolders", value: "Included" }] : []),
    ...(input.preset !== undefined ? [{ name: "Preset", value: input.preset }] : []),
    ...(input.quality !== undefined ? [{ name: "Quality", value: String(input.quality) }] : []),
    ...(input.maxSize !== undefined ? [{ name: "Maximum Image Size", value: `${input.maxSize} px` }] : []),
    ...(input.pages !== undefined ? [{ name: "PDF Pages", value: input.pages }] : []),
    ...(input.dpi !== undefined ? [{ name: "Render DPI", value: String(input.dpi) }] : []),
    ...(input.background !== undefined ? [{ name: "Background", value: input.background }] : []),
    ...(input.videoHeight !== undefined ? [{ name: "Maximum Video Height", value: `${input.videoHeight}p` }] : []),
    ...(input.videoCodec !== undefined ? [{ name: "Video Codec", value: input.videoCodec }] : []),
    ...(input.audioBitrate !== undefined ? [{ name: "Audio Bitrate", value: `${input.audioBitrate} kbps` }] : []),
    ...(input.noAudio ? [{ name: "Audio", value: "Removed" }] : []),
  ],
});

/** Convert local files or folders and return each converted file's actual output paths, failure, or cancellation. Never retry successful entries. */
export default async function tool(input: Input) {
  return convertFiles(await getBinary(), { ...input, jobs: getJobs() });
}
