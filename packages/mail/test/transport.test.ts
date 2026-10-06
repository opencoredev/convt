import { describe, expect, test } from "bun:test";

import { resendTransport } from "../src";

const email = {
  from: "convt <hello@convt.app>",
  to: "a@convt.test",
  subject: "s",
  text: "t",
  html: "h",
};

function fake(status: number, body: unknown) {
  return (async () => new Response(JSON.stringify(body), { status })) as unknown as typeof fetch;
}

describe("resend transport classification", () => {
  test("success, retryable, dead and unknown outcomes", async () => {
    const cases: Array<[number, unknown, unknown]> = [
      [200, { id: "msg_1" }, { ok: true, id: "msg_1" }],
      [409, { name: "concurrent_idempotent_requests" }, { outcome: "retry", unknown: false }],
      [409, { name: "invalid_idempotent_request" }, { outcome: "dead", unknown: false }],
      [429, { name: "rate_limit_exceeded" }, { outcome: "retry", unknown: false }],
      [422, { name: "validation_error" }, { outcome: "dead", unknown: false }],
      [500, { name: "internal_server_error" }, { outcome: "retry", unknown: true }],
    ];
    for (const [status, body, want] of cases) {
      const t = resendTransport({ apiKey: "k", baseUrl: "http://x", fetch: fake(status, body) });
      expect(await t.send(email, "eml_1")).toMatchObject(want as object);
    }
  });

  test("a timeout is an unknown outcome", async () => {
    const slow = (async (_u: string, init: RequestInit) =>
      new Promise((_, reject) =>
        init.signal?.addEventListener("abort", () => reject(init.signal?.reason)),
      )) as unknown as typeof fetch;
    const t = resendTransport({ apiKey: "k", baseUrl: "http://x", fetch: slow, timeoutMs: 50 });
    expect(await t.send(email, "eml_1")).toMatchObject({
      ok: false,
      unknown: true,
      code: "timeout",
    });
  });
});
