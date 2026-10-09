// The background worker has no DOM, so conversions run in an offscreen document.

import {
  parseTranscodeResult,
  type ReleaseRequest,
  type TranscodeRequest,
  type TranscodeResult,
} from "../shared/messages.ts";

const URL_PATH = "offscreen.html";
let creating: Promise<void> | null = null;

/** One check-and-create at a time, so two quick clicks can't both try to create it. */
function ensureDocument(): Promise<void> {
  creating ??= (async () => {
    const existing = await chrome.runtime.getContexts({
      contextTypes: [chrome.runtime.ContextType.OFFSCREEN_DOCUMENT],
      documentUrls: [chrome.runtime.getURL(URL_PATH)],
    });
    if (existing.length > 0) return;
    await chrome.offscreen.createDocument({
      url: URL_PATH,
      reasons: [chrome.offscreen.Reason.BLOBS],
      justification: "Decode the image the user chose and encode it in the format they picked.",
    });
  })().finally(() => {
    creating = null;
  });
  return creating;
}

/** Longer than any real conversion; a decode that never settles fails instead of hanging. */
const TRANSCODE_TIMEOUT_MS = 60_000;

export async function transcodeOffscreen(
  request: Omit<TranscodeRequest, "target" | "kind">,
): Promise<TranscodeResult> {
  await ensureDocument();
  const message: TranscodeRequest = { target: "offscreen", kind: "transcode", ...request };
  const failed: TranscodeResult = { ok: false, problem: "encode" };
  let timedOut = false;
  let timer: ReturnType<typeof setTimeout> | undefined;
  const timeout = new Promise<TranscodeResult>((resolve) => {
    timer = setTimeout(() => {
      timedOut = true;
      resolve(failed);
    }, TRANSCODE_TIMEOUT_MS);
  });
  const answer = chrome.runtime.sendMessage(message).then((response: unknown) => {
    const result = parseTranscodeResult(response) ?? failed;
    // Nobody will download a result that arrives after the deadline; free it now.
    if (timedOut && result.ok && result.output.kind === "url") {
      void releaseOffscreen(result.output.url);
    }
    return result;
  });
  try {
    return await Promise.race([answer, timeout]);
  } finally {
    clearTimeout(timer);
  }
}

/** Lets the offscreen document free a converted file once Chrome has saved it. */
export async function releaseOffscreen(url: string): Promise<void> {
  const message: ReleaseRequest = { target: "offscreen", kind: "release", url };
  await chrome.runtime.sendMessage(message).catch(() => {
    // The document is gone, and the URL with it.
  });
}
