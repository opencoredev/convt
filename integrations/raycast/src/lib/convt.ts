import { execFile, spawn } from "node:child_process";
import { constants } from "node:fs";
import { access, readdir, stat } from "node:fs/promises";
import { homedir } from "node:os";
import path from "node:path";
import { createInterface } from "node:readline";
import { promisify } from "node:util";

const execFileAsync = promisify(execFile);

export class ConvtNotFoundError extends Error {
  constructor(message = "Install Convt, or set its executable path in the extension preferences.") {
    super(message);
    this.name = "ConvtNotFoundError";
  }
}

export type Format = { id: string; name: string; category: string; extensions: string[] };
export type Source = { file: string; extension: string; format: Format | null };
/** A folder input, summarized: the files Convt would consider, grouped by recognized format. */
export type FolderSource = {
  folder: string;
  files: number;
  formats: { format: Format; files: number }[];
  /** False when the folder held more files than the extension scans; targets then reflect the scanned part. */
  complete: boolean;
};
export type FileInspection = {
  /** Files named directly. Each must reach every listed target. */
  sources: Source[];
  /** Folder inputs. Their files that can't reach the chosen target are skipped, as the CLI does. */
  folders: FolderSource[];
  targets: Format[];
  suggestedTargetIds: string[];
  /** An Office document in the selection has no targets: neither LibreOffice nor the document pack is installed. */
  documentPackMissing: boolean;
};
export type Preset = { name: string; target: string | null };

export const VIDEO_HEIGHTS = [2160, 1440, 1080, 720, 480, 360] as const;
export const VIDEO_CODECS = ["h264", "hevc"] as const;
export type VideoCodec = (typeof VIDEO_CODECS)[number];
export const AUDIO_BITRATES = [320, 256, 192, 128, 96, 64] as const;
export const OFFICE_FORMAT_IDS: ReadonlySet<string> = new Set([
  "doc",
  "docx",
  "odt",
  "rtf",
  "txt",
  "html",
  "xls",
  "xlsx",
  "ods",
  "csv",
  "ppt",
  "pptx",
  "odp",
]);
// Mirrors convt_core::Format::keeps_transparency.
const TRANSPARENT_IMAGE_IDS: ReadonlySet<string> = new Set([
  "png",
  "webp",
  "avif",
  "gif",
  "tiff",
  "bmp",
  "ico",
  "tga",
  "qoi",
  "exr",
  "heic",
]);
// Convt's codec choice applies to these containers only.
export const CODEC_TARGET_IDS: ReadonlySet<string> = new Set(["mp4", "mov", "mkv"]);
// Folders larger than this are scanned partially when finding targets; Convt still converts every file.
const FOLDER_SCAN_LIMIT = 10_000;

export const DOCUMENT_PACK_URL = "https://convt.app/download";
export const DOCUMENT_PACK_HINT = `Office documents in this selection can't be converted because neither Convt's document pack nor LibreOffice is installed. The user can install the pack with the Install Document Pack action in the Convert Files command, by running \`convt pack install documents\`, or by downloading Convt from ${DOCUMENT_PACK_URL}.`;

export function keepsTransparency(format: Format): boolean {
  return format.category !== "image" || TRANSPARENT_IMAGE_IDS.has(format.id);
}

export type ConversionOptions = {
  /** A preset name from `convt presets`. Explicit options override the preset's. */
  preset?: string;
  quality?: number;
  maxSize?: number;
  pages?: string;
  dpi?: number;
  videoHeight?: number;
  videoCodec?: VideoCodec;
  audioBitrate?: number;
  noAudio?: boolean;
  /** transparent, white, black, or a color such as #ff8800. */
  background?: string;
};
export type ConversionRequest = ConversionOptions & {
  /** Files and folders. */
  files: string[];
  target: string;
  outputDirectory?: string;
  /** Include subfolders of folder inputs. */
  recursive?: boolean;
  /** Files converted at once. "auto" lets Convt choose (CPU count). Defaults to 1. */
  jobs?: number | "auto";
};
export type ConversionResult =
  | { status: "done"; input: string; outputs: string[] }
  | { status: "failed" | "cancelled" | "skipped"; input: string; message: string };
export type ConversionProgress = { input: string; completed: number; total: number; fraction: number | null };

export function expandPath(value: string): string {
  return value === "~" ? homedir() : value.startsWith("~/") ? path.join(homedir(), value.slice(2)) : value;
}

