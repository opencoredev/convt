import { formats, type Format } from "./formats.js";
export { formats, type Format };
export { verifyWebhook, type WebhookEvent } from "./webhooks.js";

/** Conversion settings. Every field is optional; ones that don't apply to the target are ignored. */
export type JobOptions = {
  /** Lossy quality from 1 to 100 for JPEG, WebP, AVIF, HEIC and video. */
  quality?: number | null;
  /** Longest edge of image output in pixels. Scales down only. */
  max_size?: number | null;
  /** Maximum video height in pixels, an even number. Scales down only. */
  video_height?: number | null;
  /** Audio bitrate in kbit/s, from 8 to 640. */
  audio_bitrate?: number | null;
  /** PDF pages to render: "2", "1-3" or "4-". */
  pages?: string | null;
  /** Resolution for PDF and SVG pages, from 18 to 1200. */
  dpi?: number | null;
  video_codec?: "h264" | "hevc" | null;
  /** "transparent", "white", "black" or a color like "#ff8800". */
  background?: string | null;
  /** Leave the audio out of video output. */
  strip_audio?: boolean;
  /** Largest output file in bytes, at least 10000. Lowers quality to fit. */
  target_size?: number;
  /** Leave EXIF, GPS and other metadata out of the output. */
  strip_metadata?: boolean;
  /** Start video or audio output here: seconds or a timestamp like "1:30". */
  trim_start?: number | string | null;
  /** End video or audio output here: seconds or a timestamp like "1:30". */
  trim_end?: number | string | null;
  /** Frames per second for video and GIF output, from 1 to 120. */
  fps?: number | null;
};
export type JobStatus =
  | "created"
  | "uploaded"
  | "queued"
  | "running"
  | "succeeded"
  | "failed"
  | "cancelled";
export type Job = {
  id: string;
  status: JobStatus;
  input_format: Format;
  target_format: Format;
  /** Absent from servers that predate options. */
  options?: JobOptions;
  /** Null for a job whose input comes from `source_url`. */
  input_bytes: number | null;
  attempt: number;
  error_code: string | null;
  /** Absent from servers that predate it. */
  created_at?: string;
  expires_at: string;
  source_url?: string;
  webhook_url?: string;
};
export class ConvtError extends Error {
  constructor(
    readonly code: string,
    message: string,
    readonly status?: number,
    /** The `X-Request-Id` of the failed call. Quote it when you contact support. */
    readonly requestId?: string,
    /**
     * The `Idempotency-Key` a failed create was sent with. When the outcome is
     * unknown, retry with the same key to get the original job instead of a new one.
     */
    readonly idempotencyKey?: string,
    options?: ErrorOptions,
  ) {
    super(message, options);
    this.name = "ConvtError";
  }
}
export class ConvtCancellationError extends ConvtError {
  constructor(readonly jobId: string) {
    super(
      "cancellation_unconfirmed",
      "Cancellation could not be confirmed. Retry cancellation to release the reservation.",
    );
  }
}
async function abortable<T>(operation: () => Promise<T>, signal: AbortSignal): Promise<T> {
  signal.throwIfAborted();
  return new Promise<T>((resolve, reject) => {
    const abort = () => reject(signal.reason);
    signal.addEventListener("abort", abort, { once: true });
    Promise.resolve()
      .then(operation)
      .then(resolve, reject)
      .finally(() => signal.removeEventListener("abort", abort));
  });
}
function sleep(ms: number, signal: AbortSignal): Promise<void> {
  return new Promise<void>((resolve, reject) => {
    const abort = () => {
      clearTimeout(timer);
      reject(signal.reason);
    };
    const timer = setTimeout(() => {
      signal.removeEventListener("abort", abort);
      resolve();
    }, ms);
    if (signal.aborted) abort();
    else signal.addEventListener("abort", abort, { once: true });
  });
}
function newIdempotencyKey(): string {
  return globalThis.crypto.randomUUID();
}
/** Reads the API's `{ error: { code, message } }` shape from an unknown JSON body. */
function errorFields(value: unknown): { code?: string; message?: string } {
  if (typeof value !== "object" || value === null || !("error" in value)) return {};
  const error = value.error;
  if (typeof error !== "object" || error === null) return {};
  return {
    code: "code" in error && typeof error.code === "string" ? error.code : undefined,
    message: "message" in error && typeof error.message === "string" ? error.message : undefined,
  };
}
const RETRYABLE_STATUS = new Set([429, 502, 503, 504]);

