// Mail transports. `resend` is Resend's POST /emails with an Idempotency-Key; its
// base URL points at the local billing mock in development and tests, so the same
// code path runs everywhere. `mailpit` posts straight to a local Mailpit and `log`
// prints one line; both are for development only.

export type OutgoingEmail = {
  from: string;
  to: string;
  subject: string;
  text: string;
  html: string;
  /** Resend tags; the outbox puts its row id here so a message can be found later. */
  tags?: Array<{ name: string; value: string }>;
};

export type SendResult =
  | { ok: true; id: string }
  | {
      ok: false;
      /** `retry`: safe to try again with the same key. `dead`: never retry. */
      outcome: "retry" | "dead";
      /** True when Resend may have accepted the message (timeout, network error, 5xx). */
      unknown: boolean;
      status: number | null;
      code: string;
    };

export type MailTransport = {
  name: "resend" | "mailpit" | "log";
  send(email: OutgoingEmail, idempotencyKey: string): Promise<SendResult>;
};

export type ResendOptions = {
  apiKey: string;
  /** `https://api.resend.com` in production, the billing mock locally. */
  baseUrl: string;
  timeoutMs?: number;
  fetch?: typeof fetch;
};

export function resendTransport(options: ResendOptions): MailTransport {
  const doFetch = options.fetch ?? fetch;
  const base = options.baseUrl.replace(/\/$/, "");
  return {
    name: "resend",
    async send(email, idempotencyKey) {
      let res: Response;
      try {
        res = await doFetch(`${base}/emails`, {
          method: "POST",
          headers: {
            authorization: `Bearer ${options.apiKey}`,
            "content-type": "application/json",
            "idempotency-key": idempotencyKey,
          },
          body: JSON.stringify({
            from: email.from,
            to: [email.to],
            subject: email.subject,
            text: email.text,
            html: email.html,
            tags: email.tags,
          }),
          signal: AbortSignal.timeout(options.timeoutMs ?? 10_000),
        });
      } catch (e) {
        const timeout =
          e instanceof Error && (e.name === "TimeoutError" || e.name === "AbortError");
        return {
          ok: false,
          outcome: "retry",
          unknown: true,
          status: null,
          code: timeout ? "timeout" : "network_error",
        };
      }
      let body: { id?: string; name?: string } = {};
      try {
        body = (await res.json()) as typeof body;
      } catch {
        // A non-JSON answer is treated by status alone.
      }
      if (res.ok && typeof body.id === "string") return { ok: true, id: body.id };
      const code = typeof body.name === "string" ? body.name : `http_${res.status}`;
      if (res.status >= 500 || res.ok)
        return { ok: false, outcome: "retry", unknown: true, status: res.status, code };
      if (res.status === 429 || code === "concurrent_idempotent_requests")
        return { ok: false, outcome: "retry", unknown: false, status: res.status, code };
      // invalid_idempotent_request, validation and address errors: never retried.
      return { ok: false, outcome: "dead", unknown: false, status: res.status, code };
    },
  };
}

function parseAddress(text: string): { Email: string; Name?: string } {
  const m = text.match(/^\s*(.*?)\s*<([^>]+)>\s*$/);
  return m ? { Name: m[1] || undefined, Email: m[2] } : { Email: text.trim() };
}

export function mailpitTransport(url: string, doFetch: typeof fetch = fetch): MailTransport {
  return {
    name: "mailpit",
    async send(email) {
      const res = await doFetch(`${url.replace(/\/$/, "")}/api/v1/send`, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          From: parseAddress(email.from),
          To: [{ Email: email.to }],
          Subject: email.subject,
          Text: email.text,
          HTML: email.html,
          Tags: email.tags?.map((t) => `${t.name}:${t.value}`),
        }),
      });
      if (!res.ok)
        return { ok: false, outcome: "retry", unknown: false, status: res.status, code: "mailpit" };
      const body = (await res.json()) as { ID?: string };
      return { ok: true, id: body.ID ?? "mailpit" };
    },
  };
}

export function logTransport(log: (line: string) => void = console.info): MailTransport {
  return {
    name: "log",
    async send(email, idempotencyKey) {
      // Never the body: it can hold a license key.
      log(`[mail] ${idempotencyKey} to ${email.to.replace(/^[^@]*/, "***")}: ${email.subject}`);
      return { ok: true, id: `log_${idempotencyKey}` };
    },
  };
}
