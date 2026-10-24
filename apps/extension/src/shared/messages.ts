// Messages between the background worker, the offscreen converter, the in-page toast
// and the extension's own pages. Each receiver parses what arrives before acting on it.

import { isSourceKind, isTarget, type SourceKind, type Target } from "./formats.ts";
import {
  parseAction,
  parseJobId,
  type Job,
  type JobId,
  type Note,
  type Problem,
  type ToastView,
} from "./jobs.ts";

/** From the toast, the popup and the welcome page to the background worker. */
export type ToBackground =
  | { kind: "show-download"; downloadId: number }
  | { kind: "open-access"; jobId: JobId }
  | { kind: "access-granted"; jobId: JobId }
  /** The popup's Clear, run in the worker so it can't race a save finishing. */
  | { kind: "clear-recent" }
  /** Only handled in end-to-end test builds, where native menus can't be clicked. */
  | { kind: "test:run"; job: Job };

/** From the background worker to the toast script in a page, or to convt's own pages. */
export type ToContent =
  | { kind: "toast"; view: ToastView }
  /**
   * `tabId` is set when the target is one of convt's own pages, which all hear
   * runtime broadcasts: only the page in that tab may answer.
   */
  | { kind: "copy-image"; png: string; tabId: number | null };

export type CopyResult = { ok: boolean };

/**
 * From the background worker to the offscreen document. Messages are JSON and Chrome
 * caps them at 64 MiB, so the source travels as base64 (capped well below that) and
 * the result comes back as a blob: URL Chrome can download, unless the caller needs
 * the bytes themselves (Copy as PNG).
 */
export type TranscodeRequest = {
  target: "offscreen";
  kind: "transcode";
  /** Source image, base64. */
  data: string;
  from: SourceKind;
  to: Target;
  quality: number;
  deliver: "url" | "bytes";
};

/** Frees a blob: URL once Chrome has finished downloading it. */
export type ReleaseRequest = { target: "offscreen"; kind: "release"; url: string };

export type TranscodeOutput = { kind: "url"; url: string } | { kind: "bytes"; data: string };

export type TranscodeResult =
  | { ok: true; output: TranscodeOutput; bytes: number; thumb: string }
  | { ok: false; problem: "unreadable" | "too-large" | "encode" };

type Obj = Record<string, unknown>;

function isObj(value: unknown): value is Obj {
  return typeof value === "object" && value !== null;
}

function str(o: Obj, key: string): string | null {
  const v = o[key];
  return typeof v === "string" ? v : null;
}

function num(o: Obj, key: string): number | null {
  const v = o[key];
  return typeof v === "number" && Number.isFinite(v) ? v : null;
}

export function parseToBackground(value: unknown): ToBackground | null {
  if (!isObj(value)) return null;
  switch (value.kind) {
    case "show-download": {
      const downloadId = num(value, "downloadId");
      return downloadId === null ? null : { kind: "show-download", downloadId };
    }
    case "open-access":
    case "access-granted": {
      const jobId = parseJobId(value.jobId);
      return jobId === null ? null : { kind: value.kind, jobId };
    }
    case "clear-recent":
      return { kind: "clear-recent" };
    case "test:run": {
      const job = parseJob(value.job);
      return job === null ? null : { kind: "test:run", job };
    }
    default:
      return null;
  }
}

export function parseJob(value: unknown): Job | null {
  if (!isObj(value)) return null;
  const id = parseJobId(value.id);
  const srcUrl = str(value, "srcUrl");
  const pageUrl = str(value, "pageUrl");
  const tabId = num(value, "tabId");
  const frameId = num(value, "frameId");
  const action = parseAction(value.action);
  if (id === null || srcUrl === null || pageUrl === null || tabId === null) return null;
  if (frameId === null || action === null) return null;
  return { id, srcUrl, pageUrl, tabId, frameId, action };
}

export function parseToContent(value: unknown): ToContent | null {
  if (!isObj(value)) return null;
  if (value.kind === "copy-image") {
    const png = str(value, "png");
    const tabId = value.tabId === null ? null : num(value, "tabId");
    if (png === null || (value.tabId !== null && tabId === null)) return null;
    return { kind: "copy-image", png, tabId };
  }
  if (value.kind === "toast") {
    const view = parseToastView(value.view);
    return view === null ? null : { kind: "toast", view };
  }
  return null;
}

