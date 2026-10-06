import type { MailTransport } from "./transport";

export type SequenzyOptions = {
  apiKey: string;
  timeoutMs?: number;
  fetch?: typeof fetch;
};

/** Sequenzy accepts mail asynchronously and replays keys for 14 days. */
export function sequenzyTransport(options: SequenzyOptions): MailTransport {
  const doFetch = options.fetch ?? fetch;
  return {
    name: "sequenzy",
    async send(email, idempotencyKey) {
      if (!idempotencyKey || idempotencyKey.length > 255)
        return {
          ok: false,
          outcome: "dead",
          unknown: false,
          status: null,
          code: "invalid_idempotency_key",
        };
      let res: Response;
      try {
        res = await doFetch("https://api.sequenzy.com/api/v1/transactional/send", {
          method: "POST",
          headers: {
            authorization: `Bearer ${options.apiKey}`,
            "content-type": "application/json",
            "idempotency-key": idempotencyKey,
          },
          body: JSON.stringify({
            from: email.from,
            to: email.to,
            subject: email.subject,
            body: email.html,
            emailType: "transactional",
            trackingSettings: { clickTracking: false, openTracking: false },
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
      let body: unknown;
      try {
        body = await res.json();
      } catch {
        // Malformed success responses may still mean the provider accepted mail.
      }
      const object = typeof body === "object" && body !== null ? body : {};
      if (
        res.ok &&
        "success" in object &&
        object.success === true &&
        "emailSendId" in object &&
        typeof object.emailSendId === "string" &&
        object.emailSendId !== ""
      )
        return { ok: true, id: object.emailSendId };
      const code =
        "code" in object && typeof object.code === "string" ? object.code : `http_${res.status}`;
      // Match Resend's conservative treatment of 5xx and malformed successes.
      if (res.status >= 500 || res.ok)
        return { ok: false, outcome: "retry", unknown: true, status: res.status, code };
      if (res.status === 429) {
        const retryAfter = res.headers.get("retry-after");
        const seconds = retryAfter === null ? NaN : Number(retryAfter);
        const delay = Number.isFinite(seconds)
          ? seconds * 1000
          : Date.parse(retryAfter ?? "") - Date.now();
        return {
          ok: false,
          outcome: "retry",
          unknown: false,
          status: res.status,
          code,
          ...(Number.isFinite(delay) && delay >= 0 ? { retryAfterMs: delay } : {}),
        };
      }
      return { ok: false, outcome: "dead", unknown: false, status: res.status, code };
    },
  };
}