export function cliEnvironment(): NodeJS.ProcessEnv {
  const extra =
    process.platform === "win32"
      ? []
      : ["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin", "/bin", "/Applications/LibreOffice.app/Contents/MacOS"];
  // Windows environment keys are case-insensitive; pass only one spelling of PATH.
  const environment = Object.fromEntries(Object.entries(process.env).filter(([key]) => key.toLowerCase() !== "path"));
  const searchPath = process.env.PATH || process.env.Path || "";
  return { ...environment, PATH: [...searchPath.split(path.delimiter), ...extra].join(path.delimiter) };
}

export async function resolveBinary(configuredPath?: string): Promise<string> {
  const configured = configuredPath?.trim();
  if (configured && !path.isAbsolute(expandPath(configured))) {
    throw new ConvtNotFoundError("Convt CLI Path must be an absolute path to the executable.");
  }
  const executable = process.platform === "win32" ? "convt.exe" : "convt";
  const appPaths =
    process.platform === "win32"
      ? [
          path.join(
            process.env.LOCALAPPDATA || path.join(homedir(), "AppData", "Local"),
            "Programs",
            "convt",
            executable,
          ),
          path.join(process.env.ProgramFiles || "C:\\Program Files", "convt", executable),
        ]
      : [
          "/Applications/convt.app/Contents/MacOS/convt",
          path.join(homedir(), "Applications/convt.app/Contents/MacOS/convt"),
        ];
  const candidates = configured
    ? [expandPath(configured)]
    : [
        ...appPaths,
        ...(cliEnvironment().PATH || "")
          .split(path.delimiter)
          .filter(Boolean)
          .map((dir) => path.join(dir, executable)),
      ];
  for (const candidate of candidates) {
    try {
      await access(candidate, process.platform === "win32" ? constants.F_OK : constants.X_OK);
      if ((await stat(candidate)).isFile()) return candidate;
    } catch {
      // Continue to the next installation location.
    }
  }
  throw new ConvtNotFoundError(
    configured ? `No executable found at ${configured}. Update Convt CLI Path in preferences.` : undefined,
  );
}

function abortError(signal: AbortSignal): Error {
  if (signal.reason instanceof Error) return signal.reason;
  const error = new Error("The operation was aborted");
  error.name = "AbortError";
  return error;
}

async function query(binary: string, args: string[], signal?: AbortSignal): Promise<string> {
  if (signal?.aborted) throw abortError(signal);
  try {
    const { stdout } = await execFileAsync(binary, args, {
      env: cliEnvironment(),
      timeout: 30_000,
      maxBuffer: 2 * 1024 * 1024,
      windowsHide: true,
      signal,
    });
    return stdout;
  } catch (error) {
    if (signal?.aborted) throw error;
    if (error instanceof Error && "stderr" in error && typeof error.stderr === "string" && error.stderr.trim()) {
      throw new Error(error.stderr.trim());
    }
    throw error;
  }
}

export function parseFormats(text: string): Format[] {
  const data: unknown = JSON.parse(text);
  if (!Array.isArray(data)) throw new Error("Convt returned an invalid format list. Update Convt and try again.");
  return data.map((value: unknown) => {
    if (
      !value ||
      typeof value !== "object" ||
      !("id" in value) ||
      typeof value.id !== "string" ||
      !/^[a-z0-9]+$/.test(value.id) ||
      !("name" in value) ||
      typeof value.name !== "string" ||
      !("category" in value) ||
      typeof value.category !== "string" ||
      !("extensions" in value) ||
      !Array.isArray(value.extensions) ||
      !value.extensions.every((extension: unknown) => typeof extension === "string")
    )
      throw new Error("Convt returned an invalid format. Update Convt and try again.");
    return { id: value.id, name: value.name, category: value.category, extensions: value.extensions };
  });
}

export async function getFormats(binary: string, signal?: AbortSignal): Promise<Format[]> {
  return parseFormats(await query(binary, ["formats", "--json"], signal));
}

export async function validateFiles(files: string[]): Promise<string[]> {
  if (!files.length) throw new Error("Choose at least one file or folder.");
  const normalized = [
    ...new Set(
      files.map((file) => {
        const expanded = expandPath(file);
        if (!path.isAbsolute(expanded) || expanded.includes("\0"))
          throw new Error("Choose files and folders using absolute paths.");
        return path.resolve(expanded);
      }),
    ),
  ];
  for (const file of normalized) {
    const info = await stat(file).catch(() => {
      throw new Error(`Cannot read ${file}. Check that it still exists.`);
    });
    if (!info.isFile() && !info.isDirectory()) throw new Error(`${path.basename(file)} is not a file or folder.`);
    await access(file, constants.R_OK);
  }
  return normalized;
}

