// Everything the extension remembers. Settings sync with the Chrome profile; recent
// files stay on this machine; in-flight jobs live in session storage so they survive
// the background worker being suspended, and vanish when the browser closes.

import { isSourceKind, type SourceKind } from "./formats.ts";
import {
  MAX_RECENT,
  parseRecent,
  type Job,
  type JobId,
  type RecentItem,
  type ToastView,
} from "./jobs.ts";
import { parseJob, parseToastView } from "./messages.ts";
import { parseSettings, type Settings } from "./settings.ts";

export async function getSettings(): Promise<Settings> {
  const { settings } = await chrome.storage.sync.get("settings");
  return parseSettings(settings);
}

export async function saveSettings(settings: Settings): Promise<void> {
  await chrome.storage.sync.set({ settings });
}

export async function getRecent(): Promise<RecentItem[]> {
  const { recent } = await chrome.storage.local.get("recent");
  return parseRecent(recent);
}

/**
 * Runs read-modify-write updates one at a time. Two downloads can finish together,
 * and without this the second write would drop the first one's entry.
 */
let updates: Promise<unknown> = Promise.resolve();
function serialized<T>(update: () => Promise<T>): Promise<T> {
  const next = updates.then(update, update);
  updates = next.catch(() => undefined);
  return next;
}

export function addRecent(item: RecentItem): Promise<void> {
  return serialized(async () => {
    const recent = await getRecent();
    const next = [item, ...recent.filter((r) => r.jobId !== item.jobId)].slice(0, MAX_RECENT);
    await chrome.storage.local.set({ recent: next });
  });
}

/** Call from the worker (the popup sends `clear-recent`), so it queues behind saves. */
export function clearRecent(): Promise<void> {
  return serialized(() => chrome.storage.local.set({ recent: [] }));
}

/** Counts finished conversions; returns the new total. */
export function countConversion(): Promise<number> {
  return serialized(async () => {
    const { conversions } = await chrome.storage.local.get("conversions");
    const next = (typeof conversions === "number" ? conversions : 0) + 1;
    await chrome.storage.local.set({ conversions: next });
    return next;
  });
}

/** A job waiting for the user to grant site access. */
export async function setPending(job: Job): Promise<void> {
  await chrome.storage.session.set({ [`pending:${job.id}`]: job });
}

export async function takePending(jobId: JobId): Promise<Job | null> {
  const key = `pending:${jobId}`;
  const stored = (await chrome.storage.session.get(key))[key];
  await chrome.storage.session.remove(key);
  return parseJob(stored);
}

export async function getPending(jobId: JobId): Promise<Job | null> {
  const key = `pending:${jobId}`;
  return parseJob((await chrome.storage.session.get(key))[key]);
}

/**
 * A download Chrome is still writing, with what the toast needs once it finishes.
 * `preview` is the small WebP the popup lists.
 */
export type InFlight = {
  job: Job;
  from: SourceKind;
  animated: boolean;
  preview: string;
  /** The offscreen document's copy of the file, freed once Chrome has saved it. */
  blobUrl: string;
};

export async function setInFlight(downloadId: number, entry: InFlight): Promise<void> {
  await chrome.storage.session.set({ [`download:${downloadId}`]: entry });
}

export async function takeInFlight(downloadId: number): Promise<InFlight | null> {
  const key = `download:${downloadId}`;
  const stored: unknown = (await chrome.storage.session.get(key))[key];
  if (typeof stored !== "object" || stored === null) return null;
  await chrome.storage.session.remove(key);
  const job = "job" in stored ? parseJob(stored.job) : null;
  const from = "from" in stored && isSourceKind(stored.from) ? stored.from : null;
  const animated =
    "animated" in stored && typeof stored.animated === "boolean" ? stored.animated : null;
  const preview = "preview" in stored && typeof stored.preview === "string" ? stored.preview : null;
  const blobUrl = "blobUrl" in stored && typeof stored.blobUrl === "string" ? stored.blobUrl : null;
  if (job === null || from === null || animated === null || preview === null || blobUrl === null) {
    return null;
  }
  return { job, from, animated, preview, blobUrl };
}

/**
 * The last failure on a page that couldn't show the toast. The popup shows it so the
 * user still learns what went wrong.
 */
export type LastFailure = { view: Extract<ToastView, { phase: "failed" }>; at: number };

export async function setLastFailure(failure: LastFailure | null): Promise<void> {
  await chrome.storage.session.set({ lastFailure: failure });
}

export async function getLastFailure(): Promise<LastFailure | null> {
  const { lastFailure } = await chrome.storage.session.get("lastFailure");
  if (typeof lastFailure !== "object" || lastFailure === null) return null;
  const view = "view" in lastFailure ? parseToastView(lastFailure.view) : null;
  const at = "at" in lastFailure && typeof lastFailure.at === "number" ? lastFailure.at : null;
  if (view === null || view.phase !== "failed" || at === null) return null;
  return { view, at };
}
