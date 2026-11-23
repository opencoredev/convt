import { afterEach, expect, test } from "bun:test";
import { Convt, Conversion } from "../src/index";
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
    },
    { input_format: "png", target_format: "jpeg", input_bytes: 3 },
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
  expect(error.code).toBe("network_error");
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
  globalThis.fetch = (async (_input: string | URL | Request) =>
    new Response("busy", { status: 503, headers: { "retry-after": "30" } })) as typeof fetch;
  const error = await new Convt({ apiKey: "test", baseUrl: "https://api.test" })
    .convertUrl("https://cdn.example.com/a.svg", { to: "png", timeoutMs: 100 })
    .catch((e) => e);
  expect(error.code).toBe("create_outcome_unknown");
  expect(error.idempotencyKey).toBeTruthy();
});
