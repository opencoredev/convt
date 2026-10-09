// The offscreen document: a hidden page the background worker asks to convert images.

import { fromBase64, toBase64 } from "../shared/bytes.ts";
import { parseOffscreenRequest, type TranscodeResult } from "../shared/messages.ts";
import { transcode } from "./transcode.ts";

/** Base64 grows bytes by a third; this keeps Copy as PNG under Chrome's 64 MiB message cap. */
const MAX_INLINE_BYTES = 40_000_000;
/** A safety net in case the worker never says a download finished. */
const URL_LIFETIME_MS = 10 * 60 * 1000;

const live = new Map<string, number>();

function release(url: string) {
  const timer = live.get(url);
  if (timer === undefined) return;
  clearTimeout(timer);
  live.delete(url);
  URL.revokeObjectURL(url);
}

chrome.runtime.onMessage.addListener((message: unknown, _sender, sendResponse) => {
  const request = parseOffscreenRequest(message);
  if (request === null) return false;
  if (request.kind === "release") {
    release(request.url);
    return false;
  }
  void (async (): Promise<TranscodeResult> => {
    const result = await transcode({
      bytes: fromBase64(request.data),
      from: request.from,
      to: request.to,
      quality: request.quality,
    });
    let response: TranscodeResult;
    if (!result.ok) {
      response = result;
    } else if (request.deliver === "url") {
      const url = URL.createObjectURL(result.blob);
      live.set(
        url,
        window.setTimeout(() => release(url), URL_LIFETIME_MS),
      );
      response = {
        ok: true,
        output: { kind: "url", url },
        bytes: result.blob.size,
        thumb: result.thumb,
      };
    } else if (result.blob.size > MAX_INLINE_BYTES) {
      response = { ok: false, problem: "too-large" };
    } else {
      const data = toBase64(new Uint8Array(await result.blob.arrayBuffer()));
      response = {
        ok: true,
        output: { kind: "bytes", data },
        bytes: result.blob.size,
        thumb: result.thumb,
      };
    }
    return response;
  })()
    // Always answer: the worker waits on this response.
    .catch((): TranscodeResult => ({ ok: false, problem: "encode" }))
    .then(sendResponse);
  return true;
});