export type DiscoveryOptions = { recursive?: boolean; signal?: AbortSignal };

// Lists a folder the way convt_core::expand_inputs does: hidden entries are skipped,
// symlinked folders are not followed, and subfolders are read only when recursive.
async function listFolder(
  folder: string,
  recursive: boolean,
  signal?: AbortSignal,
): Promise<{ files: string[]; complete: boolean }> {
  const files: string[] = [];
  const pending = [folder];
  for (let directory = pending.shift(); directory !== undefined; directory = pending.shift()) {
    if (signal?.aborted) throw abortError(signal);
    const entries = await readdir(directory, { withFileTypes: true }).catch(() => {
      throw new Error(`Cannot read the folder ${directory}. Check its permissions.`);
    });
    for (const entry of entries) {
      if (entry.name.startsWith(".")) continue;
      const entryPath = path.join(directory, entry.name);
      if (entry.isDirectory()) {
        if (recursive) pending.push(entryPath);
        continue;
      }
      const isFile =
        entry.isFile() || (entry.isSymbolicLink() && (await stat(entryPath).catch(() => undefined))?.isFile());
      if (!isFile) continue;
      if (files.length >= FOLDER_SCAN_LIMIT) return { files, complete: false };
      files.push(entryPath);
    }
  }
  return { files, complete: true };
}

function identify(formats: Format[], file: string): Source {
  const extension = path.extname(file).slice(1).toLowerCase();
  const format = formats.find((format) => format.extensions.some((alias) => alias.toLowerCase() === extension)) ?? null;
  return { file, extension, format };
}

type Selection = {
  formats: Format[];
  /** Files named directly. */
  sources: Source[];
  folders: { folder: string; sources: Source[]; complete: boolean }[];
};

async function readSelection(binary: string, files: string[], options: DiscoveryOptions): Promise<Selection> {
  const inputs = await validateFiles(files);
  if (options.signal?.aborted) throw abortError(options.signal);
  const formats = await getFormats(binary, options.signal);
  const selection: Selection = { formats, sources: [], folders: [] };
  for (const input of inputs) {
    if ((await stat(input)).isDirectory()) {
      const { files, complete } = await listFolder(input, options.recursive ?? false, options.signal);
      selection.folders.push({ folder: input, sources: files.map((file) => identify(formats, file)), complete });
    } else {
      selection.sources.push(identify(formats, input));
    }
  }
  return selection;
}

function allSources(selection: Selection): Source[] {
  return [...selection.sources, ...selection.folders.flatMap((folder) => folder.sources)];
}

type TargetLists = Map<string, { targets: string[]; menu: string[] }>;

// Targets depend on the source format and installed engines, not file contents, so query each extension once.
async function queryTargets(binary: string, sources: Source[], menu: boolean, signal?: AbortSignal) {
  const byExtension = new Map<string, string>();
  for (const source of sources) if (source.format) byExtension.set(source.extension, source.file);
  const lists: TargetLists = new Map();
  for (const [extension, file] of byExtension) {
    const targets = await queryTargetIds(binary, file, false, signal);
    lists.set(extension, { targets, menu: menu ? await queryTargetIds(binary, file, true, signal) : [] });
  }
  return lists;
}

async function queryTargetIds(binary: string, file: string, menu: boolean, signal?: AbortSignal): Promise<string[]> {
  return (await query(binary, ["targets", file, ...(menu ? ["--menu"] : [])], signal))
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter(Boolean);
}

function commonTargetIds(lists: string[][]): Set<string> {
  let common = new Set(lists[0]);
  for (const list of lists.slice(1)) {
    const ids = new Set(list);
    common = new Set([...common].filter((id) => ids.has(id)));
  }
  return common;
}

// Every named file must reach a target. Convt skips folder files that can't, so with named
// files the folders don't narrow the list; folders alone need one file, in any of them, that does.
function selectionTargetIds(selection: Selection, list: (source: Source) => string[]): Set<string> {
  if (selection.sources.length) return commonTargetIds(selection.sources.map(list));
  const folderSources = selection.folders.flatMap((folder) => folder.sources).filter((source) => source.format);
  return new Set(folderSources.flatMap(list));
}

