import { formats, type Format } from "./formats.js";
export { formats, type Format };
export type Job = {
  id: string;
  status: "created" | "uploaded" | "queued" | "running" | "succeeded" | "failed" | "cancelled";
  input_format: Format;
  target_format: Format;
  input_bytes: number;
  attempt: number;
  error_code: string | null;
  expires_at: string;
};
export class ConvtError extends Error {
  constructor(
    readonly code: string,
    message: string,
    readonly status?: number,
  ) {
    super(message);
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
export type ConvertOptions = {
  to: Format;
  from?: Format;
  signal?: AbortSignal;
  onProgress?: (job: Job) => void;
  timeoutMs?: number;
};
export type Output = { name: string; url: string };
export class Conversion {
  constructor(
    readonly job: Job,
    readonly outputs: Output[],
  ) {}
  async blob(index = 0): Promise<Blob> {
    const output = this.outputs[index];
    if (!output) throw new ConvtError("missing_output", "Output does not exist.");
    const response = await fetch(output.url, { signal: AbortSignal.timeout(60_000) });
    if (!response.ok)
      throw new ConvtError(
        "download_failed",
        "The download expired or is unavailable.",
        response.status,
      );
    return response.blob();
  }
  async save(path: string): Promise<void> {
    if (typeof window !== "undefined")
      throw new ConvtError("node_only", "Use blob() or outputs in the browser.");
    if (this.outputs.length !== 1)
      throw new ConvtError(
        "multiple_outputs",
        "Use saveAll(directory) for jobs with several outputs.",
      );
    const { writeFile } = await import("node:fs/promises");
    await writeFile(path, new Uint8Array(await (await this.blob()).arrayBuffer()));
  }
  async saveAll(directory: string): Promise<void> {
    const { mkdir, writeFile } = await import("node:fs/promises");
    const { join, basename } = await import("node:path");
    await mkdir(directory, { recursive: true });
    for (let i = 0; i < this.outputs.length; i++)
      await writeFile(
        join(directory, basename(this.outputs[i].name)),
        new Uint8Array(await (await this.blob(i)).arrayBuffer()),
      );
  }
}
export class Convt {
  private readonly credential: string | (() => Promise<string>);
  private readonly baseUrl: string;
  constructor(
    options:
      | { apiKey: string; baseUrl?: string }
      | { token: () => Promise<string>; baseUrl: string } = {
      apiKey:
        typeof process !== "undefined"
          ? (process.env.CONVT_API_KEY ?? process.env.CONVT_KEY ?? "")
          : "",
    },
  ) {
    this.credential = "apiKey" in options ? options.apiKey : options.token;
    this.baseUrl = (options.baseUrl ?? "https://api.convt.app").replace(/\/$/, "");
    if (!this.credential)
      throw new ConvtError("api_key_required", "Pass apiKey or set CONVT_API_KEY.");
  }
  private async request<T>(
    path: string,
    method = "GET",
    data?: unknown,
    signal?: AbortSignal,
  ): Promise<T> {
    signal ??= AbortSignal.timeout(30_000);
    const credential = this.credential;
    const key = typeof credential === "function" ? await abortable(credential, signal) : credential;
    const response = await fetch(`${this.baseUrl}${path}`, {
      method,
      signal: signal ?? AbortSignal.timeout(30_000),
      headers: {
        authorization: `Bearer ${key}`,
        ...(data ? { "content-type": "application/json" } : {}),
      },
      ...(data ? { body: JSON.stringify(data) } : {}),
    });
    const value = await response.json();
    if (!response.ok)
      throw new ConvtError(
        value.error?.code ?? "request_failed",
        value.error?.message ?? "Request failed.",
        response.status,
      );
    return value as T;
  }
  create(
    input: { input_format: Format; target_format: Format; input_bytes: number },
    signal?: AbortSignal,
  ) {
    return this.request<{ job: Job; upload_url: string; upload_expires_in: number }>(
      "/v1/jobs",
      "POST",
      input,
      signal,
    );
  }
  start(id: string, signal?: AbortSignal) {
    return this.request<Job>(`/v1/jobs/${encodeURIComponent(id)}/start`, "POST", undefined, signal);
  }
  status(id: string, signal?: AbortSignal) {
    return this.request<Job>(`/v1/jobs/${encodeURIComponent(id)}`, "GET", undefined, signal);
  }
  cancel(id: string) {
    return this.request<Job>(`/v1/jobs/${encodeURIComponent(id)}/cancel`, "POST");
  }
  download(id: string, signal?: AbortSignal) {
    return this.request<{ outputs: Output[] }>(
      `/v1/jobs/${encodeURIComponent(id)}/download`,
      "GET",
      undefined,
      signal,
    );
  }
  async convert(input: string | Blob | Uint8Array, options: ConvertOptions): Promise<Conversion> {
    let body: Blob;
    let name = "";
    if (typeof input === "string") {
      if (typeof window !== "undefined")
        throw new ConvtError("file_required", "Pass a File or Blob in the browser.");
      const { readFile } = await import("node:fs/promises");
      const bytes = await readFile(input);
      body = new Blob([bytes]);
      name = input;
    } else if (input instanceof Blob) {
      body = input;
      name = "name" in input ? String(input.name) : "";
    } else {
      body = new Blob([new Uint8Array(input)]);
    }
    const extension = name.split(".").pop()?.toLowerCase();
    const from =
      options.from ??
      formats.find((f) => (f.extensions as readonly string[]).includes(extension ?? ""))?.id;
    if (!from)
      throw new ConvtError(
        "input_format_required",
        "Provide from for an input without a recognized extension.",
      );
    const deadline = AbortSignal.timeout(options.timeoutMs ?? 660_000);
    const signal = options.signal ? AbortSignal.any([options.signal, deadline]) : deadline;
    const created = await this.create(
      { input_format: from, target_format: options.to, input_bytes: body.size },
      signal,
    );
    try {
      const uploaded = await fetch(created.upload_url, { method: "PUT", body, signal });
      if (!uploaded.ok)
        throw new ConvtError("upload_failed", "Upload failed. Try again.", uploaded.status);
      let job = await this.start(created.job.id, signal);
      while (true) {
        options.onProgress?.(job);
        if (job.status === "succeeded")
          return new Conversion(job, (await this.download(job.id, signal)).outputs);
        if (job.status === "failed" || job.status === "cancelled")
          throw new ConvtError(
            job.error_code ?? job.status,
            job.status === "cancelled"
              ? "Conversion cancelled."
              : "Conversion failed. Try another format or use the desktop app.",
          );
        await new Promise<void>((resolve, reject) => {
          const abort = () => {
            clearTimeout(timer);
            reject(signal.reason);
          };
          const timer = setTimeout(() => {
            signal.removeEventListener("abort", abort);
            resolve();
          }, 1000);
          if (signal.aborted) abort();
          else signal.addEventListener("abort", abort, { once: true });
        });
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