function parseNote(value: unknown): Note | null {
  if (!isObj(value)) return null;
  if (value.kind === "tip") return { kind: "tip" };
  if (value.kind === "animated" && isSourceKind(value.from)) {
    return { kind: "animated", from: value.from };
  }
  return null;
}

function parseProblem(value: unknown): Problem | null {
  if (!isObj(value)) return null;
  switch (value.kind) {
    case "blocked": {
      const host = str(value, "host");
      return host === null ? null : { kind: "blocked", host };
    }
    case "http": {
      const status = num(value, "status");
      return status === null ? null : { kind: "http", status };
    }
    case "unreadable":
      return isSourceKind(value.from) ? { kind: "unreadable", from: value.from } : null;
    case "save-failed": {
      const reason = str(value, "reason");
      return reason === null ? null : { kind: "save-failed", reason };
    }
    case "network":
    case "not-image":
    case "too-large":
    case "clipboard":
    case "copy-unavailable":
    case "in-frame":
    case "internal":
      return { kind: value.kind };
    default:
      return null;
  }
}

export function parseToastView(value: unknown): ToastView | null {
  if (!isObj(value)) return null;
  const jobId = parseJobId(value.jobId);
  if (jobId === null) return null;
  if (value.phase === "dismissed") return { phase: "dismissed", jobId };
  const thumb = str(value, "thumb");
  if (thumb === null) return null;
  switch (value.phase) {
    case "working": {
      const action = parseAction(value.action);
      return action === null ? null : { phase: "working", jobId, thumb, action };
    }
    case "failed": {
      const action = parseAction(value.action);
      const problem = parseProblem(value.problem);
      if (action === null || problem === null) return null;
      return { phase: "failed", jobId, thumb, action, problem };
    }
    case "saved":
    case "copied": {
      const bytes = num(value, "bytes");
      const from = isSourceKind(value.from) ? value.from : null;
      const note = value.note === null ? null : parseNote(value.note);
      if (bytes === null || from === null || (value.note !== null && note === null)) return null;
      if (value.phase === "copied") return { phase: "copied", jobId, thumb, bytes, from, note };
      const name = str(value, "name");
      const downloadId = num(value, "downloadId");
      const target = isTarget(value.target) ? value.target : null;
      if (name === null || downloadId === null || target === null) return null;
      return { phase: "saved", jobId, thumb, name, bytes, from, target, downloadId, note };
    }
    default:
      return null;
  }
}

export function parseOffscreenRequest(value: unknown): TranscodeRequest | ReleaseRequest | null {
  if (!isObj(value) || value.target !== "offscreen") return null;
  if (value.kind === "release") {
    const url = str(value, "url");
    return url === null ? null : { target: "offscreen", kind: "release", url };
  }
  if (value.kind !== "transcode") return null;
  const data = str(value, "data");
  const quality = num(value, "quality");
  const deliver = value.deliver === "url" || value.deliver === "bytes" ? value.deliver : null;
  if (data === null || quality === null || deliver === null) return null;
  if (!isSourceKind(value.from) || !isTarget(value.to)) return null;
  return {
    target: "offscreen",
    kind: "transcode",
    data,
    from: value.from,
    to: value.to,
    quality,
    deliver,
  };
}

function parseTranscodeOutput(value: unknown): TranscodeOutput | null {
  if (!isObj(value)) return null;
  if (value.kind === "url") {
    const url = str(value, "url");
    return url?.startsWith("blob:") ? { kind: "url", url } : null;
  }
  if (value.kind === "bytes") {
    const data = str(value, "data");
    return data === null ? null : { kind: "bytes", data };
  }
  return null;
}

export function parseTranscodeResult(value: unknown): TranscodeResult | null {
  if (!isObj(value)) return null;
  if (value.ok === true) {
    const output = parseTranscodeOutput(value.output);
    const bytes = num(value, "bytes");
    const thumb = str(value, "thumb");
    return output === null || bytes === null || thumb === null
      ? null
      : { ok: true, output, bytes, thumb };
  }
  if (
    value.ok === false &&
    (value.problem === "unreadable" || value.problem === "too-large" || value.problem === "encode")
  ) {
    return { ok: false, problem: value.problem };
  }
  return null;
}

export function parseCopyResult(value: unknown): CopyResult {
  return { ok: isObj(value) && value.ok === true };
}