function sortedFormats(formats: Format[], ids: Set<string>): Format[] {
  return formats
    .filter((format) => ids.has(format.id))
    .sort((a, b) => a.category.localeCompare(b.category) || a.name.localeCompare(b.name));
}

async function discoverTargets(binary: string, files: string[], options: DiscoveryOptions & { menu: boolean }) {
  const selection = await readSelection(binary, files, options);
  // An unrecognized named file can't convert to anything; skip the queries.
  const lists: TargetLists = selection.sources.some((source) => source.format === null)
    ? new Map()
    : await queryTargets(binary, allSources(selection), options.menu, options.signal);
  const targetsOf = (source: Source) => lists.get(source.extension)?.targets ?? [];
  const targets = sortedFormats(selection.formats, selectionTargetIds(selection, targetsOf));
  return { selection, lists, targets, targetsOf };
}

export async function getTargets(binary: string, files: string[], options: DiscoveryOptions = {}): Promise<Format[]> {
  return (await discoverTargets(binary, files, { ...options, menu: false })).targets;
}

export async function inspectFiles(
  binary: string,
  files: string[],
  options: DiscoveryOptions = {},
): Promise<FileInspection> {
  const { selection, lists, targets, targetsOf } = await discoverTargets(binary, files, { ...options, menu: true });
  const supported = new Set(targets.map((format) => format.id));
  const suggestedTargetIds = [
    ...selectionTargetIds(selection, (source) => lists.get(source.extension)?.menu ?? []),
  ].filter((id) => supported.has(id));
  const documentPackMissing = allSources(selection).some(
    (source) =>
      source.format &&
      OFFICE_FORMAT_IDS.has(source.format.id) &&
      lists.has(source.extension) &&
      !targetsOf(source).length,
  );
  const folders = selection.folders.map(({ folder, sources, complete }): FolderSource => {
    const counts = new Map<string, { format: Format; files: number }>();
    for (const { format } of sources) {
      if (!format) continue;
      const entry = counts.get(format.id) ?? { format, files: 0 };
      entry.files++;
      counts.set(format.id, entry);
    }
    return { folder, files: sources.length, formats: [...counts.values()], complete };
  });
  return { sources: selection.sources, folders, targets, suggestedTargetIds, documentPackMissing };
}

export function parsePresets(text: string): Preset[] {
  // `convt presets` prints `name<padding> target` (`-` without one) or `name<padding> invalid: reason`.
  return text.split(/\r?\n/).flatMap((line): Preset[] => {
    if (/^.+? +invalid: /.test(line)) return [];
    // A preset's `to` is printed as written; Convt accepts any case and extension aliases (jpg, JPEG).
    const match = /^(.+?) +(\S+)$/.exec(line.trimEnd());
    return match ? [{ name: match[1], target: match[2] === "-" ? null : match[2] }] : [];
  });
}

/** The format id Convt picks for a preset's `to`: an id or an extension, in any case. */
export function resolveFormatId(formats: Format[], value: string): string | null {
  const wanted = value.toLowerCase();
  const format =
    formats.find((entry) => entry.id === wanted) ??
    formats.find((entry) => entry.extensions.some((extension) => extension.toLowerCase() === wanted));
  return format?.id ?? null;
}

export async function getPresets(binary: string, signal?: AbortSignal): Promise<Preset[]> {
  return parsePresets(await query(binary, ["presets"], signal));
}

export type PackProgress =
  { kind: "download"; bytes: number; total: number | null } | { kind: "verifying" } | { kind: "installing" };

export function parsePackProgress(line: string): PackProgress | null {
  const download = /^Downloaded (\d+) bytes(?: of (\d+))?$/.exec(line.trim());
  if (download)
    return { kind: "download", bytes: Number(download[1]), total: download[2] ? Number(download[2]) : null };
  if (line.startsWith("Verifying")) return { kind: "verifying" };
  if (line.startsWith("Installing document pack")) return { kind: "installing" };
  return null;
}

