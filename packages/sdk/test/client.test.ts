import { afterEach, expect, test } from "bun:test";
import { Convt, Conversion, ConvtError } from "../src/index";
const originalFetch = globalThis.fetch;
afterEach(() => {
  globalThis.fetch = originalFetch;
});
const job = {
  id: "job_test",
  status: "queued" as const,
  input_format: "svg" as const,
  target_format: "png" as const,
  input_bytes: 6,
  attempt: 0,
  error_code: null,
  expires_at: "2099-01-01T00:00:00Z",
};
test("timeout cancels an accepted job and does not poll forever", async () => {
  const calls: string[] = [];
  globalThis.fetch = (async (input: string | URL | Request) => {
    const url = String(input);
    calls.push(url);
    if (url.endsWith("/v1/jobs"))
      return Response.json({ job, upload_url: "https://objects.test/input" });
    if (url.includes("objects.test")) return new Response("", { status: 200 });
    return Response.json(job);
  }) as typeof fetch;
  await expect(
    new Convt({ apiKey: "test", baseUrl: "https://api.test" }).convert(new Blob(["<svg/>"]), {
      from: "svg",
      to: "png",
      timeoutMs: 30,
    }),
  ).rejects.toThrow();
  expect(calls.filter((url) => url.endsWith("/cancel"))).toHaveLength(1);
});
test("abort before creation leaves no reservation or upload", async () => {
  globalThis.fetch = (async (
    _input: string | URL | Request,
    init?: RequestInit,
  ): Promise<Response> => {
    init?.signal?.throwIfAborted();
    throw new Error("unexpected fetch");
  }) as typeof fetch;
  const controller = new AbortController();
  controller.abort(new Error("user cancelled"));
  await expect(
    new Convt({ apiKey: "test" }).convert(new Blob(["x"]), {
      from: "svg",
      to: "png",
      signal: controller.signal,
    }),
  ).rejects.toThrow("user cancelled");
});
test("several outputs must be saved explicitly", async () => {
  const result = new Conversion({ ...job, status: "succeeded" }, [
    { name: "1.png", url: "https://objects.test/1" },
    { name: "2.png", url: "https://objects.test/2" },
  ]);
  await expect(result.save("/tmp/should-not-be-written.png")).rejects.toThrow("saveAll");
  await expect(result.blob(2)).rejects.toThrow("does not exist");
});

