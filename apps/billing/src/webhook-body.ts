const MAX_BYTES = 256 * 1024;

export class WebhookBodyError extends Error {
  constructor(
    public status: number,
    message: string,
  ) {
    super(message);
  }
}

/** Preserve signed raw bytes without buffering an unbounded public request. */
export async function readWebhookBody(request: Request): Promise<Uint8Array> {
  if (request.method !== "POST") throw new WebhookBodyError(405, "method not allowed");
  const length = Number(request.headers.get("content-length") ?? 0);
  if (length > MAX_BYTES) throw new WebhookBodyError(413, "too large");
  if (!request.body) return new Uint8Array();
  const reader = request.body.getReader();
  const chunks: Uint8Array[] = [];
  let total = 0;
  try {
    for (;;) {
      const { done, value } = await reader.read();
      if (done) break;
      total += value.byteLength;
      if (total > MAX_BYTES) {
        await reader.cancel();
        throw new WebhookBodyError(413, "too large");
      }
      chunks.push(value);
    }
  } finally {
    reader.releaseLock();
  }
  const raw = new Uint8Array(total);
  let offset = 0;
  for (const chunk of chunks) {
    raw.set(chunk, offset);
    offset += chunk.byteLength;
  }
  return raw;
}