/** Runs `convt pack install documents`, which downloads and verifies Convt's document pack. */
export async function installDocumentPack(
  binary: string,
  callbacks: { signal?: AbortSignal; onProgress?: (progress: PackProgress) => void } = {},
): Promise<void> {
  if (callbacks.signal?.aborted) throw abortError(callbacks.signal);
  return new Promise((resolve, reject) => {
    const child = spawn(binary, ["pack", "install", "documents"], {
      env: cliEnvironment(),
      windowsHide: true,
      stdio: ["ignore", "ignore", "pipe"],
      signal: callbacks.signal,
    });
    let message = "";
    const lines = createInterface({ input: child.stderr });
    lines.on("line", (line) => {
      const progress = parsePackProgress(line);
      if (progress) callbacks.onProgress?.(progress);
      else if (line.trim()) message = `${message}${line}\n`.slice(-4000);
    });
    child.on("error", reject);
    child.on("close", (code) => {
      lines.close();
      if (code === 0) resolve();
      else
        reject(
          new Error(message.trim() || `Convt exited ${code ?? "unexpectedly"} while installing the document pack.`),
        );
    });
  });
}

export function validateOptions(options: ConversionOptions): void {
  for (const [label, value, min, max] of [
    ["Quality", options.quality, 1, 100],
    ["Maximum image size", options.maxSize, 1, 4_294_967_295],
    ["DPI", options.dpi, 18, 1200],
    ["Audio bitrate", options.audioBitrate, 8, 640],
    ["Maximum video height", options.videoHeight, 2, 4_294_967_294],
  ] satisfies [string, number | undefined, number, number][]) {
    if (value !== undefined && (!Number.isInteger(value) || value < min || value > max)) {
      throw new Error(`${label} must be a whole number between ${min} and ${max}.`);
    }
  }
  if (options.videoHeight !== undefined && options.videoHeight % 2 !== 0) {
    throw new Error("Maximum video height must be an even number of pixels, such as 1080 or 720.");
  }
  if (options.videoCodec !== undefined && !VIDEO_CODECS.some((codec) => codec === options.videoCodec)) {
    throw new Error("Video codec must be h264 or hevc.");
  }
  if (
    options.background !== undefined &&
    !/^(?:transparent|white|black|#?[0-9a-f]{3}|#?[0-9a-f]{6})$/i.test(options.background)
  ) {
    throw new Error("Background must be transparent, white, black, or a color such as #ff8800.");
  }
  if (options.preset !== undefined && !/^[^/\\\0\r\n]+$/.test(options.preset)) {
    throw new Error("Choose a preset by its name from Convt's presets.");
  }
  if (options.pages !== undefined) {
    const match = /^([1-9]\d*)(?:-([1-9]\d*)?)?$/.exec(options.pages);
    if (
      !match ||
      Number(match[1]) > 4_294_967_295 ||
      (match[2] && (Number(match[2]) < Number(match[1]) || Number(match[2]) > 4_294_967_295))
    ) {
      throw new Error("PDF pages must be a page or range, such as 2, 1-3, or 4-.");
    }
  }
}

export function conversionArgs(request: ConversionRequest): string[] {
  validateOptions(request);
  if (!/^[a-z0-9]+$/.test(request.target)) throw new Error("Choose a valid output format.");
  const jobs = request.jobs ?? 1;
  if (jobs !== "auto" && (!Number.isInteger(jobs) || jobs < 1 || jobs > 256))
    throw new Error("Parallel conversions must be Auto or a whole number between 1 and 256.");
  const args = ["--json", ...(jobs === "auto" ? [] : ["--jobs", String(jobs)]), "--to", request.target];
  // `=` keeps a preset name that starts with a dash from reading as a flag.
  if (request.preset !== undefined) args.push(`--preset=${request.preset}`);
  if (request.recursive) args.push("--recursive");
  if (request.outputDirectory) args.push("--out-dir", request.outputDirectory);
  if (request.quality !== undefined) args.push("--quality", String(request.quality));
  if (request.maxSize !== undefined) args.push("--max-size", String(request.maxSize));
  if (request.pages !== undefined) args.push("--pages", request.pages);
  if (request.dpi !== undefined) args.push("--dpi", String(request.dpi));
  if (request.background !== undefined) args.push("--background", request.background);
  if (request.videoHeight !== undefined) args.push("--video-height", String(request.videoHeight));
  if (request.videoCodec !== undefined) args.push("--video-codec", request.videoCodec);
  if (request.audioBitrate !== undefined) args.push("--audio-bitrate", String(request.audioBitrate));
  if (request.noAudio) args.push("--no-audio");
  return [...args, "--", ...request.files];
}

type Event =
  | { event: "started"; index: number; input: string }
  | { event: "progress"; index: number; fraction: number | null }
  | { event: "done"; index: number; input: string; outputs: string[] }
  | { event: "failed"; index: number; input: string; message: string; cancelled: boolean }
  | { event: "summary" };

export function parseEvent(line: string): Event {
  const value: unknown = JSON.parse(line);
  if (!value || typeof value !== "object" || !("event" in value)) throw new Error("Invalid Convt event.");
  if (value.event === "summary") return { event: "summary" };
  if (!("index" in value) || typeof value.index !== "number" || !Number.isInteger(value.index) || value.index < 0)
    throw new Error("Invalid Convt event index.");
  const index = value.index;
  if (value.event === "progress") {
    if (
      "fraction" in value &&
      (value.fraction === null || (typeof value.fraction === "number" && value.fraction >= 0 && value.fraction <= 1))
    )
      return { event: "progress", index, fraction: value.fraction };
    throw new Error("Unrecognized Convt event. Update Convt and try again.");
  }
  // Folder inputs expand into many jobs, so the job's input comes from the event, not the argument list.
  if (!("input" in value) || typeof value.input !== "string" || !path.isAbsolute(value.input))
    throw new Error("Convt did not report which file an event belongs to. Update Convt and try again.");
  const input = value.input;
  switch (value.event) {
    case "started":
      return { event: "started", index, input };
    case "done":
      if (
        !("outputs" in value) ||
        !Array.isArray(value.outputs) ||
        !value.outputs.length ||
        !value.outputs.every((output: unknown) => typeof output === "string" && path.isAbsolute(output))
      )
        break;
      return { event: "done", index, input, outputs: value.outputs };
    case "failed":
      if (!("message" in value) || typeof value.message !== "string") break;
      return {
        event: "failed",
        index,
        input,
        message: value.message,
        cancelled: "kind" in value && value.kind === "cancelled",
      };
  }
  throw new Error("Unrecognized Convt event. Update Convt and try again.");
}

function isInside(folder: string, file: string): boolean {
  const relative = path.relative(folder, file);
  return !!relative && !relative.startsWith("..") && !path.isAbsolute(relative);
}

export async function convertFiles(
  binary: string,
  request: ConversionRequest,
  callbacks: { signal?: AbortSignal; onProgress?: (progress: ConversionProgress) => void } = {},
): Promise<ConversionResult[]> {
  validateOptions(request);
  const files = await validateFiles(request.files);
  const target = request.target.trim().toLowerCase();
  const cancelled = (): ConversionResult[] =>
    files.map((input) => ({ status: "cancelled", input, message: "Conversion cancelled." }));
  if (callbacks.signal?.aborted) return cancelled();
  let discovery: Awaited<ReturnType<typeof discoverTargets>>;
  try {
    discovery = await discoverTargets(binary, files, {
      recursive: request.recursive,
      signal: callbacks.signal,
      menu: false,
    });
  } catch (error) {
    if (callbacks.signal?.aborted) return cancelled();
    throw error;
  }
  const { selection, targets, targetsOf } = discovery;
  const targetFormat = targets.find((format) => format.id === target);
  if (!targetFormat)
    throw new Error(
      selection.folders.length
        ? `Nothing in this selection converts to ${target}. Choose one of its available formats.`
        : `These files cannot all convert to ${target}. Choose one of their available formats.`,
    );
  if (request.background?.toLowerCase() === "transparent" && !keepsTransparency(targetFormat))
    throw new Error(`${targetFormat.name} can't store transparency. Choose a background color such as white or black.`);
  const outputDirectory = request.outputDirectory ? expandPath(request.outputDirectory) : undefined;
  if (outputDirectory) {
    if (!path.isAbsolute(outputDirectory)) throw new Error("Choose an existing output folder.");
    const info = await stat(outputDirectory).catch((error: NodeJS.ErrnoException) => {
      if (error.code === "ENOENT" || error.code === "ENOTDIR") return undefined;
      throw error;
    });
    if (!info?.isDirectory()) throw new Error("Choose an existing output folder.");
    await access(outputDirectory, constants.W_OK);
  }
  const args = conversionArgs({ ...request, files, target, outputDirectory });
  if (callbacks.signal?.aborted) return cancelled();
  // Predict each folder's jobs the way Convt filters folder files; unknown when a folder was too large to scan.
  const folderJobs = selection.folders.map((folder) =>
    folder.complete
      ? folder.sources.filter(
          (source) => source.format && source.format.id !== target && targetsOf(source).includes(target),
        ).length
      : undefined,
  );
  const expectedJobs = folderJobs.every((jobs) => jobs !== undefined)
    ? selection.sources.length + folderJobs.reduce<number>((sum, jobs) => sum + (jobs ?? 0), 0)
    : 0;
  // A fully scanned folder contributes nothing exactly when no file in it passes that filter. For a
  // folder too large to scan, look for a result inside it that isn't a file named directly; a named
  // file reported more often than it was named also ran as a folder job.
  const named = new Map<string, number>();
  for (const { file } of selection.sources) named.set(file, (named.get(file) ?? 0) + 1);
  const contributes = (index: number, folder: string, reported: ConversionResult[]) => {
    if (folderJobs[index] !== undefined) return folderJobs[index] > 0;
    const seen = new Map<string, number>();
    for (const { input } of reported) seen.set(input, (seen.get(input) ?? 0) + 1);
    return [...seen].some(([input, count]) => isInside(folder, input) && count > (named.get(input) ?? 0));
  };

  return new Promise((resolve, reject) => {
    const child = spawn(binary, args, { env: cliEnvironment(), windowsHide: true, stdio: ["ignore", "pipe", "pipe"] });
    // Keyed by job index; a file named directly and found in a folder runs twice.
    const results = new Map<number, ConversionResult>();
    const inputs = new Map<number, string>();
    let finished = false;
    let stderr = "";
    let protocolError = "";
    const cancel = () => {
      if (process.platform === "win32" && child.pid) {
        // Node's SIGINT terminates immediately on Windows; stop engine children too.
        execFile("taskkill", ["/PID", String(child.pid), "/T", "/F"], { windowsHide: true }, (error) => {
          if (error) child.kill();
        });
      } else {
        child.kill("SIGINT");
      }
    };
    callbacks.signal?.addEventListener("abort", cancel, { once: true });
    const lines = createInterface({ input: child.stdout });
    child.stderr.setEncoding("utf8").on("data", (chunk: string) => {
      stderr = (stderr + chunk).slice(-8000);
    });
    lines.on("line", (line) => {
      if (!line.trim()) return;
      try {
        const event = parseEvent(line);
        if (event.event === "summary") {
          finished = true;
          return;
        }
        const input = event.event === "progress" ? inputs.get(event.index) : event.input;
        if (input === undefined) throw new Error("Convt returned an unexpected file index.");
        inputs.set(event.index, input);
        if (event.event === "done") results.set(event.index, { status: "done", input, outputs: event.outputs });
        if (event.event === "failed")
          results.set(event.index, { status: event.cancelled ? "cancelled" : "failed", input, message: event.message });
        callbacks.onProgress?.({
          input,
          completed: results.size,
          total: Math.max(expectedJobs, inputs.size),
          fraction: event.event === "progress" ? event.fraction : null,
        });
      } catch (error) {
        protocolError = error instanceof Error ? error.message : String(error);
      }
    });
    child.on("error", reject);
    child.on("close", (code) => {
      lines.close();
      callbacks.signal?.removeEventListener("abort", cancel);
      const cancelled = callbacks.signal?.aborted || code === 130;
      const missing = (input: string): ConversionResult => ({
        status: cancelled ? "cancelled" : "failed",
        input,
        message: cancelled
          ? "Conversion cancelled."
          : stderr.trim() ||
            protocolError ||
            `Convt exited ${code ?? "unexpectedly"} without reporting an output. Check the output folder before retrying.`,
      });
      const reported = [...results].sort(([a], [b]) => a - b).map(([, result]) => result);
      const reportedInputs = new Set(reported.map((result) => result.input));
      resolve([
        ...reported,
        ...selection.sources.filter(({ file }) => !reportedInputs.has(file)).map(({ file }) => missing(file)),
        // Without a summary, Convt stopped before reporting every folder file. With one, a folder
        // without results had nothing that converts to the target, which Convt skips quietly; say so.
        ...selection.folders
          .filter(({ folder }, index) => !finished || !contributes(index, folder, reported))
          .map(({ folder }): ConversionResult =>
            finished
              ? {
                  status: "skipped",
                  input: folder,
                  message: `Nothing in this folder converts to ${targetFormat.name}.`,
                }
              : missing(folder),
          ),
      ]);
    });
  });
}