test("stalled credential callback respects conversion deadline", async () => {
  const client = new Convt({
    token: () => new Promise<string>(() => {}),
    baseUrl: "https://api.test",
  });
  const started = Date.now();
  await expect(
    client.convert(new Blob(["x"]), { from: "svg", to: "png", timeoutMs: 20 }),
  ).rejects.toThrow();
  expect(Date.now() - started).toBeLessThan(200);
});
test("failed cancellation is exposed with a retryable job ID", async () => {
  const controller = new AbortController();
  globalThis.fetch = (async (input: string | URL | Request) => {
    const url = String(input);
    if (url.endsWith("/v1/jobs"))
      return Response.json({ job, upload_url: "https://objects.test/input" });
    if (url.endsWith("/cancel"))
      return Response.json({ error: { code: "unavailable" } }, { status: 503 });
    if (url.includes("objects.test")) {
      controller.abort();
      throw new Error("aborted");
    }
    return Response.json(job);
  }) as typeof fetch;
  const error = await new Convt({ apiKey: "test" })
    .convert(new Blob(["x"]), { from: "svg", to: "png", signal: controller.signal })
    .catch((e) => e);
  expect(error.code).toBe("cancellation_unconfirmed");
  expect(error.jobId).toBe(job.id);
});
test("completion racing cancellation does not claim released usage", async () => {
  const controller = new AbortController();
  globalThis.fetch = (async (input: string | URL | Request) => {
    const url = String(input);
    if (url.endsWith("/v1/jobs"))
      return Response.json({ job, upload_url: "https://objects.test/input" });
    if (url.endsWith("/cancel")) return Response.json({ ...job, status: "succeeded" });
    controller.abort();
    throw new Error("aborted");
  }) as typeof fetch;
  const error = await new Convt({ apiKey: "test" })
    .convert(new Blob(["x"]), { from: "svg", to: "png", signal: controller.signal })
    .catch((e) => e);
  expect(error.code).toBe("already_completed");
  expect(error.message).toContain("billed");
  expect(error.jobId).toBe(job.id);
});
test("conversion options are sent with the job, and left out when unset", async () => {
  const bodies: unknown[] = [];
  globalThis.fetch = (async (input: string | URL | Request, init?: RequestInit) => {
    const url = String(input);
    if (url.endsWith("/v1/jobs")) {
      bodies.push(JSON.parse(String(init?.body)));
      return Response.json({ job, upload_url: "https://objects.test/input" });
    }
    if (url.includes("objects.test")) return new Response("", { status: 200 });
    if (url.endsWith("/download"))
      return Response.json({ outputs: [{ name: "input.jpg", url: "https://objects.test/out" }] });
    return Response.json({ ...job, status: "succeeded" });
  }) as typeof fetch;
  const client = new Convt({ apiKey: "test", baseUrl: "https://api.test" });
  await client.convert(new Blob(["png"]), {
    from: "png",
    to: "jpeg",
    options: { quality: 40, max_size: 512 },
  });
  await client.convert(new Blob(["png"]), { from: "png", to: "jpeg" });
  expect(bodies).toEqual([
    {
      input_format: "png",
      target_format: "jpeg",
      input_bytes: 3,
      options: { quality: 40, max_size: 512 },
      input_sha256: "8f8cbb7dcf46e0bc7d53265749a6c17d116093a6ba95e442764060c76fd4a86c",
    },
    {
      input_format: "png",
      target_format: "jpeg",
      input_bytes: 3,
      input_sha256: "8f8cbb7dcf46e0bc7d53265749a6c17d116093a6ba95e442764060c76fd4a86c",
    },
  ]);
});
test("every convert sends an Idempotency-Key and retries a lost create with it", async () => {
  const keys: (string | null)[] = [];
  let creates = 0;
  globalThis.fetch = (async (input: string | URL | Request, init?: RequestInit) => {
    const url = String(input);
    if (url.endsWith("/v1/jobs")) {
      keys.push(new Headers(init?.headers).get("idempotency-key"));
      creates++;
      if (creates === 1) return new Response("bad gateway", { status: 502 });
      return Response.json({ job, upload_url: "https://objects.test/input" });
    }
    if (url.includes("objects.test")) return new Response("", { status: 200 });
    if (url.endsWith("/download"))
      return Response.json({ outputs: [{ name: "input.png", url: "https://objects.test/out" }] });
    return Response.json({ ...job, status: "succeeded" });
  }) as typeof fetch;
  await new Convt({ apiKey: "test", baseUrl: "https://api.test" }).convert(new Blob(["x"]), {
    from: "svg",
    to: "png",
  });
  expect(keys).toHaveLength(2);
  expect(keys[0]).toBeTruthy();
  expect(keys[1]).toBe(keys[0]);
});
test("a create without a key is never retried", async () => {
  let creates = 0;
  globalThis.fetch = (async (_input: string | URL | Request) => {
    creates++;
    return new Response("bad gateway", { status: 502 });
  }) as typeof fetch;
  const error = await new Convt({ apiKey: "test", baseUrl: "https://api.test" })
    .create({ input_format: "png", target_format: "webp", input_bytes: 1 })
    .catch((e) => e);
  expect(creates).toBe(1);
  expect(error.code).toBe("request_failed");
  expect(error.status).toBe(502);
});
test("rate limits honor Retry-After and errors carry the request id", async () => {
  const started = Date.now();
  let calls = 0;
  globalThis.fetch = (async (_input: string | URL | Request) => {
    calls++;
    if (calls === 1)
      return Response.json(
        { error: { code: "rate_limited", message: "slow down" } },
        { status: 429, headers: { "retry-after": "1" } },
      );
    return Response.json(
      { error: { code: "not_found", message: "Job not found." } },
      { status: 404, headers: { "x-request-id": "req_abc" } },
    );
  }) as typeof fetch;
  const error = await new Convt({ apiKey: "test", baseUrl: "https://api.test" })
    .status("job_x")
    .catch((e) => e);
  expect(calls).toBe(2);
  expect(Date.now() - started).toBeGreaterThanOrEqual(900);
  expect(error.code).toBe("not_found");
  expect(error.requestId).toBe("req_abc");
});
test("convertUrl sends source_url, skips the upload and infers the input format", async () => {
  const calls: string[] = [];
  const bodies: unknown[] = [];
  globalThis.fetch = (async (input: string | URL | Request, init?: RequestInit) => {
    const url = String(input);
    calls.push(url);
    if (url.endsWith("/v1/jobs")) {
      bodies.push(JSON.parse(String(init?.body)));
      return Response.json({ job, upload_url: null, upload_expires_in: null });
    }
    if (url.endsWith("/download"))
      return Response.json({ outputs: [{ name: "input.webp", url: "https://objects.test/out" }] });
    return Response.json({ ...job, status: "succeeded" });
  }) as typeof fetch;
  const result = await new Convt({ apiKey: "test", baseUrl: "https://api.test" }).convertUrl(
    "https://cdn.example.com/a/photo.PNG?X-Amz-Signature=1",
    { to: "webp", webhookUrl: "https://example.com/hook" },
  );
  expect(result.outputs).toHaveLength(1);
  expect(bodies).toEqual([
    {
      input_format: "png",
      target_format: "webp",
      source_url: "https://cdn.example.com/a/photo.PNG?X-Amz-Signature=1",
      webhook_url: "https://example.com/hook",
    },
  ]);
  expect(calls.some((u) => u.endsWith("/start") || u.includes("objects.test/input"))).toBe(false);
});
test("a non-JSON error page becomes a ConvtError, not a SyntaxError", async () => {
  globalThis.fetch = (async (_input: string | URL | Request) =>
    new Response("<html>oops</html>", { status: 500 })) as typeof fetch;
  const error = await new Convt({ apiKey: "test", baseUrl: "https://api.test", maxRetries: 0 })
    .status("job_x")
    .catch((e) => e);
  expect(error.name).toBe("ConvtError");
  expect(error.status).toBe(500);
});
test("a create whose body breaks off is retried under the same key", async () => {
  const keys: (string | null)[] = [];
  globalThis.fetch = (async (input: string | URL | Request, init?: RequestInit) => {
    const url = String(input);
    if (url.endsWith("/v1/jobs")) {
      keys.push(new Headers(init?.headers).get("idempotency-key"));
      if (keys.length === 1)
        return new Response(
          new ReadableStream({
            start(controller) {
              controller.enqueue(new TextEncoder().encode('{"job":'));
              controller.error(new Error("connection reset"));
            },
          }),
          { status: 200 },
        );
      return Response.json({ job, upload_url: null, upload_expires_in: null });
    }
    if (url.endsWith("/download"))
      return Response.json({ outputs: [{ name: "input.png", url: "https://objects.test/out" }] });
    return Response.json({ ...job, status: "succeeded" });
  }) as typeof fetch;
  await new Convt({ apiKey: "test", baseUrl: "https://api.test" }).convertUrl(
    "https://cdn.example.com/a.svg",
    { to: "png", idempotencyKey: "order-7" },
  );
  expect(keys).toEqual(["order-7", "order-7"]);
});
test("a create that keeps failing reports its idempotency key", async () => {
  globalThis.fetch = (async (_input: string | URL | Request): Promise<Response> => {
    throw new TypeError("fetch failed");
  }) as typeof fetch;
  const error = await new Convt({ apiKey: "test", baseUrl: "https://api.test", maxRetries: 1 })
    .create(
      { input_format: "png", target_format: "webp", input_bytes: 1 },
      { idempotencyKey: "k-1" },
    )
    .catch((e) => e);
  // A request that failed on the wire may still have reached convt.
  expect(error.code).toBe("create_outcome_unknown");
  expect(error.cause.code).toBe("network_error");
  expect(error.idempotencyKey).toBe("k-1");
});
test("an empty idempotency key is refused before anything is sent", async () => {
  let calls = 0;
  globalThis.fetch = (async (_input: string | URL | Request) => {
    calls++;
    return Response.json({});
  }) as typeof fetch;
  const error = await new Convt({ apiKey: "test", baseUrl: "https://api.test" })
    .convertUrl("https://cdn.example.com/a.svg", { to: "png", idempotencyKey: "" })
    .catch((e) => e);
  expect(error.code).toBe("invalid_idempotency_key");
  expect(calls).toBe(0);
});
test("a create cut off by the deadline reports the key it was sent with", async () => {
  globalThis.fetch = (async (input: string | URL | Request, init?: RequestInit) => {
    if (String(input).endsWith("/v1/jobs"))
      return new Response(
        new ReadableStream({
          start(controller) {
            controller.enqueue(new TextEncoder().encode("{"));
            init?.signal?.addEventListener("abort", () => controller.error(init.signal?.reason));
          },
        }),
        { status: 200 },
      );
    return Response.json({});
  }) as typeof fetch;
  const error = await new Convt({ apiKey: "test", baseUrl: "https://api.test", maxRetries: 0 })
    .convertUrl("https://cdn.example.com/a.svg", { to: "png", timeoutMs: 50 })
    .catch((e) => e);
  expect(error.code).toBe("create_outcome_unknown");
  expect(error.idempotencyKey).toMatch(/^[0-9a-f-]{36}$/);
});
test("save streams the output to disk", async () => {
  const { mkdtemp, readFile, rm } = await import("node:fs/promises");
  const { join } = await import("node:path");
  const { tmpdir } = await import("node:os");
  const dir = await mkdtemp(join(tmpdir(), "convt-sdk-"));
  globalThis.fetch = (async (_input: string | URL | Request) =>
    new Response(new Blob(["chunk-1", "chunk-2"]))) as typeof fetch;
  await new Conversion({ ...job, status: "succeeded" }, [
    { name: "input.png", url: "https://objects.test/out" },
  ]).save(join(dir, "out.png"));
  expect(await readFile(join(dir, "out.png"), "utf8")).toBe("chunk-1chunk-2");
  await rm(dir, { recursive: true });
});
test("a billed conversion whose links fail keeps its job id and is not cancelled", async () => {
  const calls: string[] = [];
  globalThis.fetch = (async (input: string | URL | Request) => {
    const url = String(input);
    calls.push(url);
    if (url.endsWith("/v1/jobs"))
      return Response.json({ job: { ...job, status: "succeeded" }, upload_url: null });
    if (url.endsWith("/download"))
      return Response.json(
        { error: { code: "storage_unavailable", message: "x" } },
        { status: 400 },
      );
    return Response.json({ ...job, status: "succeeded" });
  }) as typeof fetch;
  const error = await new Convt({ apiKey: "test", baseUrl: "https://api.test" })
    .convertUrl("https://cdn.example.com/a.svg", { to: "png" })
    .catch((e) => e);
  expect(error.code).toBe("download_failed");
  expect(error.jobId).toBe(job.id);
  expect(error.idempotencyKey).toBeTruthy();
  expect(calls.some((u) => u.endsWith("/cancel"))).toBe(false);
});
test("with the caller's own key, a failed upload leaves the shared job alone", async () => {
  const calls: string[] = [];
  globalThis.fetch = (async (input: string | URL | Request) => {
    const url = String(input);
    calls.push(url);
    if (url.endsWith("/v1/jobs"))
      return Response.json({ job, upload_url: "https://objects.test/in" });
    if (url.includes("objects.test")) return new Response("", { status: 503 });
    return Response.json(job);
  }) as typeof fetch;
  const error = await new Convt({ apiKey: "test", baseUrl: "https://api.test" })
    .convert(new Blob(["x"]), { from: "svg", to: "png", idempotencyKey: "order-9" })
    .catch((e) => e);
  expect(error.code).toBe("upload_failed");
  expect(error.jobId).toBe(job.id);
  expect(error.idempotencyKey).toBe("order-9");
  expect(calls.some((u) => u.endsWith("/cancel"))).toBe(false);
});
test("a deadline during retry backoff still reports the create's key", async () => {
  globalThis.fetch = (async (_input: string | URL | Request) => {
    throw new TypeError("connection reset");
  }) as unknown as typeof fetch;
  const error = await new Convt({ apiKey: "test", baseUrl: "https://api.test" })
    .convertUrl("https://cdn.example.com/a.svg", { to: "png", timeoutMs: 100 })
    .catch((e) => e);
  expect(error.code).toBe("create_outcome_unknown");
  expect(error.idempotencyKey).toBeTruthy();
});
test("a refused create whose Retry-After cannot fit the deadline is reported as refused", async () => {
  globalThis.fetch = (async (_input: string | URL | Request) =>
    Response.json(
      { error: { code: "rate_limited", message: "slow down" } },
      { status: 429, headers: { "retry-after": "10" } },
    )) as typeof fetch;
  const error = await new Convt({ apiKey: "test", baseUrl: "https://api.test" })
    .convertUrl("https://cdn.example.com/a.svg", { to: "png", timeoutMs: 100 })
    .catch((e) => e);
  expect(error.code).toBe("rate_limited");
  expect(error.idempotencyKey).toBeTruthy();
});
test("a first gateway timeout with a long Retry-After leaves a keyed create unknown", async () => {
  globalThis.fetch = (async (_input: string | URL | Request) =>
    new Response("gateway timeout", {
      status: 504,
      headers: { "retry-after": "90" },
    })) as typeof fetch;
  const error = await new Convt({ apiKey: "test", baseUrl: "https://api.test" })
    .convertUrl("https://cdn.example.com/a.svg", {
      to: "png",
      idempotencyKey: "order-5",
      timeoutMs: 1000,
    })
    .catch((e) => e);
  expect(error.code).toBe("create_outcome_unknown");
  expect(error.idempotencyKey).toBe("order-5");
});
test("two refusals in a row stay refusals", async () => {
  let creates = 0;
  globalThis.fetch = (async (_input: string | URL | Request) => {
    creates++;
    return Response.json(
      { error: { code: "rate_limited", message: "slow down" } },
      { status: 429, headers: { "retry-after": creates === 1 ? "0" : "999" } },
    );
  }) as typeof fetch;
  const error = await new Convt({ apiKey: "test", baseUrl: "https://api.test" })
    .create(
      { input_format: "png", target_format: "webp", input_bytes: 1 },
      { idempotencyKey: "k-3" },
    )
    .catch((e) => e);
  expect(creates).toBe(2);
  expect(error.code).toBe("rate_limited");
});
test("the wait budget counts time spent waiting on the answer", async () => {
  let polls = 0;
  globalThis.fetch = (async (input: string | URL | Request) => {
    const url = String(input);
    if (url.endsWith("/v1/jobs")) return Response.json({ job, upload_url: null });
    if (url.endsWith("/cancel")) return Response.json({ ...job, status: "cancelled" });
    polls++;
    await new Promise((resolve) => setTimeout(resolve, 500));
    return Response.json(
      { error: { code: "rate_limited", message: "slow down" } },
      { status: 429, headers: { "retry-after": "1", "x-request-id": "req_9" } },
    );
  }) as typeof fetch;
  const error = await new Convt({ apiKey: "test", baseUrl: "https://api.test" })
    // 1750 ms: the budget left after the first poll pause (about 1250 ms) still
    // fits the 1 s wait, so only a budget rechecked after the slow answer refuses.
    .convertUrl("https://cdn.example.com/a.svg", { to: "png", timeoutMs: 1750 })
    .catch((e) => e);
  expect(polls).toBe(1);
  expect(error.code).toBe("rate_limited");
  expect(error.requestId).toBe("req_9");
});
test("with the caller's own key, a deadline stops waiting without cancelling", async () => {
  const calls: string[] = [];
  const bodies: Record<string, unknown>[] = [];
  globalThis.fetch = (async (input: string | URL | Request, init?: RequestInit) => {
    const url = String(input);
    calls.push(url);
    if (url.endsWith("/v1/jobs")) {
      bodies.push(JSON.parse(String(init?.body)));
      return Response.json({ job, upload_url: "https://objects.test/in" });
    }
    if (url.includes("objects.test")) return new Response("", { status: 200 });
    return Response.json(job);
  }) as typeof fetch;
  const error = await new Convt({ apiKey: "test", baseUrl: "https://api.test" })
    .convert(new Blob(["abc"]), {
      from: "svg",
      to: "png",
      idempotencyKey: "order-11",
      timeoutMs: 80,
    })
    .catch((e) => e);
  expect(error.code).toBe("stopped_waiting");
  expect(error.jobId).toBe(job.id);
  expect(calls.some((u) => u.endsWith("/cancel"))).toBe(false);
  // SHA-256 of "abc".
  expect(bodies[0].input_sha256).toBe(
    "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
  );
});
test("a reset upload keeps the job id and key", async () => {
  globalThis.fetch = (async (input: string | URL | Request) => {
    const url = String(input);
    if (url.endsWith("/v1/jobs"))
      return Response.json({ job, upload_url: "https://objects.test/in" });
    if (url.includes("objects.test")) throw new TypeError("socket hang up");
    return Response.json(job);
  }) as typeof fetch;
  const error = await new Convt({ apiKey: "test", baseUrl: "https://api.test" })
    .convert(new Blob(["x"]), { from: "svg", to: "png", idempotencyKey: "order-12" })
    .catch((e) => e);
  expect(error.code).toBe("network_error");
  expect(error.jobId).toBe(job.id);
  expect(error.idempotencyKey).toBe("order-12");
});
test("an abort while hashing a caller-keyed file stops at once and sends nothing", async () => {
  let calls = 0;
  globalThis.fetch = (async (_input: string | URL | Request) => {
    calls++;
    return Response.json({});
  }) as typeof fetch;
  // A file whose bytes never arrive, like a stalled network drive.
  class StalledBlob extends Blob {
    override stream(): ReadableStream<Uint8Array<ArrayBuffer>> {
      return new ReadableStream<Uint8Array<ArrayBuffer>>({ pull: () => new Promise(() => {}) });
    }
  }
  const stalled = new StalledBlob(["0123456789"]);
  const controller = new AbortController();
  setTimeout(() => controller.abort(new Error("user cancelled")), 15);
  const started = Date.now();
  const error = await new Convt({ apiKey: "test", baseUrl: "https://api.test" })
    .convert(stalled, { from: "svg", to: "png", idempotencyKey: "k", signal: controller.signal })
    .catch((e) => e);
  expect(Date.now() - started).toBeLessThan(200);
  expect(String(error.message ?? error)).toContain("user cancelled");
  expect(calls).toBe(0);
});
test("recovering with the key an error reports sends the identical request", async () => {
  const bodies: string[] = [];
  const keys: (string | null)[] = [];
  let first = true;
  globalThis.fetch = (async (input: string | URL | Request, init?: RequestInit) => {
    const url = String(input);
    if (url.endsWith("/v1/jobs")) {
      bodies.push(String(init?.body));
      keys.push(new Headers(init?.headers).get("idempotency-key"));
      if (first) {
        first = false;
        throw new TypeError("connection reset");
      }
      return Response.json({ job, upload_url: "https://objects.test/in" });
    }
    if (url.includes("objects.test")) return new Response("", { status: 200 });
    if (url.endsWith("/download"))
      return Response.json({ outputs: [{ name: "input.png", url: "https://objects.test/out" }] });
    return Response.json({ ...job, status: "succeeded" });
  }) as typeof fetch;
  const client = new Convt({ apiKey: "test", baseUrl: "https://api.test", maxRetries: 0 });
  const error = await client.convert(new Blob(["svg"]), { from: "svg", to: "png" }).catch((e) => e);
  expect(error.idempotencyKey).toBeTruthy();
  await client.convert(new Blob(["svg"]), {
    from: "svg",
    to: "png",
    idempotencyKey: error.idempotencyKey,
  });
  expect(keys[1]).toBe(keys[0]);
  expect(bodies[1]).toBe(bodies[0]);
});
test("an abort right as convert starts stops before reading the file", async () => {
  let calls = 0;
  globalThis.fetch = (async (_input: string | URL | Request) => {
    calls++;
    return Response.json({});
  }) as typeof fetch;
  class StalledBlob extends Blob {
    override stream(): ReadableStream<Uint8Array<ArrayBuffer>> {
      return new ReadableStream<Uint8Array<ArrayBuffer>>({ pull: () => new Promise(() => {}) });
    }
  }
  const controller = new AbortController();
  const started = Date.now();
  const pending = new Convt({ apiKey: "test", baseUrl: "https://api.test" })
    .convert(new StalledBlob(["0123456789"]), { from: "svg", to: "png", signal: controller.signal })
    .catch((e) => e);
  controller.abort(new Error("user cancelled"));
  const error = await pending;
  expect(Date.now() - started).toBeLessThan(200);
  expect(String(error.message ?? error)).toContain("user cancelled");
  expect(calls).toBe(0);
});
test("save keeps a private file private and accepts long names", async () => {
  const { chmod, mkdtemp, readFile, rm, stat, writeFile, readdir } =
    await import("node:fs/promises");
  const { join } = await import("node:path");
  const { tmpdir } = await import("node:os");
  const dir = await mkdtemp(join(tmpdir(), "convt-sdk-"));
  const target = join(dir, `${"n".repeat(235)}.png`);
  await writeFile(target, "old");
  await chmod(target, 0o640);
  const before = await stat(target);
  globalThis.fetch = (async (_input: string | URL | Request) =>
    new Response(new Blob(["new-bytes"]))) as typeof fetch;
  await new Conversion({ ...job, status: "succeeded" }, [
    { name: "input.png", url: "https://objects.test/out" },
  ]).save(target);
  expect(await readFile(target, "utf8")).toBe("new-bytes");
  const after = await stat(target);
  // Same file, written in place: owner, mode and any ACLs stay as they were.
  expect(after.ino).toBe(before.ino);
  expect(after.mode & 0o777).toBe(0o640);
  expect((await readdir(dir)).length).toBe(1);
  await rm(dir, { recursive: true });
});
test("a target created during the download is written in place, not replaced", async () => {
  const { mkdtemp, readFile, rm, stat, writeFile, chmod } = await import("node:fs/promises");
  const { join } = await import("node:path");
  const { tmpdir } = await import("node:os");
  const dir = await mkdtemp(join(tmpdir(), "convt-sdk-"));
  const target = join(dir, "out.png");
  let release: () => void = () => {};
  const gate = new Promise<void>((resolve) => (release = resolve));
  globalThis.fetch = (async (_input: string | URL | Request) =>
    new Response(
      new ReadableStream({
        async start(controller) {
          controller.enqueue(new TextEncoder().encode("new-"));
          await gate;
          controller.enqueue(new TextEncoder().encode("bytes"));
          controller.close();
        },
      }),
    )) as typeof fetch;
  const saving = new Conversion({ ...job, status: "succeeded" }, [
    { name: "input.png", url: "https://objects.test/out" },
  ]).save(target);
  await new Promise((resolve) => setTimeout(resolve, 30));
  await writeFile(target, "someone else's");
  await chmod(target, 0o600);
  const before = await stat(target);
  release();
  await saving;
  const after = await stat(target);
  expect(after.ino).toBe(before.ino);
  expect(after.mode & 0o777).toBe(0o600);
  expect(await readFile(target, "utf8")).toBe("new-bytes");
  await rm(dir, { recursive: true });
});
test("a gateway error whose body cannot be read still reports the key", async () => {
  globalThis.fetch = (async (_input: string | URL | Request) =>
    new Response(
      new ReadableStream({
        start(controller) {
          controller.error(new Error("broken body"));
        },
      }),
      { status: 502 },
    )) as typeof fetch;
  const error = await new Convt({ apiKey: "test", baseUrl: "https://api.test", maxRetries: 1 })
    .convertUrl("https://cdn.example.com/a.svg", { to: "png" })
    .catch((e) => e);
  expect(error.idempotencyKey).toBeTruthy();
});
test("a ConvtError from the credential callback after a send keeps the key", async () => {
  let calls = 0;
  globalThis.fetch = (async (_input: string | URL | Request) =>
    new Response("busy", { status: 503 })) as typeof fetch;
  const client = new Convt({
    baseUrl: "https://api.test",
    maxRetries: 2,
    token: async () => {
      calls++;
      if (calls > 1) throw new ConvtError("token_failed", "Could not refresh the token.");
      return "tok";
    },
  });
  const error = await client
    .convertUrl("https://cdn.example.com/a.svg", { to: "png" })
    .catch((e) => e);
  // The 503 may hide a create that landed, so the outcome is unknown; the
  // credential failure is the cause.
  expect(error.code).toBe("create_outcome_unknown");
  expect(error.cause.code).toBe("token_failed");
  expect(error.idempotencyKey).toBeTruthy();
});
test("a remote create may leave the input format to the URL", async () => {
  globalThis.fetch = (async (_input: string | URL | Request) =>
    Response.json({ job, upload_url: null, upload_expires_in: null })) as typeof fetch;
  const created = await new Convt({ apiKey: "test", baseUrl: "https://api.test" }).create({
    source_url: "https://cdn.example.com/a.png",
    target_format: "webp",
  });
  expect(created.job.id).toBe(job.id);
});
test("create still takes an AbortSignal as its second argument", async () => {
  globalThis.fetch = (async (_input: string | URL | Request, init?: RequestInit) => {
    init?.signal?.throwIfAborted();
    return Response.json({ job, upload_url: null, upload_expires_in: null });
  }) as typeof fetch;
  const controller = new AbortController();
  controller.abort(new Error("stop"));
  const error = await new Convt({ apiKey: "test", baseUrl: "https://api.test" })
    .create({ input_format: "png", target_format: "webp", input_bytes: 1 }, controller.signal)
    .catch((e) => e);
  expect(String(error.message ?? error)).toContain("stop");
});
test("a Retry-After longer than the call's deadline is reported, not slept on", async () => {
  let calls = 0;
  globalThis.fetch = (async (_input: string | URL | Request) => {
    calls++;
    return Response.json(
      { error: { code: "rate_limited", message: "slow down" } },
      { status: 429, headers: { "retry-after": "45" } },
    );
  }) as typeof fetch;
  const started = Date.now();
  const error = await new Convt({ apiKey: "test", baseUrl: "https://api.test" })
    .status("job_x")
    .catch((e) => e);
  expect(Date.now() - started).toBeLessThan(1000);
  expect(calls).toBe(1);
  expect(error.code).toBe("rate_limited");
  expect(error.message).toContain("45");
});
test("a long Retry-After while a conversion polls is waited out in full, then polling resumes", async () => {
  const calls: string[] = [];
  let polls = 0;
  globalThis.fetch = (async (input: string | URL | Request) => {
    const url = String(input);
    calls.push(url);
    if (url.endsWith("/v1/jobs")) return Response.json({ job, upload_url: null });
    if (url.endsWith("/cancel")) return Response.json({ ...job, status: "cancelled" });
    if (url.endsWith("/download"))
      return Response.json({ outputs: [{ name: "a.png", url: "https://objects.test/out" }] });
    if (++polls === 1)
      return Response.json(
        { error: { code: "rate_limited", message: "slow down" } },
        { status: 429, headers: { "retry-after": "90" } },
      );
    return Response.json({ ...job, status: "succeeded" });
  }) as typeof fetch;
  // A controlled clock: record each long wait and run it at once.
  const realSetTimeout = globalThis.setTimeout;
  const waits: number[] = [];
  globalThis.setTimeout = ((fn: () => void, ms?: number) => {
    if ((ms ?? 0) >= 20_000) {
      waits.push(ms ?? 0);
      return realSetTimeout(fn, 0);
    }
    return realSetTimeout(fn, ms);
  }) as typeof setTimeout;
  try {
    const result = await new Convt({ apiKey: "test", baseUrl: "https://api.test" }).convertUrl(
      "https://cdn.example.com/a.svg",
      { to: "png" },
    );
    expect(result.outputs).toHaveLength(1);
  } finally {
    globalThis.setTimeout = realSetTimeout;
  }
  expect(waits).toContain(90_000);
  expect(polls).toBe(2);
  expect(calls.some((u) => u.endsWith("/cancel"))).toBe(false);
});
test("create tells an AbortSignal from an options object that looks like one", async () => {
  const keys: (string | null)[] = [];
  globalThis.fetch = (async (_input: string | URL | Request, init?: RequestInit) => {
    keys.push(new Headers(init?.headers).get("idempotency-key"));
    return Response.json({ job, upload_url: null, upload_expires_in: null });
  }) as typeof fetch;
  const options = Object.assign(new EventTarget(), {
    aborted: false,
    signal: new AbortController().signal,
    idempotencyKey: "order-9",
  });
  await new Convt({ apiKey: "test", baseUrl: "https://api.test" }).create(
    { input_format: "png", target_format: "webp", input_bytes: 1 },
    options,
  );
  expect(keys).toEqual(["order-9"]);
});
test("a caller's own signal does not wait out a day-long Retry-After", async () => {
  let calls = 0;
  globalThis.fetch = (async (_input: string | URL | Request) => {
    calls++;
    return Response.json(
      { error: { code: "rate_limited", message: "slow down" } },
      { status: 429, headers: { "retry-after": "86400" } },
    );
  }) as typeof fetch;
  const started = Date.now();
  const error = await new Convt({ apiKey: "test", baseUrl: "https://api.test" })
    .status("job_x", new AbortController().signal)
    .catch((e) => e);
  expect(Date.now() - started).toBeLessThan(1000);
  expect(calls).toBe(1);
  expect(error.code).toBe("rate_limited");
});
test("a Retry-After past a conversion's deadline is reported, not retried at once", async () => {
  let polls = 0;
  globalThis.fetch = (async (input: string | URL | Request) => {
    const url = String(input);
    if (url.endsWith("/v1/jobs")) return Response.json({ job, upload_url: null });
    if (url.endsWith("/cancel")) return Response.json({ ...job, status: "cancelled" });
    polls++;
    return Response.json(
      { error: { code: "rate_limited", message: "slow down" } },
      { status: 429, headers: { "retry-after": "2147484" } },
    );
  }) as typeof fetch;
  const error = await new Convt({ apiKey: "test", baseUrl: "https://api.test" })
    .convertUrl("https://cdn.example.com/a.svg", { to: "png" })
    .catch((e) => e);
  expect(polls).toBe(1);
  expect(error.code).toBe("rate_limited");
});
test("a create retried after a gateway error reports an unknown outcome on a long Retry-After", async () => {
  let creates = 0;
  globalThis.fetch = (async (_input: string | URL | Request) => {
    creates++;
    if (creates === 1) return new Response("bad gateway", { status: 502 });
    return new Response("busy", { status: 503, headers: { "retry-after": "999" } });
  }) as typeof fetch;
  const error = await new Convt({ apiKey: "test", baseUrl: "https://api.test" })
    .create(
      { input_format: "png", target_format: "webp", input_bytes: 1 },
      { idempotencyKey: "k-2" },
    )
    .catch((e) => e);
  expect(creates).toBe(2);
  expect(error.code).toBe("create_outcome_unknown");
  expect(error.idempotencyKey).toBe("k-2");
});
const createInput = { input_format: "png", target_format: "webp", input_bytes: 1 } as const;
test("a keyed create whose only answer is a 504 ends unknown when retries run out", async () => {
  globalThis.fetch = (async (_input: string | URL | Request) =>
    new Response("<html>gateway timeout</html>", { status: 504 })) as typeof fetch;
  const error = await new Convt({ apiKey: "test", baseUrl: "https://api.test", maxRetries: 0 })
    .create(createInput, { idempotencyKey: "k-4" })
    .catch((e) => e);
  expect(error.code).toBe("create_outcome_unknown");
  expect(error.idempotencyKey).toBe("k-4");
});
test("a 504 then a final 429 leaves a keyed create unknown", async () => {
  let creates = 0;
  globalThis.fetch = (async (_input: string | URL | Request) => {
    creates++;
    if (creates === 1) return new Response("gateway timeout", { status: 504 });
    return Response.json(
      { error: { code: "rate_limited", message: "slow down" } },
      { status: 429, headers: { "retry-after": "0" } },
    );
  }) as typeof fetch;
  const error = await new Convt({ apiKey: "test", baseUrl: "https://api.test", maxRetries: 1 })
    .create(createInput, { idempotencyKey: "k-5" })
    .catch((e) => e);
  expect(creates).toBe(2);
  expect(error.code).toBe("create_outcome_unknown");
});
test("a 503 is never taken as a refusal, whatever request id it carries", async () => {
  globalThis.fetch = (async (_input: string | URL | Request) =>
    new Response("busy", {
      status: 503,
      headers: { "retry-after": "999", "x-request-id": "req_proxy" },
    })) as typeof fetch;
  const error = await new Convt({ apiKey: "test", baseUrl: "https://api.test" })
    .create(createInput, { idempotencyKey: "k-6" })
    .catch((e) => e);
  expect(error.code).toBe("create_outcome_unknown");
});
test("a 429 without Retry-After stays a refusal when the fallback wait cannot fit", async () => {
  globalThis.fetch = (async (_input: string | URL | Request) =>
    Response.json(
      { error: { code: "rate_limited", message: "slow down" } },
      { status: 429 },
    )) as typeof fetch;
  const error = await new Convt({ apiKey: "test", baseUrl: "https://api.test" })
    .convertUrl("https://cdn.example.com/a.svg", { to: "png", timeoutMs: 120 })
    .catch((e) => e);
  expect(error.code).toBe("rate_limited");
  expect(error.idempotencyKey).toBeTruthy();
});
test("a key conflict after a gateway error stays the conflict", async () => {
  let creates = 0;
  globalThis.fetch = (async (_input: string | URL | Request) => {
    creates++;
    if (creates === 1) return new Response("gateway timeout", { status: 504 });
    return Response.json(
      { error: { code: "idempotency_key_reused", message: "Key used for another request." } },
      { status: 409 },
    );
  }) as typeof fetch;
  const error = await new Convt({ apiKey: "test", baseUrl: "https://api.test", maxRetries: 1 })
    .create(createInput, { idempotencyKey: "k-7" })
    .catch((e) => e);
  expect(error.code).toBe("idempotency_key_reused");
  expect(error.idempotencyKey).toBe("k-7");
});
test("an HTML 429 is still a refusal", async () => {
  globalThis.fetch = (async (_input: string | URL | Request) =>
    new Response("<html>too many</html>", { status: 429 })) as typeof fetch;
  const error = await new Convt({ apiKey: "test", baseUrl: "https://api.test", maxRetries: 0 })
    .create(createInput, { idempotencyKey: "k-8" })
    .catch((e) => e);
  expect(error.code).toBe("request_failed");
  expect(error.status).toBe(429);
});
test("a 200 without a job leaves a keyed create unknown", async () => {
  globalThis.fetch = (async (_input: string | URL | Request) =>
    Response.json({ ok: true })) as typeof fetch;
  const error = await new Convt({ apiKey: "test", baseUrl: "https://api.test", maxRetries: 0 })
    .create(createInput, { idempotencyKey: "k-9" })
    .catch((e) => e);
  expect(error.code).toBe("create_outcome_unknown");
  expect(error.cause.code).toBe("invalid_response");
});
test("every retry under a key sends the body of the first attempt", async () => {
  const bodies: string[] = [];
  const input: {
    input_format: string;
    target_format: string;
    input_bytes: number;
    options?: unknown;
  } = { input_format: "png", target_format: "webp", input_bytes: 1, options: { quality: 70 } };
  globalThis.fetch = (async (_input: string | URL | Request, init?: RequestInit) => {
    bodies.push(String(init?.body));
    if (bodies.length === 1) {
      input.options = { quality: 85 };
      return new Response("gateway timeout", { status: 504, headers: { "retry-after": "0" } });
    }
    return Response.json({ job, upload_url: null, upload_expires_in: null });
  }) as typeof fetch;
  await new Convt({ apiKey: "test", baseUrl: "https://api.test", maxRetries: 1 }).create(
    input as never,
    { idempotencyKey: "k-11" },
  );
  expect(bodies).toHaveLength(2);
  expect(bodies[1]).toBe(bodies[0]);
});
test("an abort while backing off from a 429 stays an abort", async () => {
  const controller = new AbortController();
  globalThis.fetch = (async (_input: string | URL | Request) => {
    setTimeout(() => controller.abort(new Error("user stopped")), 20);
    return Response.json(
      { error: { code: "rate_limited", message: "slow down" } },
      { status: 429, headers: { "retry-after": "5" } },
    );
  }) as typeof fetch;
  const error = await new Convt({ apiKey: "test", baseUrl: "https://api.test" })
    .create(createInput, { idempotencyKey: "k-12", signal: controller.signal })
    .catch((e) => e);
  expect(error.message).toBe("user stopped");
});
test("a credential failure after a 429 is that failure", async () => {
  let tokens = 0;
  globalThis.fetch = (async (_input: string | URL | Request) =>
    Response.json(
      { error: { code: "rate_limited", message: "slow down" } },
      { status: 429, headers: { "retry-after": "0" } },
    )) as typeof fetch;
  const error = await new Convt({
    baseUrl: "https://api.test",
    maxRetries: 1,
    token: async () => {
      if (++tokens > 1) throw new Error("token service down");
      return "t";
    },
  })
    .create(createInput, { idempotencyKey: "k-13" })
    .catch((e) => e);
  expect(error.message).toBe("token service down");
});
test("a 200 with a partial job leaves a keyed create unknown", async () => {
  globalThis.fetch = (async (_input: string | URL | Request) =>
    Response.json({ job: { id: "job_1" } })) as typeof fetch;
  const error = await new Convt({ apiKey: "test", baseUrl: "https://api.test", maxRetries: 0 })
    .create(createInput, { idempotencyKey: "k-14" })
    .catch((e) => e);
  expect(error.code).toBe("create_outcome_unknown");
});
test("a create rejected with an object String() cannot show still keeps its key", async () => {
  globalThis.fetch = (async (_input: string | URL | Request) => {
    throw Object.create(null);
  }) as unknown as typeof fetch;
  const error = await new Convt({ apiKey: "test", baseUrl: "https://api.test", maxRetries: 0 })
    .create(createInput, { idempotencyKey: "k-15" })
    .catch((e) => e);
  expect(error.code).toBe("create_outcome_unknown");
  expect(error.idempotencyKey).toBe("k-15");
});
test("an already aborted create reports the abort without reading its payload", async () => {
  const controller = new AbortController();
  controller.abort(new Error("stopped first"));
  const input = {
    input_format: "png",
    target_format: "webp",
    input_bytes: 1,
    get options(): never {
      throw new Error("payload read");
    },
  };
  const error = await new Convt({ apiKey: "test", baseUrl: "https://api.test" })
    .create(input as never, { signal: controller.signal })
    .catch((e) => e);
  expect(error.message).toBe("stopped first");
});
test("save rewrites a writable file in a folder that allows no new files", async () => {
  const { mkdtemp, mkdir, writeFile, readFile, chmod, readdir, rm } =
    await import("node:fs/promises");
  const { join } = await import("node:path");
  const { tmpdir } = await import("node:os");
  const root = await mkdtemp(join(tmpdir(), "sdk-ro-"));
  const folder = join(root, "ro");
  await mkdir(folder);
  await writeFile(join(folder, "out.png"), "old");
  await chmod(folder, 0o555);
  globalThis.fetch = (async (_input: string | URL | Request) =>
    new Response("new bytes")) as typeof fetch;
  try {
    await new Conversion({ ...job, status: "succeeded" }, [
      { name: "a.png", url: "https://objects.test/out" },
    ]).save(join(folder, "out.png"));
    expect(await readFile(join(folder, "out.png"), "utf8")).toBe("new bytes");
    expect(await readdir(folder)).toEqual(["out.png"]);
  } finally {
    await chmod(folder, 0o755);
    await rm(root, { recursive: true, force: true });
  }
});
