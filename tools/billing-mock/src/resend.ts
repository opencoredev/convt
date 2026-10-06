// Resend's POST /emails, with the documented idempotency semantics: keys are kept
// 24 hours (on the mock's clock); the same key with another payload is 409
// invalid_idempotent_request; a key whose first request is still running is 409
// concurrent_idempotent_requests. Faults: accept and then time out, delay
// acceptance, or answer 5xx without accepting. Accepted messages go to Mailpit (or
// to `sent` in tests).

import { createHash, randomUUID } from "node:crypto";

export type ResendFault =
  | { kind: "timeout"; hangMs: number }
  | { kind: "delay"; ms: number }
  | { kind: "5xx" };

export type SentEmail = {
  id: string;
  key: string;
  from: string;
  to: string[];
  subject: string;
  text: string;
  html: string;
  tags: Array<{ name: string; value: string }>;
};

type KeyState = {
  payloadHash: string;
  state: "running" | "done";
  response?: { id: string };
  at: number;
};

const keyTtlMs = 24 * 60 * 60 * 1000;

export function createResend(options: { apiKey: string; now: () => number; mailpitUrl?: string }) {
  const keys = new Map<string, KeyState>();
  const sent: SentEmail[] = [];
  const faults: ResendFault[] = [];
  let requests = 0;

  const json = (status: number, body: unknown) =>
    new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json" } });
  const error = (status: number, name: string, message: string) =>
    json(status, { statusCode: status, name, message });

  async function forward(email: SentEmail) {
    sent.push(email);
    if (!options.mailpitUrl) return;
    const m = email.from.match(/^\s*(.*?)\s*<([^>]+)>\s*$/);
    await fetch(`${options.mailpitUrl}/api/v1/send`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        From: m ? { Name: m[1], Email: m[2] } : { Email: email.from },
        To: email.to.map((Email) => ({ Email })),
        Subject: email.subject,
        Text: email.text,
        HTML: email.html,
        Tags: email.tags.map((t) => `${t.name}:${t.value}`),
      }),
    }).catch(() => {});
  }

  async function handle(req: Request): Promise<Response> {
    requests++;
    if (req.headers.get("authorization") !== `Bearer ${options.apiKey}`)
      return error(401, "missing_api_key", "Missing or wrong API key");
    const raw = await req.text();
    let body: {
      from?: string;
      to?: string[] | string;
      subject?: string;
      text?: string;
      html?: string;
      tags?: SentEmail["tags"];
    };
    try {
      body = JSON.parse(raw);
    } catch {
      return error(422, "validation_error", "Invalid JSON");
    }
    const to = Array.isArray(body.to) ? body.to : body.to ? [body.to] : [];
    if (!body.from || to.length === 0 || !body.subject)
      return error(422, "validation_error", "from, to and subject are required");
    if (to.some((a) => !/^[^@\s]+@[^@\s]+\.[^@\s]+$/.test(a)))
      return error(422, "validation_error", "Invalid `to` field");

    const fault = faults.shift();
    if (fault?.kind === "5xx") return error(500, "internal_server_error", "mock fault");

    const key = req.headers.get("idempotency-key");
    const payloadHash = createHash("sha256").update(raw).digest("hex");
    const now = options.now();
    for (const [k, v] of keys) if (now - v.at > keyTtlMs) keys.delete(k);
    if (key) {
      if (key.length > 256) return error(422, "validation_error", "Idempotency key too long");
      const seen = keys.get(key);
      if (seen) {
        if (seen.payloadHash !== payloadHash)
          return error(409, "invalid_idempotent_request", "Same key, different payload");
        if (seen.state === "running")
          return error(409, "concurrent_idempotent_requests", "Original request still running");
        return json(200, seen.response);
      }
      keys.set(key, { payloadHash, state: "running", at: now });
    }
    if (fault?.kind === "delay") await Bun.sleep(fault.ms);
    const id = randomUUID();
    await forward({
      id,
      key: key ?? "",
      from: body.from,
      to,
      subject: body.subject,
      text: body.text ?? "",
      html: body.html ?? "",
      tags: body.tags ?? [],
    });
    const response = { id };
    if (key) keys.set(key, { payloadHash, state: "done", response, at: now });
    if (fault?.kind === "timeout") await Bun.sleep(fault.hangMs);
    return json(200, response);
  }

  return {
    handle,
    sent,
    keys,
    addFault: (f: ResendFault) => faults.push(f),
    clearFaults: () => faults.splice(0),
    get requests() {
      return requests;
    },
  };
}

export type ResendMock = ReturnType<typeof createResend>;
