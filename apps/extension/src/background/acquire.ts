// Get the bytes of the image the user right-clicked.
//
// 1. data: URLs decode directly.
// 2. The background worker fetches it. That works for hosts that allow cross-origin
//    reads, and for every host the user has granted access to.
// 3. Otherwise the page fetches it (activeTab lets us run code there after the
//    right-click). It has the page's cookies and same-origin access, and blob: URLs
//    only resolve in the frame that made them. activeTab covers the top frame's
//    site only, so a frame from another site is out of reach.
// 4. If both are refused and we lack access to the host, the user can grant it.

import { fromBase64, parseDataUrl } from "../shared/bytes.ts";
import type { Job, Problem } from "../shared/jobs.ts";
import { hostOf, hostPattern } from "../shared/hosts.ts";
import { sniff, type Sniffed } from "../shared/sniff.ts";

/**
 * The source reaches the converter as base64 in one message, which Chrome caps at
 * 64 MiB. 45 MB stays under it; bigger images get the desktop app's offer instead.
 */
const MAX_BYTES = 45_000_000;

export type Acquired =
  | { ok: true; bytes: Uint8Array; sniffed: Sniffed }
  | { ok: false; problem: Problem };

type Fetched =
  | { ok: true; bytes: Uint8Array }
  /** No response at all: CORS refusal, offline, or a blocked request. */
  | { ok: false; kind: "refused" }
  /** We couldn't run code in the image's frame (another site's iframe). */
  | { ok: false; kind: "no-frame" }
  | { ok: false; kind: "too-large" }
  | { ok: false; kind: "http"; status: number };

export async function acquire(job: Job): Promise<Acquired> {
  if (job.srcUrl.startsWith("data:")) {
    const parsed = parseDataUrl(job.srcUrl);
    return parsed ? checked(parsed.bytes) : { ok: false, problem: { kind: "not-image" } };
  }

  const isBlob = job.srcUrl.startsWith("blob:");
  const direct: Fetched = isBlob
    ? { ok: false, kind: "refused" }
    : await fetchFromWorker(job.srcUrl);
  if (direct.ok) {
    const result = checked(direct.bytes);
    // A 200 that isn't an image is often a login page; the page's cookies may help.
    if (result.ok || result.problem.kind !== "not-image") return result;
  }

  const inPage = await fetchInPage(job);
  if (inPage.ok) return checked(inPage.bytes);
  if (inPage.kind === "too-large") return { ok: false, problem: { kind: "too-large" } };
  if (inPage.kind === "http")
    return { ok: false, problem: { kind: "http", status: inPage.status } };
  // The host answered us with an error; access wouldn't change that.
  if (!direct.ok && direct.kind === "http") {
    return { ok: false, problem: { kind: "http", status: direct.status } };
  }

  if (isBlob) {
    return { ok: false, problem: { kind: inPage.kind === "no-frame" ? "in-frame" : "network" } };
  }
  const host = hostOf(job.srcUrl);
  if (host !== null && !(await hasAccess(job.srcUrl))) {
    return { ok: false, problem: { kind: "blocked", host } };
  }
  return { ok: false, problem: { kind: "network" } };
}

function checked(bytes: Uint8Array): Acquired {
  if (bytes.length > MAX_BYTES) return { ok: false, problem: { kind: "too-large" } };
  const sniffed = sniff(bytes);
  if (sniffed.kind === "unknown") return { ok: false, problem: { kind: "not-image" } };
  return { ok: true, bytes, sniffed };
}

async function fetchFromWorker(url: string): Promise<Fetched> {
  try {
    const response = await fetch(url, { credentials: "omit", cache: "force-cache" });
    if (!response.ok) return { ok: false, kind: "http", status: response.status };
    return { ok: true, bytes: new Uint8Array(await response.arrayBuffer()) };
  } catch {
    return { ok: false, kind: "refused" };
  }
}

async function fetchInPage(job: Job): Promise<Fetched> {
  try {
    const [injection] = await chrome.scripting.executeScript({
      target: { tabId: job.tabId, frameIds: [job.frameId] },
      func: pageFetch,
      args: [job.srcUrl, MAX_BYTES],
    });
    return parsePageFetch(injection?.result);
  } catch {
    // No access to that frame: another site's iframe, or a page Chrome protects.
    return job.frameId === 0 ? { ok: false, kind: "refused" } : { ok: false, kind: "no-frame" };
  }
}

function parsePageFetch(value: unknown): Fetched {
  if (typeof value !== "object" || value === null || !("ok" in value)) {
    return { ok: false, kind: "refused" };
  }
  if (value.ok === true && "data" in value && typeof value.data === "string") {
    return { ok: true, bytes: fromBase64(value.data) };
  }
  if ("tooLarge" in value && value.tooLarge === true) return { ok: false, kind: "too-large" };
  if ("status" in value && typeof value.status === "number" && value.status > 0) {
    return { ok: false, kind: "http", status: value.status };
  }
  return { ok: false, kind: "refused" };
}

/**
 * Runs inside the page, so it must stand alone: Chrome serializes the function's
 * source, and nothing it closes over comes with it.
 */
async function pageFetch(
  url: string,
  maxBytes: number,
): Promise<{ ok: true; data: string } | { ok: false; status: number; tooLarge?: true }> {
  try {
    const response = await fetch(url);
    if (!response.ok) return { ok: false, status: response.status };
    // Stop before encoding something too big to send back; it would stall the page.
    if (Number(response.headers.get("content-length") ?? 0) > maxBytes) {
      return { ok: false, status: 0, tooLarge: true };
    }
    const bytes = new Uint8Array(await response.arrayBuffer());
    if (bytes.length > maxBytes) return { ok: false, status: 0, tooLarge: true };
    let binary = "";
    for (let i = 0; i < bytes.length; i += 0x8000) {
      binary += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
    }
    return { ok: true, data: btoa(binary) };
  } catch {
    return { ok: false, status: 0 };
  }
}

export async function hasAccess(url: string): Promise<boolean> {
  const pattern = hostPattern(url);
  if (pattern === null) return false;
  return chrome.permissions.contains({ origins: [pattern] });
}
