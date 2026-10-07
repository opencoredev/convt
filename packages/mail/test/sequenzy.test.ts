import { describe, expect, test } from "bun:test";
import { sequenzyTransport, type SendResult } from "../src";

const email = {
  from: "convt <hello@convt.app>",
  to: "a@convt.test",
  subject: "License",
  text: "key",
  html: "<p>key</p>",
};
// Bun's fetch carries a preconnect property; attach it to test implementations too.
const fakeFetch = (
  handler: (input: Parameters<typeof fetch>[0], init?: RequestInit) => Promise<Response>,
): typeof fetch => Object.assign(handler, { preconnect: fetch.preconnect });

describe("Sequenzy transport", () => {
  test("sends the documented payload and stable key, with tracking off", async () => {
    const requests: string[] = [];
    const transport = sequenzyTransport({
      apiKey: "fake-key",
      fetch: fakeFetch(async (input, init) => {
        expect(input).toBe("https://api.sequenzy.com/api/v1/transactional/send");
        expect(init?.method).toBe("POST");
        const headers = new Headers(init?.headers);
        expect(headers.get("authorization")).toBe("Bearer fake-key");
        expect(headers.get("content-type")).toBe("application/json");
        expect(headers.get("idempotency-key")).toBe("eml_1");
        expect(JSON.parse(String(init?.body))).toEqual({
          from: email.from,
          to: email.to,
          subject: email.subject,
          body: email.html,
          emailType: "transactional",
          trackingSettings: { clickTracking: false, openTracking: false },
        });
        requests.push(String(init?.body));
        return Response.json(
          { success: true, emailSendId: "send_1", jobId: "job_1", to: email.to },
          { headers: requests.length > 1 ? { "Idempotent-Replayed": "true" } : {} },
        );
      }),
    });
    expect(await transport.send(email, "eml_1")).toEqual({ ok: true, id: "send_1" });
    expect(await transport.send(email, "eml_1")).toEqual({ ok: true, id: "send_1" });
    expect(requests[0]).toBe(requests[1]);
  });

  const cases: Array<{ status: number; body: unknown; expected: SendResult }> = [
    {
      status: 400,
      body: { success: false, error: "bad input" },
      expected: { ok: false, outcome: "dead", unknown: false, status: 400, code: "http_400" },
    },
    {
      status: 401,
      body: { success: false, error: "unauthorized" },
      expected: { ok: false, outcome: "dead", unknown: false, status: 401, code: "http_401" },
    },
    {
      status: 409,
      body: { success: false, code: "IDEMPOTENCY_KEY_REUSED" },
      expected: {
        ok: false,
        outcome: "dead",
        unknown: false,
        status: 409,
        code: "IDEMPOTENCY_KEY_REUSED",
      },
    },
    {
      status: 429,
      body: { success: false, error: "rate limited", retryable: true },
      expected: {
        ok: false,
        outcome: "retry",
        unknown: false,
        status: 429,
        code: "http_429",
        retryAfterMs: 120000,
      },
    },
    {
      status: 503,
      body: { success: false, code: "TRANSACTIONAL_ENQUEUE_UNAVAILABLE", retryable: true },
      expected: {
        ok: false,
        outcome: "retry",
        unknown: true,
        status: 503,
        code: "TRANSACTIONAL_ENQUEUE_UNAVAILABLE",
      },
    },
    {
      status: 500,
      body: { success: false, error: "failure" },
      expected: { ok: false, outcome: "retry", unknown: true, status: 500, code: "http_500" },
    },
    {
      status: 200,
      body: { success: true, jobId: "job_1" },
      expected: { ok: false, outcome: "retry", unknown: true, status: 200, code: "http_200" },
    },
  ];
  for (const { status, body, expected } of cases)
    test(`classifies ${status}`, async () => {
      const transport = sequenzyTransport({
        apiKey: "k",
        fetch: fakeFetch(async () =>
          Response.json(body, { status, headers: status === 429 ? { "Retry-After": "120" } : {} }),
        ),
      });
      expect(await transport.send(email, "eml_1")).toEqual(expected);
    });
  test("network failures are ambiguous", async () => {
    const transport = sequenzyTransport({
      apiKey: "k",
      fetch: fakeFetch(async () => {
        throw new TypeError("network");
      }),
    });
    expect(await transport.send(email, "eml_1")).toEqual({
      ok: false,
      outcome: "retry",
      unknown: true,
      status: null,
      code: "network_error",
    });
  });
  test("timeouts are ambiguous", async () => {
    const transport = sequenzyTransport({
      apiKey: "k",
      timeoutMs: 5,
      fetch: fakeFetch(
        async (_input, init) =>
          new Promise((_resolve, reject) =>
            init?.signal?.addEventListener("abort", () => reject(init.signal?.reason)),
          ),
      ),
    });
    expect(await transport.send(email, "eml_1")).toMatchObject({
      ok: false,
      unknown: true,
      code: "timeout",
    });
  });
  test("rejects keys outside the documented limit before sending", async () => {
    const transport = sequenzyTransport({
      apiKey: "k",
      fetch: fakeFetch(async () => {
        throw new Error("should not fetch");
      }),
    });
    for (const key of ["", "x".repeat(256)])
      expect(await transport.send(email, key)).toMatchObject({
        outcome: "dead",
        unknown: false,
        code: "invalid_idempotency_key",
      });
  });
});
