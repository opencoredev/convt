// A conversion the user asked for, and what the toast shows about it.

import { isTarget, type SourceKind, type Target } from "./formats.ts";

export type JobId = string & { readonly __brand: "JobId" };

export function newJobId(): JobId {
  // Branding a fresh UUID: the only place a JobId is minted.
  return crypto.randomUUID() as JobId;
}

export function parseJobId(value: unknown): JobId | null {
  return typeof value === "string" && /^[0-9a-f-]{36}$/.test(value) ? (value as JobId) : null;
}

export type Action = { kind: "save"; target: Target } | { kind: "copy" };

export type Job = {
  id: JobId;
  srcUrl: string;
  pageUrl: string;
  tabId: number;
  /** The frame the image is in. blob: URLs only resolve there. */
  frameId: number;
  action: Action;
};

/** Something worth one extra line under a finished conversion. */
export type Note =
  | { kind: "animated"; from: SourceKind }
  /** The occasional pointer to the desktop app. */
  | { kind: "tip" };

/** Why a conversion didn't finish, in terms the toast can explain. */
export type Problem =
  /** The image host refused a cross-origin read and we lack host access. */
  | { kind: "blocked"; host: string }
  /** We have access, but the request still failed (offline, hotlink protection). */
  | { kind: "network" }
  | { kind: "http"; status: number }
  /** The URL answered with something that isn't an image (a login page, say). */
  | { kind: "not-image" }
  /** A real image Chrome can't decode, such as HEIC or TIFF. */
  | { kind: "unreadable"; from: SourceKind }
  | { kind: "too-large" }
  | { kind: "save-failed"; reason: string }
  | { kind: "clipboard" }
  /** Copy as PNG on a page Chrome protects, such as the Web Store or the PDF viewer. */
  | { kind: "copy-unavailable" }
  /** A blob: image inside a frame from another site, which convt can't reach. */
  | { kind: "in-frame" }
  /** Something unexpected broke; the details are in the worker's console. */
  | { kind: "internal" };

export type ToastView =
  | { phase: "working"; jobId: JobId; thumb: string; action: Action }
  | {
      phase: "saved";
      jobId: JobId;
      thumb: string;
      name: string;
      bytes: number;
      from: SourceKind;
      target: Target;
      downloadId: number;
      note: Note | null;
    }
  | {
      phase: "copied";
      jobId: JobId;
      thumb: string;
      bytes: number;
      from: SourceKind;
      note: Note | null;
    }
  | { phase: "failed"; jobId: JobId; thumb: string; action: Action; problem: Problem }
  /** Remove the toast without a message, e.g. the user cancelled Save As. */
  | { phase: "dismissed"; jobId: JobId };

/** A finished conversion, listed in the popup. */
export type RecentItem =
  | {
      kind: "saved";
      jobId: JobId;
      at: number;
      name: string;
      bytes: number;
      target: Target;
      thumb: string;
      downloadId: number;
    }
  | { kind: "copied"; jobId: JobId; at: number; bytes: number; thumb: string };

export const MAX_RECENT = 8;

export function parseAction(value: unknown): Action | null {
  if (typeof value !== "object" || value === null || !("kind" in value)) return null;
  if (value.kind === "copy") return { kind: "copy" };
  if (value.kind === "save" && "target" in value && isTarget(value.target)) {
    return { kind: "save", target: value.target };
  }
  return null;
}

/** Context menu item ids, one per action. */
export function menuId(action: Action): string {
  return action.kind === "save" ? `save:${action.target}` : "copy:png";
}

export function actionFromMenuId(id: string | number): Action | null {
  if (id === "copy:png") return { kind: "copy" };
  const target = typeof id === "string" && id.startsWith("save:") ? id.slice(5) : null;
  return isTarget(target) ? { kind: "save", target } : null;
}

export function parseRecent(value: unknown): RecentItem[] {
  if (!Array.isArray(value)) return [];
  return value.flatMap((item): RecentItem[] => {
    if (typeof item !== "object" || item === null) return [];
    const jobId = "jobId" in item ? parseJobId(item.jobId) : null;
    const at = "at" in item && typeof item.at === "number" ? item.at : null;
    const bytes = "bytes" in item && typeof item.bytes === "number" ? item.bytes : null;
    const thumb = "thumb" in item && typeof item.thumb === "string" ? item.thumb : null;
    if (jobId === null || at === null || bytes === null || thumb === null) return [];
    if ("kind" in item && item.kind === "copied")
      return [{ kind: "copied", jobId, at, bytes, thumb }];
    if (
      "kind" in item &&
      item.kind === "saved" &&
      "name" in item &&
      typeof item.name === "string" &&
      "target" in item &&
      isTarget(item.target) &&
      "downloadId" in item &&
      typeof item.downloadId === "number"
    ) {
      return [
        {
          kind: "saved",
          jobId,
          at,
          bytes,
          thumb,
          name: item.name,
          target: item.target,
          downloadId: item.downloadId,
        },
      ];
    }
    return [];
  });
}

/** Animated sources and the occasional desktop tip; never both, never on every save. */
export function noteFor(input: {
  animated: boolean;
  from: SourceKind;
  count: number;
}): Note | null {
  if (input.animated) return { kind: "animated", from: input.from };
  return TIP_AT.includes(input.count) ? { kind: "tip" } : null;
}

/** Which finished conversions (1-based) get the desktop tip. */
const TIP_AT = [3, 25, 100];
