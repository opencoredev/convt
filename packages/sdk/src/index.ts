import { formats, type Format } from "./formats.js";
import { Sha256 } from "./sha256.js";
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
/** What a ConvtError knows about the request and job it came from. */
export type ConvtErrorDetails = {
  /** The HTTP status, when the API answered. */
  status?: number;
  /** The `X-Request-Id` of the failed call. Quote it when you contact support. */
  requestId?: string;
  /**
   * The `Idempotency-Key` the job was created with. When the outcome is unknown,
   * retry with the same key to get the original job instead of a new one.
   */
  idempotencyKey?: string;
  /** The job this error concerns, once one exists. `convt.status(jobId)` shows it. */
  jobId?: string;
  cause?: unknown;
};
export class ConvtError extends Error {
  readonly status?: number;
  readonly requestId?: string;
  readonly idempotencyKey?: string;
  readonly jobId?: string;
  constructor(
    readonly code: string,
    message: string,
    details: ConvtErrorDetails = {},
  ) {
    super(message, details.cause === undefined ? undefined : { cause: details.cause });
    this.name = "ConvtError";
    this.status = details.status;
    this.requestId = details.requestId;
    this.idempotencyKey = details.idempotencyKey;
    this.jobId = details.jobId;
  }
}
export class ConvtCancellationError extends ConvtError {
  declare readonly jobId: string;
  constructor(jobId: string, details: Omit<ConvtErrorDetails, "jobId"> = {}) {
    super(
      "cancellation_unconfirmed",
      "Cancellation could not be confirmed. Retry cancellation to release the reservation.",
      { ...details, jobId },
    );
  }
}
/** Adds job context to an error that lacks it, keeping its code and cause. */
function withContext(error: unknown, context: { jobId: string; idempotencyKey: string }): unknown {
  if (error instanceof ConvtError) {
    if (error.jobId) return error;
    return new ConvtError(error.code, error.message, {
      status: error.status,
      requestId: error.requestId,
      ...context,
      cause: error.cause ?? error,
    });
  }
  // A transport failure (a reset upload, a dropped poll) after the job exists:
  // keep the job id so the caller can check on it or cancel it.
  return new ConvtError(
    "network_error",
    `The request did not complete: ${String(error)}. The job may still be open; check it with convt.status(error.jobId).`,
    { ...context, cause: error },
  );
}

/**
 * Hex SHA-256 of a Blob, streamed chunk by chunk in every runtime: node:crypto
 * where it exists, a small built-in implementation elsewhere (browsers, where
 * bundlers may stub node:crypto). Stops on abort.
 */
