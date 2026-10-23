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
test("default base URL uses the live API host", async () => {
  const calls: string[] = [];
  globalThis.fetch = (async (input: string | URL | Request) => {
    calls.push(String(input));
    return Response.json({ error: { code: "unauthorized", message: "test" } }, { status: 401 });
  }) as typeof fetch;
  await expect(new Convt({ apiKey: "test" }).status("job_test")).rejects.toThrow();
  expect(calls).toEqual(["https://api.convt.app/v1/jobs/job_test"]);
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
  expect(error.message).toContain("settled");
});
