// What a toast says for each state. Kept apart from the DOM so every message can be
// read, tested and changed in one place.

import { sourceInfo, targetInfo, type SourceKind } from "./formats.ts";
import type { JobId, Note, Problem, ToastView } from "./jobs.ts";
import { siteUrl, type LinkSource } from "./links.ts";
import { formatBytes, outputName } from "./naming.ts";

export type ToastAction =
  | { kind: "show-download"; label: string; downloadId: number }
  | { kind: "open-access"; label: string; jobId: JobId }
  | { kind: "link"; label: string; href: string };

export type ToastTone = "working" | "done" | "failed";

export type ToastCopy = {
  tone: ToastTone;
  title: string;
  /** Second line. `mono` lines are file facts: name, formats, size. */
  detail: { text: string; mono: boolean };
  action: ToastAction | null;
  note: { text: string; link: { label: string; href: string } | null } | null;
  /** How long it stays up, unless hovered or focused. Null stays until closed. */
  lingerMs: number | null;
};

const desktopLink = (source: LinkSource) => ({
  label: "Get convt for desktop",
  href: siteUrl("/download", source),
});

function conversion(from: SourceKind, to: string, bytes: number): string {
  return `${sourceInfo[from].label} → ${to} · ${formatBytes(bytes)}`;
}

export function toastCopy(
  view: Exclude<ToastView, { phase: "dismissed" }>,
  srcUrl: string,
): ToastCopy {
  switch (view.phase) {
    case "working": {
      const target = view.action.kind === "save" ? view.action.target : "png";
      return {
        tone: "working",
        title:
          view.action.kind === "save"
            ? `Saving as ${targetInfo[target].label}…`
            : "Copying as PNG…",
        detail: { text: outputName(srcUrl, target), mono: true },
        action: null,
        note: null,
        // Conversions take a second or two. This only clears a toast whose worker died.
        lingerMs: 120_000,
      };
    }
    case "saved":
      return {
        tone: "done",
        title: view.name,
        detail: {
          text: conversion(view.from, targetInfo[view.target].label, view.bytes),
          mono: true,
        },
        action: { kind: "show-download", label: "Show in folder", downloadId: view.downloadId },
        note: noteCopy(view.note),
        lingerMs: view.note ? 9000 : 5000,
      };
    case "copied":
      return {
        tone: "done",
        title: "Copied as PNG",
        detail: { text: conversion(view.from, "PNG", view.bytes), mono: true },
        action: null,
        note: noteCopy(view.note),
        lingerMs: view.note ? 9000 : 4000,
      };
    case "failed":
      return problemCopy(view.problem, view.jobId);
    default: {
      const _exhaustive: never = view;
      return _exhaustive;
    }
  }
}

function noteCopy(note: Note | null): ToastCopy["note"] {
  if (note === null) return null;
  switch (note.kind) {
    case "animated":
      // convt for desktop turns GIFs into MP4 (FFmpeg); it doesn't read animated WebP,
      // AVIF or APNG frames, so only GIF gets the offer.
      return note.from === "gif"
        ? {
            text: "Only the first frame was saved. convt for desktop turns GIFs into MP4 video.",
            link: desktopLink("toast-animated"),
          }
        : { text: "This image is animated. Only the first frame was saved.", link: null };
    case "tip":
      return {
        text: "convt for desktop converts video, audio and PDFs from the right-click menu on your computer.",
        link: { label: "Take a look", href: siteUrl("/", "toast-tip") },
      };
    default: {
      const _exhaustive: never = note;
      return _exhaustive;
    }
  }
}

function failed(
  title: string,
  detail: string,
  extra: Partial<Pick<ToastCopy, "action" | "lingerMs">> = {},
): ToastCopy {
  return {
    tone: "failed",
    title,
    detail: { text: detail, mono: false },
    action: extra.action ?? null,
    note: null,
    lingerMs: extra.lingerMs === undefined ? 12000 : extra.lingerMs,
  };
}

function problemCopy(problem: Problem, jobId: JobId): ToastCopy {
  switch (problem.kind) {
    case "blocked":
      return failed(
        "convt needs access to this image",
        `${problem.host} doesn't share its images with extensions until you allow it.`,
        { action: { kind: "open-access", label: "Allow access", jobId }, lingerMs: null },
      );
    case "network":
      return failed(
        "Couldn't download the image",
        "Check your connection, or open the image in its own tab and try again.",
      );
    case "http":
      return failed(
        `The image didn't load (error ${problem.status})`,
        "Reload the page and try again.",
      );
    case "not-image":
      return failed(
        "That isn't an image convt can read",
        "The address returned something else, like a web page.",
      );
    case "unreadable":
      return failed(
        `Chrome can't read ${sourceInfo[problem.from].label} images`,
        "convt for desktop converts them on your computer.",
        { action: { kind: "link", ...desktopLink("toast-unreadable") } },
      );
    case "too-large":
      return failed(
        "This image is too big to convert in the browser",
        "convt for desktop handles images of any size.",
        { action: { kind: "link", ...desktopLink("toast-unreadable") } },
      );
    case "save-failed":
      return failed("Couldn't save the file", saveFailure(problem.reason));
    case "clipboard":
      return failed(
        "Couldn't copy the image",
        "Click anywhere on the page, then choose Copy as PNG again.",
      );
    case "copy-unavailable":
      return failed(
        "Copy as PNG doesn't work on this page",
        "Chrome protects this page from extensions. Save as PNG instead.",
      );
    case "in-frame":
      return failed(
        "This image is inside an embedded frame",
        "Open the image in its own tab, then try again.",
      );
    case "internal":
      return failed("Something went wrong converting this image", "Try again in a moment.");
    default: {
      const _exhaustive: never = problem;
      return _exhaustive;
    }
  }
}

/** Chrome's download interrupt reasons, in words. */
function saveFailure(reason: string): string {
  switch (reason) {
    case "FILE_NO_SPACE":
      return "Your disk is full.";
    case "FILE_ACCESS_DENIED":
    case "FILE_SECURITY_CHECK_FAILED":
      return "Chrome isn't allowed to save to that folder.";
    case "FILE_NAME_TOO_LONG":
    case "FILE_TOO_LONG":
      return "The file name is too long for that folder.";
    case "FILE_BLOCKED":
    case "FILE_VIRUS_INFECTED":
      return "Your computer's security settings blocked the download.";
    default:
      return `Chrome stopped the download (${reason.toLowerCase().replaceAll("_", " ")}).`;
  }
}