async function sha256Hex(blob: Blob, signal: AbortSignal): Promise<string> {
  signal.throwIfAborted();
  let hash: { update(data: Uint8Array): unknown; hex(): string } | undefined;
  try {
    const nodeCrypto: unknown = await import("node:crypto");
    if (
      typeof nodeCrypto === "object" &&
      nodeCrypto !== null &&
      "createHash" in nodeCrypto &&
      typeof nodeCrypto.createHash === "function"
    ) {
      const node = (
        nodeCrypto.createHash as (algorithm: string) => {
          update(data: Uint8Array): unknown;
          digest(encoding: "hex"): string;
        }
      )("sha256");
      hash = { update: (data) => node.update(data), hex: () => node.digest("hex") };
    }
  } catch {
    hash = undefined;
  }
  hash ??= new Sha256();
  // The import above may have taken a while; an abort during it ends here.
  signal.throwIfAborted();
  const reader = blob.stream().getReader();
  const stop = () => void reader.cancel(signal.reason).catch(() => {});
  signal.addEventListener("abort", stop, { once: true });
  try {
    for (;;) {
      const { done, value } = await reader.read();
      signal.throwIfAborted();
      if (done) break;
      hash.update(value);
    }
  } finally {
    signal.removeEventListener("abort", stop);
  }
  return hash.hex();
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
  /**
   * Hex SHA-256 of the file. With an idempotency key, a retry that would upload
   * different bytes under that key is refused instead of converting them.
   */
  input_sha256?: string;
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
        { status: response.status },
      );
    return response;
  }
  /** Reads one output into memory. For large files, prefer save() or saveAll(). */
  async blob(index = 0): Promise<Blob> {
    return (await this.fetchOutput(index, 60_000)).blob();
  }
  /** Streams one output to disk without holding it in memory. Node, Bun and Deno. */
  private async stream(index: number, path: string): Promise<void> {
    const { Readable } = await import("node:stream");
    const { pipeline } = await import("node:stream/promises");
    const { link, mkdtemp, open, rm } = await import("node:fs/promises");
    const { createReadStream } = await import("node:fs");
    const { dirname, join } = await import("node:path");
    const response = await this.fetchOutput(index, 30 * 60_000);
    // Download into a private staging directory beside the target (mkdtemp makes
    // it owner-only), so an interrupted download never touches the target and
    // nobody else can read the bytes meanwhile. The staged file is created with
    // the ordinary mode for a new file, so the umask and any default ACL the
    // directory passes down apply exactly as for any other new file there.
    const staging = await mkdtemp(join(dirname(path), ".convt-"));
    const partial = join(staging, "part");
    try {
      const handle = await open(partial, "wx", 0o666);
      try {
        // `response.body` is checked in fetchOutput.
        await pipeline(Readable.fromWeb(response.body as never), handle.createWriteStream());
      } finally {
        await handle.close().catch(() => {});
      }
      // Decide only now whether the target exists, so a file created meanwhile
      // is never replaced. A new target appears atomically through a hard link
      // that fails if the name is taken.
      const created = await link(partial, path).then(
        () => true,
        (error: NodeJS.ErrnoException) => {
          if (error.code === "EEXIST") return false;
          throw error;
        },
      );
      if (!created) {
        // An existing file is written in place: opening it truncates it but
        // keeps its inode, owner, mode and ACLs, which a rename would replace.
        const target = await open(path, "w");
        try {
          await pipeline(createReadStream(partial), target.createWriteStream());
        } finally {
          await target.close().catch(() => {});
        }
      }
    } finally {
      await rm(staging, { recursive: true, force: true });
    }
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
    ) => new ConvtError(code, message, { status, requestId, idempotencyKey, cause });
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
    let dispatched = false;
    try {
      return await this.attempts<T>({
        path,
        method,
        data,
        signal,
        idempotencyKey,
        retryable,
        fail,
        unknown,
        onDispatch: () => (dispatched = true),
      });
    } catch (error) {
      // Once anything was sent, a failure outside the API's own answers leaves a
      // keyed create's outcome unknown; keep the key on it.
      if (dispatched && !(error instanceof ConvtError)) throw unknown(error);
      // A ConvtError raised after dispatch (say, by a credential callback on a
      // retry) still concerns a create that may have landed: add the key.
      if (
        dispatched &&
        error instanceof ConvtError &&
        !error.idempotencyKey &&
        method === "POST" &&
        idempotencyKey !== undefined
      )
        throw new ConvtError(error.code, error.message, {
          status: error.status,
          requestId: error.requestId,
          idempotencyKey,
          cause: error,
        });
      throw error;
    }
  }
  private async attempts<T>({
    path,
    method,
    data,
    signal,
    idempotencyKey,
    retryable,
    fail,
    unknown,
    onDispatch,
  }: {
    path: string;
    method: "GET" | "POST";
    data?: unknown;
    signal: AbortSignal;
    idempotencyKey?: string;
    retryable: boolean;
    fail: (
      code: string,
      message: string,
      status?: number,
      requestId?: string,
      cause?: unknown,
    ) => ConvtError;
    unknown: (cause: unknown) => unknown;
    onDispatch: () => void;
  }): Promise<T> {
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
      onDispatch();
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
        await response.body?.cancel().catch(() => {});
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
    // A caller's own key can be shared with another call (a retry still running),
    // so this call cancels the job only when the key is its own or the caller
    // aborted. Otherwise errors carry the job id, to resume or cancel explicitly.
    const ownsJob = options.idempotencyKey === undefined;
    const idempotencyKey = options.idempotencyKey ?? newIdempotencyKey();
    // Always fingerprint the bytes, so a later recovery with the key this call
    // reports sends exactly the same request.
    if (body) input = { ...input, input_sha256: await sha256Hex(body, signal) };
    const created = await this.create(input, { signal, idempotencyKey });
    const context = { jobId: created.job.id, idempotencyKey };
    try {
      if (body && created.upload_url) {
        const uploaded = await fetch(created.upload_url, { method: "PUT", body, signal });
        if (!uploaded.ok)
          throw new ConvtError("upload_failed", "Upload failed. Try again.", {
            status: uploaded.status,
            ...context,
          });
      }
      let job = body ? await this.start(created.job.id, signal) : created.job;
      let pause = 500;
      while (true) {
        options.onProgress?.(job);
        if (job.status === "succeeded") {
          try {
            return new Conversion(job, (await this.download(job.id, signal)).outputs);
          } catch (error) {
            throw new ConvtError(
              "download_failed",
              "The conversion succeeded and was billed, but its output links could not be fetched. Call convt.download(error.jobId) to get them; do not convert again.",
              { ...context, cause: error },
            );
          }
        }
        if (job.status === "failed" || job.status === "cancelled")
          throw new ConvtError(
            job.error_code ?? job.status,
            job.status === "cancelled"
              ? "Conversion cancelled."
              : job.error_code === "source_fetch_failed"
                ? "convt could not download source_url. Check that it is public and returns the file."
                : "Conversion failed. Try another format or use the desktop app.",
            context,
          );
        await sleep(pause, signal);
        pause = Math.min(pause * 1.5, 2000);
        job = await this.status(job.id, signal);
      }
    } catch (error) {
      // Billed already: never cancel, just point at the outputs.
      if (error instanceof ConvtError && error.code === "download_failed" && error.jobId)
        throw error;
      // The caller's own key may be shared by another call, so never cancel its
      // job from here, not even on abort: stop waiting and say how to cancel.
      if (!ownsJob) {
        if (signal.aborted)
          throw new ConvtError(
            "stopped_waiting",
            "Stopped waiting for the conversion. The job keeps running and is billed if it succeeds; call convt.cancel(error.jobId) to stop it.",
            { ...context, cause: error },
          );
        throw withContext(error, context);
      }
      let terminal: Job;
      try {
        terminal = await this.cancel(created.job.id);
      } catch {
        throw new ConvtCancellationError(created.job.id, { idempotencyKey });
      }
      if (terminal.status === "succeeded")
        throw new ConvtError(
          "already_completed",
          "The conversion finished before it could be cancelled and was billed. Call convt.download(error.jobId) for its outputs.",
          { ...context, cause: error },
        );
      if (signal.aborted) {
        if (terminal.status === "cancelled" || terminal.status === "failed")
          throw new ConvtError("cancelled", "Conversion cancelled. Your allowance was released.", {
            ...context,
            cause: error,
          });
        throw new ConvtCancellationError(created.job.id, { idempotencyKey });
      }
      throw withContext(error, context);
    }
  }
}

function formatFromName(name: string): Format | undefined {
  const file = name.split(/[\\/]/).pop() ?? "";
  if (!file.includes(".")) return undefined;
  const extension = file.split(".").pop()?.toLowerCase() ?? "";
  return formats.find((f) => (f.extensions as readonly string[]).includes(extension))?.id;
}