export type ConvertOptions = {
  to: Format;
  from?: Format;
  /** Conversion settings sent with the job. */
  options?: JobOptions;
  /** An https URL that gets a signed POST when the job finishes. See verifyWebhook. */
  webhookUrl?: string;
  /**
   * Sent as `Idempotency-Key`. Defaults to a fresh UUID. Pass a stable value, such
   * as your own record id, so calling again after a crash returns the same job.
   */
  idempotencyKey?: string;
  signal?: AbortSignal;
  onProgress?: (job: Job) => void;
  timeoutMs?: number;
};
export type Output = { name: string; url: string };
export type CreateInput = {
  input_format: Format;
  target_format: Format;
  options?: JobOptions;
  webhook_url?: string;
} & ({ input_bytes: number; source_url?: never } | { source_url: string; input_bytes?: never });
export type Created = {
  job: Job;
  /** Null when the job needs no upload (`source_url`) or was already started. */
  upload_url: string | null;
  upload_expires_in: number | null;
};
export type RequestOptions = {
  signal?: AbortSignal;
  /** Sent as `Idempotency-Key`, so a retried create never reserves twice. */
  idempotencyKey?: string;
};
export class Conversion {
  constructor(
    readonly job: Job,
    readonly outputs: Output[],
  ) {}
  private async fetchOutput(index: number, timeoutMs: number): Promise<Response> {
    const output = this.outputs[index];
    if (!output) throw new ConvtError("missing_output", "Output does not exist.");
    const response = await fetch(output.url, { signal: AbortSignal.timeout(timeoutMs) });
    if (!response.ok || !response.body)
      throw new ConvtError(
        "download_failed",
        "The download expired or is unavailable. Call convt.download(job.id) for fresh URLs.",
        response.status,
      );
    return response;
  }
  /** Reads one output into memory. For large files, prefer save() or saveAll(). */
  async blob(index = 0): Promise<Blob> {
    return (await this.fetchOutput(index, 60_000)).blob();
  }
  /** Streams one output to disk without holding it in memory. Node, Bun and Deno. */
  private async stream(index: number, path: string): Promise<void> {
    const { createWriteStream } = await import("node:fs");
    const { Readable } = await import("node:stream");
    const { pipeline } = await import("node:stream/promises");
    const response = await this.fetchOutput(index, 30 * 60_000);
    // `response.body` is checked in fetchOutput.
    await pipeline(Readable.fromWeb(response.body as never), createWriteStream(path));
  }
  async save(path: string): Promise<void> {
    if (typeof window !== "undefined")
      throw new ConvtError("node_only", "Use blob() or outputs in the browser.");
    if (this.outputs.length !== 1)
      throw new ConvtError(
        "multiple_outputs",
        "Use saveAll(directory) for jobs with several outputs.",
      );
    await this.stream(0, path);
  }
  async saveAll(directory: string): Promise<void> {
    const { mkdir } = await import("node:fs/promises");
    const { join, basename } = await import("node:path");
    await mkdir(directory, { recursive: true });
    for (let i = 0; i < this.outputs.length; i++)
      await this.stream(i, join(directory, basename(this.outputs[i].name)));
  }
}
export type ConvtClientOptions =
  | { apiKey: string; baseUrl?: string; maxRetries?: number }
  | { token: () => Promise<string>; baseUrl: string; maxRetries?: number };

