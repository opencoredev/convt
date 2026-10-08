// The background worker: the right-click menu, the conversion pipeline, and the
// follow-ups (download finished, site access granted).

import { toBase64 } from "../shared/bytes.ts";
import type { Target } from "../shared/formats.ts";
import {
  actionFromMenuId,
  menuId,
  newJobId,
  noteFor,
  type Action,
  type Job,
  type JobId,
  type Problem,
  type ToastView,
} from "../shared/jobs.ts";
import { parseCopyResult, parseToBackground, type ToContent } from "../shared/messages.ts";
import { baseName, outputName } from "../shared/naming.ts";
import { qualityInfo } from "../shared/settings.ts";
import {
  addRecent,
  countConversion,
  getSettings,
  setInFlight,
  setLastFailure,
  setPending,
  takeInFlight,
  takePending,
} from "../shared/store.ts";
import { acquire } from "./acquire.ts";
import { releaseOffscreen, transcodeOffscreen } from "./offscreen.ts";
import { flashBadge, injectToast, showToast } from "./toast.ts";

const MENU_ROOT = "convt";
const MENU_ACTIONS: { action: Action; title: string }[] = [
  { action: { kind: "save", target: "png" }, title: "Save as PNG" },
  { action: { kind: "save", target: "jpg" }, title: "Save as JPG" },
  { action: { kind: "save", target: "webp" }, title: "Save as WebP" },
  { action: { kind: "copy" }, title: "Copy as PNG" },
];

chrome.runtime.onInstalled.addListener((details) => {
  void createMenus();
  if (details.reason === chrome.runtime.OnInstalledReason.INSTALL) {
    void chrome.tabs.create({ url: "welcome.html" });
  }
});

async function createMenus() {
  await chrome.contextMenus.removeAll();
  chrome.contextMenus.create({ id: MENU_ROOT, title: "Convert with convt", contexts: ["image"] });
  for (const [index, { action, title }] of MENU_ACTIONS.entries()) {
    if (action.kind === "copy" && index > 0) {
      chrome.contextMenus.create({
        id: "separator",
        parentId: MENU_ROOT,
        type: "separator",
        contexts: ["image"],
      });
    }
    chrome.contextMenus.create({
      id: menuId(action),
      parentId: MENU_ROOT,
      title,
      contexts: ["image"],
    });
  }
}

chrome.contextMenus.onClicked.addListener((info, tab) => {
  const action = actionFromMenuId(info.menuItemId);
  if (action === null || tab?.id === undefined || !info.srcUrl) return;
  void runJob({
    id: newJobId(),
    srcUrl: info.srcUrl,
    pageUrl: info.pageUrl ?? tab.url ?? "",
    tabId: tab.id,
    frameId: info.frameId ?? 0,
    action,
  });
});

/** Pages and toasts get the image URL as a thumbnail, unless it's a huge data: URL. */
const MAX_THUMB_URL = 512 * 1024;

function thumbFor(srcUrl: string, preview: string | null): string {
  if (srcUrl.length <= MAX_THUMB_URL && srcUrl !== COMPACT_DATA_URL) return srcUrl;
  return preview ?? "";
}

/** Session storage holds 10 MB; a big data: URL doesn't need to be kept, only named. */
const COMPACT_DATA_URL = "data:";

function storable(job: Job): Job {
  return job.srcUrl.startsWith("data:") && job.srcUrl.length > 2048
    ? { ...job, srcUrl: COMPACT_DATA_URL }
    : job;
}

/** convt's own pages (the welcome page's demo photo) can't host the toast script. */
function isOwnPage(url: string): boolean {
  return url.startsWith(chrome.runtime.getURL(""));
}

async function runJob(job: Job): Promise<void> {
  const canToast = await injectToast(job.tabId);
  let thumb = thumbFor(job.srcUrl, null);
  if (canToast) {
    await showToast(job.tabId, { phase: "working", jobId: job.id, thumb, action: job.action });
  }

  const fail = async (problem: Problem) => {
    const view: ToastView = { phase: "failed", jobId: job.id, thumb, action: job.action, problem };
    if (problem.kind === "blocked") await setPending(job);
    if (canToast && (await showToast(job.tabId, view))) return;
    // No toast on this page: leave the problem for the popup and open it.
    await setLastFailure({ view, at: Date.now() });
    await flashBadge(job.tabId, "failed");
    // openPopup works without a policy from Chrome 127.
    if (typeof chrome.action.openPopup === "function") {
      await chrome.action.openPopup().catch(() => {});
    }
  };

  try {
    await convert(job, canToast, fail, (preview) => (thumb = thumbFor(job.srcUrl, preview)));
  } catch {
    // Whatever broke, the toast must not stay on "Saving…".
    await fail({ kind: "internal" }).catch(() => {});
  }
}

async function convert(
  job: Job,
  canToast: boolean,
  fail: (problem: Problem) => Promise<void>,
  setPreview: (preview: string) => void,
): Promise<void> {
  // Chrome refuses to write the clipboard from pages it protects. Say so before
  // fetching, so nobody is asked for site access for a copy that can't happen.
  if (job.action.kind === "copy" && !canToast && !isOwnPage(job.pageUrl)) {
    return fail({ kind: "copy-unavailable" });
  }

  const settings = await getSettings();
  const acquired = await acquire(job);
  if (!acquired.ok) return fail(acquired.problem);
  const { bytes, sniffed } = acquired;

  const target: Target = job.action.kind === "save" ? job.action.target : "png";
  const converted = await transcodeOffscreen({
    data: toBase64(bytes),
    from: sniffed.kind,
    to: target,
    quality: qualityInfo[settings.quality].encoderQuality,
    deliver: job.action.kind === "save" ? "url" : "bytes",
  });
  if (!converted.ok) {
    switch (converted.problem) {
      case "unreadable":
        return fail({ kind: "unreadable", from: sniffed.kind });
      case "too-large":
        return fail({ kind: "too-large" });
      case "encode":
        return fail({ kind: "internal" });
      default: {
        const _exhaustive: never = converted.problem;
        return _exhaustive;
      }
    }
  }
  setPreview(converted.thumb);
  const thumb = thumbFor(job.srcUrl, converted.thumb);
  const { output } = converted;

  if (output.kind === "bytes") {
    const copied = isOwnPage(job.pageUrl)
      ? await copyInExtensionPage(job.tabId, output.data)
      : await copyInPage(job.tabId, output.data);
    if (!copied) return fail({ kind: "clipboard" });
    const count = await countConversion();
    await addRecent({
      kind: "copied",
      jobId: job.id,
      at: Date.now(),
      bytes: converted.bytes,
      thumb: converted.thumb,
    });
    const view: ToastView = {
      phase: "copied",
      jobId: job.id,
      thumb,
      bytes: converted.bytes,
      from: sniffed.kind,
      note: noteFor({ animated: sniffed.animated, from: sniffed.kind, count }),
    };
    if (!(canToast && (await showToast(job.tabId, view)))) await flashBadge(job.tabId, "done");
    return;
  }

  let downloadId: number;
  try {
    downloadId = await chrome.downloads.download({
      url: output.url,
      filename: outputName(job.srcUrl, target),
      saveAs: settings.askWhereToSave,
      conflictAction: "uniquify",
    });
  } catch (error) {
    await releaseOffscreen(output.url);
    return fail({
      kind: "save-failed",
      reason: error instanceof Error ? error.message : "UNKNOWN",
    });
  }
  try {
    await setInFlight(downloadId, {
      job: storable(job),
      from: sniffed.kind,
      animated: sniffed.animated,
      preview: converted.thumb,
      blobUrl: output.url,
    });
  } catch {
    // The file still saves; we just can't report it. Clear the progress toast.
    await showToast(job.tabId, { phase: "dismissed", jobId: job.id });
    return;
  }
  // Small files can finish before the listener below sees a change.
  const [item] = await chrome.downloads.search({ id: downloadId });
  if (item && item.state !== "in_progress") await finishDownload(downloadId);
}

async function copyInExtensionPage(tabId: number, png: string): Promise<boolean> {
  const message: ToContent = { kind: "copy-image", png, tabId };
  try {
    return parseCopyResult(await chrome.runtime.sendMessage(message)).ok;
  } catch {
    return false;
  }
}