export class Convt {
  private readonly credential: string | (() => Promise<string>);
  private readonly baseUrl: string;
  private readonly maxRetries: number;
  constructor(
    options: ConvtClientOptions = {
      apiKey:
        typeof process !== "undefined"
          ? (process.env.CONVT_API_KEY ?? process.env.CONVT_KEY ?? "")
          : "",
    },
  ) {
    this.credential = "apiKey" in options ? options.apiKey : options.token;
    this.baseUrl = (options.baseUrl ?? "https://api.convt.app").replace(/\/$/, "");
    this.maxRetries = Math.max(0, options.maxRetries ?? 2);
    if (!this.credential)
      throw new ConvtError("api_key_required", "Pass apiKey or set CONVT_API_KEY.");
  }
  /**
   * One API call. Rate limits (429) and gateway errors (502, 503, 504) are retried
   * with backoff, honoring `Retry-After`, but only for calls that are safe to repeat:
   * reads, start and cancel (which the API makes idempotent), and creates that carry
   * an `Idempotency-Key`.
   */
  private async request<T>({
    path,
    method = "GET",
    data,
    signal = AbortSignal.timeout(30_000),
    idempotencyKey,
  }: {
    path: string;
    method?: "GET" | "POST";
    data?: unknown;
    signal?: AbortSignal;
    idempotencyKey?: string;
  }): Promise<T> {
    if (idempotencyKey !== undefined && !/^[\x21-\x7e]{1,255}$/.test(idempotencyKey))
      throw new ConvtError(
        "invalid_idempotency_key",
        "idempotencyKey must be 1 to 255 visible ASCII characters, such as a UUID.",
      );
    const retryable = method === "GET" || idempotencyKey !== undefined || data === undefined;
    const fail = (
      code: string,
      message: string,
      status?: number,
      requestId?: string,
      cause?: unknown,
    ) => new ConvtError(code, message, status, requestId, idempotencyKey, { cause });
    // Once a keyed create is on the wire, an abort leaves its outcome unknown;
    // say so and keep the key, so a retry can find the job instead of making one.
    const unknown = (cause: unknown) =>
      method === "POST" && idempotencyKey !== undefined
        ? fail(
            "create_outcome_unknown",
            "The request may or may not have reached convt. Retry with the same idempotencyKey to find out without creating a second job.",
            undefined,
            undefined,
            cause,
          )
        : cause;
    for (let attempt = 0; ; attempt++) {
      const credential = this.credential;
      const key =
        typeof credential === "function" ? await abortable(credential, signal) : credential;
      const again = async (delay: number) => {
        await sleep(delay, signal);
      };
      // Nothing has been sent yet, so an abort here is just an abort.
      signal.throwIfAborted();
      let response: Response;
      try {
        response = await fetch(`${this.baseUrl}${path}`, {
          method,
          signal,
          headers: {
            authorization: `Bearer ${key}`,
            accept: "application/json",
            ...(data ? { "content-type": "application/json" } : {}),
            ...(idempotencyKey ? { "idempotency-key": idempotencyKey } : {}),
          },
          ...(data ? { body: JSON.stringify(data) } : {}),
        });
      } catch (error) {
        if (signal.aborted) throw unknown(error);
        if (retryable && attempt < this.maxRetries) {
          await again(250 * 2 ** attempt);
          continue;
        }
        throw fail(
          "network_error",
          `The request did not complete: ${String(error)}`,
          undefined,
          undefined,
          error,
        );
      }
      const requestId = response.headers.get("x-request-id") ?? undefined;
      if (
        !response.ok &&
        retryable &&
        attempt < this.maxRetries &&
        RETRYABLE_STATUS.has(response.status)
      ) {
        const after = Number(response.headers.get("retry-after"));
        await response.body?.cancel();
        await again(
          Number.isFinite(after) && after > 0 ? Math.min(after, 60) * 1000 : 500 * 2 ** attempt,
        );
        continue;
      }
      let value: unknown;
      try {
        value = await response.json();
      } catch (error) {
        if (signal.aborted) throw unknown(signal.reason ?? error);
        // The body broke off or was not JSON. For a create that already reserved a
        // job, retrying under the same key returns that job rather than a new one.
        if (retryable && attempt < this.maxRetries) {
          await again(250 * 2 ** attempt);
          continue;
        }
        throw fail(
          response.ok ? "invalid_response" : "request_failed",
          `The API answered HTTP ${response.status} without a readable JSON body.`,
          response.status,
          requestId,
        );
      }
      if (!response.ok) {
        const { code, message } = errorFields(value);
        throw fail(
          code ?? "request_failed",
          message ?? "Request failed.",
          response.status,
          requestId,
        );
      }
      // The API's JSON matches the documented shape for each route.
      return value as T;
    }
  }
  create(input: CreateInput, { signal, idempotencyKey }: RequestOptions = {}) {
    return this.request<Created>({
      path: "/v1/jobs",
      method: "POST",
      data: input,
      signal,
      idempotencyKey,
    });
  }
  start(id: string, signal?: AbortSignal) {
    return this.request<Job>({
      path: `/v1/jobs/${encodeURIComponent(id)}/start`,
      method: "POST",
      signal,
    });
  }
  status(id: string, signal?: AbortSignal) {
    return this.request<Job>({ path: `/v1/jobs/${encodeURIComponent(id)}`, signal });
  }
  cancel(id: string) {
    return this.request<Job>({ path: `/v1/jobs/${encodeURIComponent(id)}/cancel`, method: "POST" });
  }
  download(id: string, signal?: AbortSignal) {
    return this.request<{ outputs: Output[]; expires_in: number }>({
      path: `/v1/jobs/${encodeURIComponent(id)}/download`,
      signal,
    });
  }
  /** The account's webhook signing secret, for verifyWebhook. */
  webhookSecret(signal?: AbortSignal) {
    return this.request<{ secret: string }>({ path: "/v1/webhooks/secret", signal });
  }
  /**
   * Converts a file: a path (Node, Bun, Deno), a Blob or File, or bytes. Uploads
   * go straight to storage, so files up to 2 GB work. Retries are safe: the job is
   * created with a fresh `Idempotency-Key`.
   */
  async convert(input: string | Blob | Uint8Array, options: ConvertOptions): Promise<Conversion> {
    let body: Blob;
    let name = "";
    if (typeof input === "string") {
      if (typeof window !== "undefined")
        throw new ConvtError("file_required", "Pass a File or Blob in the browser.");
      // A file-backed Blob streams from disk on upload instead of loading the file.
      const fs = await import("node:fs");
      body =
        typeof fs.openAsBlob === "function"
          ? await fs.openAsBlob(input)
          : new Blob([await (await import("node:fs/promises")).readFile(input)]);
      name = input;
    } else if (input instanceof Blob) {
      body = input;
      name = "name" in input ? String(input.name) : "";
    } else {
      body = new Blob([new Uint8Array(input)]);
    }
    const from = options.from ?? formatFromName(name);
    if (!from)
      throw new ConvtError(
        "input_format_required",
        "Provide from for an input without a recognized extension.",
      );
    return this.run(
      {
        input_format: from,
        target_format: options.to,
        input_bytes: body.size,
        ...(options.options ? { options: options.options } : {}),
        ...(options.webhookUrl ? { webhook_url: options.webhookUrl } : {}),
      },
      body,
      options,
    );
  }
  /**
   * Converts a file convt downloads from a public https URL, such as a presigned S3
   * or R2 link. Nothing is uploaded from this machine.
   */
  async convertUrl(sourceUrl: string, options: ConvertOptions): Promise<Conversion> {
    const from = options.from ?? formatFromName(new URL(sourceUrl).pathname);
    if (!from)
      throw new ConvtError(
        "input_format_required",
        "Provide from for a URL whose path has no recognized extension.",
      );
    return this.run(
      {
        input_format: from,
        target_format: options.to,
        source_url: sourceUrl,
        ...(options.options ? { options: options.options } : {}),
        ...(options.webhookUrl ? { webhook_url: options.webhookUrl } : {}),
      },
      undefined,
      options,
    );
  }
  private async run(
    input: CreateInput,
    body: Blob | undefined,
    options: ConvertOptions,
  ): Promise<Conversion> {
    const deadline = AbortSignal.timeout(options.timeoutMs ?? 660_000);
    const signal = options.signal ? AbortSignal.any([options.signal, deadline]) : deadline;
    const created = await this.create(input, {
      signal,
      idempotencyKey: options.idempotencyKey ?? newIdempotencyKey(),
    });
    try {
      if (body && created.upload_url) {
        const uploaded = await fetch(created.upload_url, { method: "PUT", body, signal });
        if (!uploaded.ok)
          throw new ConvtError("upload_failed", "Upload failed. Try again.", uploaded.status);
      }
      let job = body ? await this.start(created.job.id, signal) : created.job;
      let pause = 500;
      while (true) {
        options.onProgress?.(job);
        if (job.status === "succeeded")
          return new Conversion(job, (await this.download(job.id, signal)).outputs);
        if (job.status === "failed" || job.status === "cancelled")
          throw new ConvtError(
            job.error_code ?? job.status,
            job.status === "cancelled"
              ? "Conversion cancelled."
              : job.error_code === "source_fetch_failed"
                ? "convt could not download source_url. Check that it is public and returns the file."
                : "Conversion failed. Try another format or use the desktop app.",
          );
        await sleep(pause, signal);
        pause = Math.min(pause * 1.5, 2000);
        job = await this.status(job.id, signal);
      }
    } catch (error) {
      let terminal: Job;
      try {
        terminal = await this.cancel(created.job.id);
      } catch {
        throw new ConvtCancellationError(created.job.id);
      }
      if (signal.aborted) {
        if (terminal.status === "cancelled" || terminal.status === "failed")
          throw new ConvtError("cancelled", "Conversion cancelled. Your allowance was released.");
        if (terminal.status === "succeeded")
          throw new ConvtError(
            "already_completed",
            "Conversion completed before cancellation. Your allowance was settled.",
          );
        throw new ConvtCancellationError(created.job.id);
      }
      throw error;
    }
  }
}

function formatFromName(name: string): Format | undefined {
  const file = name.split(/[\\/]/).pop() ?? "";
  if (!file.includes(".")) return undefined;
  const extension = file.split(".").pop()?.toLowerCase() ?? "";
  return formats.find((f) => (f.extensions as readonly string[]).includes(extension))?.id;
}