async function copyInPage(tabId: number, png: string): Promise<boolean> {
  const message: ToContent = { kind: "copy-image", png, tabId: null };
  try {
    return parseCopyResult(await chrome.tabs.sendMessage(tabId, message, { frameId: 0 })).ok;
  } catch {
    return false;
  }
}

chrome.downloads.onChanged.addListener((delta) => {
  if (delta.state?.current === "complete" || delta.state?.current === "interrupted") {
    void finishDownload(delta.id);
  }
});

const finishing = new Set<number>();

/** Reports a download we started once Chrome is done with it. Safe to call twice. */
async function finishDownload(downloadId: number): Promise<void> {
  if (finishing.has(downloadId)) return;
  finishing.add(downloadId);
  try {
    const entry = await takeInFlight(downloadId);
    if (entry === null) return;
    const { job, from, animated, preview, blobUrl } = entry;
    await releaseOffscreen(blobUrl);
    const thumb = thumbFor(job.srcUrl, preview);
    const [item] = await chrome.downloads.search({ id: downloadId });
    if (!item) return;
    if (item.state === "interrupted") {
      // Cancelling the Save As dialog isn't a failure worth a message.
      if (item.error === "USER_CANCELED") {
        await showToast(job.tabId, { phase: "dismissed", jobId: job.id });
        return;
      }
      const view: ToastView = {
        phase: "failed",
        jobId: job.id,
        thumb,
        action: job.action,
        problem: { kind: "save-failed", reason: item.error ?? "UNKNOWN" },
      };
      if (!(await showToast(job.tabId, view))) await flashBadge(job.tabId, "failed");
      return;
    }
    const target = targetOf(job);
    const name = baseName(item.filename);
    const bytes = item.fileSize > 0 ? item.fileSize : item.totalBytes;
    const count = await countConversion();
    await addRecent({
      kind: "saved",
      jobId: job.id,
      at: Date.now(),
      name,
      bytes,
      target,
      thumb: preview,
      downloadId,
    });
    const view: ToastView = {
      phase: "saved",
      jobId: job.id,
      thumb,
      name,
      bytes,
      from,
      target,
      downloadId,
      note: noteFor({ animated, from, count }),
    };
    if (!(await showToast(job.tabId, view))) await flashBadge(job.tabId, "done");
  } finally {
    finishing.delete(downloadId);
  }
}

function targetOf(job: Job): Target {
  return job.action.kind === "save" ? job.action.target : "png";
}

chrome.runtime.onMessage.addListener((raw: unknown, sender) => {
  // The offscreen document's requests are for it, not us.
  const message = parseToBackground(raw);
  if (message === null) return false;
  switch (message.kind) {
    case "show-download":
      chrome.downloads.show(message.downloadId);
      return false;
    case "open-access":
      void openAccessPage(message.jobId, sender.tab);
      return false;
    case "access-granted":
      void resumeAfterAccess(message.jobId, sender.tab?.id);
      return false;
    case "test:run":
      if (__E2E__) void runJob(message.job);
      return false;
    default: {
      const _exhaustive: never = message;
      return _exhaustive;
    }
  }
});

async function openAccessPage(jobId: JobId, from: chrome.tabs.Tab | undefined) {
  await chrome.tabs.create({
    url: `welcome.html?access=${encodeURIComponent(jobId)}`,
    ...(from?.index !== undefined ? { index: from.index + 1 } : {}),
    ...(from?.id !== undefined ? { openerTabId: from.id } : {}),
  });
}

/** The user granted access on the welcome page: go back to their tab and finish. */
async function resumeAfterAccess(jobId: JobId, accessTabId: number | undefined) {
  const job = await takePending(jobId);
  if (job === null) return;
  await chrome.tabs.update(job.tabId, { active: true }).catch(() => {});
  if (accessTabId !== undefined) await chrome.tabs.remove(accessTabId).catch(() => {});
  // Same job id: the "needs access" toast turns into progress, then the saved file.
  await runJob(job);
}
