// The outbox: one trial_ending per trial, frozen payloads, backoff, dead and
// skipped rows, the 23-hour ambiguity rule, error redaction and retention.

import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { sequenzyTransport } from "@convt/mail";
import { sql } from "drizzle-orm";

import { drainOutbox, outboxRetention, resolveOutbox, safeError } from "../../src/outbox";
import { trialEndingScan } from "../../src/reconcile";
import { createHarness, type Harness } from "../../src/testing";

let h: Harness;
beforeAll(async () => {
  h = await createHarness();
});
afterAll(async () => h?.close());

async function row(to: string) {
  const [r] = await h.q<{
    id: string;
    status: string;
    attempts: number;
    next_attempt_at: string;
    last_error: string | null;
    payload: unknown;
    payload_sha256: string | null;
    template_version: number | null;
  }>(
    sql`select id, status, attempts, next_attempt_at::text, last_error, payload, payload_sha256, template_version from email_outbox where to_email = ${to}`,
  );
  return r;
}

async function desktopEmail(email: string) {
  await h.service.drainOutbox();
  await h.buy("desktop", null, { email });
  // Only this purchase's events (not renewals the mock clock may have started).
  const held = h.mock.takeHeld().filter((d) => d.body.includes(email));
  for (const d of held) await h.deliver(d);
}

describe("outbox", () => {
  test("trial_ending is queued once per trial and sent once", async () => {
    const u = await h.user("ending@convt.test");
    await h.buy("pro_month", u);
    await h.deliverAll();
    h.mock.advance(6 * 86_400_000); // inside the last 48 hours
    await h.service.withCtx((c) => trialEndingScan(c));
    await h.service.withCtx((c) => trialEndingScan(c));
    await h.service.drainOutbox();
    await h.service.withCtx((c) => trialEndingScan(c));
    await h.service.drainOutbox();
    const mails = h.mock.resend.sent.filter(
      (m) => m.to[0] === "ending@convt.test" && m.subject.includes("trial ends"),
    );
    expect(mails.length).toBe(1);
    expect(mails[0].text).toContain("$12.00 a month");
  });

  test("a 5xx retries with backoff; the retry sends the frozen bytes under the same key", async () => {
    await desktopEmail("backoff@convt.test");
    h.mock.resend.addFault({ kind: "5xx" });
    await h.service.drainOutbox();
    let r = await row("backoff@convt.test");
    expect(r.status).toBe("pending");
    expect(r.attempts).toBe(1);
    expect(r.payload_sha256).not.toBeNull();
    expect(r.template_version).toBe(3);
    const due = Date.parse(r.next_attempt_at.replace(" ", "T").replace(/([+-]\d{2})$/, "$1:00"));
    expect(Math.round((due - h.mock.now().getTime()) / 60_000)).toBe(1);
    // Not due yet: nothing is claimed.
    expect((await h.service.drainOutbox()).claimed).toBe(0);
    h.mock.advance(61_000);
    await h.service.drainOutbox();
    r = await row("backoff@convt.test");
    expect(r.status).toBe("sent");
    // The stored request is what was sent: re-sending other bytes under that key is refused.
    const key = r.id;
    const res = await fetch(`${h.base}/emails`, {
      method: "POST",
      headers: {
        authorization: "Bearer re_test",
        "content-type": "application/json",
        "idempotency-key": key,
      },
      body: JSON.stringify({
        from: "convt <hello@convt.test>",
        to: ["backoff@convt.test"],
        subject: "a new template",
        text: "x",
      }),
    });
    expect(res.status).toBe(409);
    expect((await res.json()).name).toBe("invalid_idempotent_request");
  });

  test("a retry sends the frozen request even when a new render would differ", async () => {
    const u = await h.user("frozen@convt.test");
    await h.buy("pro_month", u);
    await h.deliverAll();
    h.mock.advance(6 * 86_400_000);
    await h.service.withCtx((c) => trialEndingScan(c));
    // Resend accepts the first request and records the key, but the answer is lost.
    h.setMailTimeout(200);
    h.mock.resend.addFault({ kind: "timeout", hangMs: 1000 });
    await h.service.drainOutbox();
    h.setMailTimeout(2000);
    const before = await row("frozen@convt.test");
    expect(before.status).toBe("pending");
    // What the template would render changes (the trial end moved); the frozen bytes do not.
    await h.owner.execute(
      sql`update subscriptions set trial_ends_at = trial_ends_at + interval '2 days' where user_id = ${u.id}`,
    );
    h.mock.advance(2 * 60_000);
    await h.service.drainOutbox();
    const after = await row("frozen@convt.test");
    // A re-render would have been refused as invalid_idempotent_request; the frozen
    // retry gets Resend's stored answer, and only one copy exists.
    expect(after.status).toBe("sent");
    expect(after.payload_sha256).toBe(before.payload_sha256);
    expect(h.mock.resend.sent.filter((m) => m.to[0] === "frozen@convt.test").length).toBe(1);
  });

  test("an address error is dead; a revoked license's email is skipped", async () => {
    const now = h.mock.now();
    await h.service.withCtx(async (c) => {
      const { enqueueEmail } = await import("../../src/outbox");
      await enqueueEmail(c.db, {
        kind: "alert_digest",
        dedupeKey: "alert_digest:bad-address",
        to: "not-an-address",
        userId: null,
        subjectId: "2026-01-01",
        now,
      });
    });
    await h.service.drainOutbox();
    expect((await row("not-an-address")).status).toBe("dead");
    // A key refunded before its email went out.
    await h.buy("desktop", null, { email: "skipme@convt.test" });
    await h.deliverAll();
    const order = h.mock.state().orders.at(-1)!;
    h.mock.refund(order.id);
    await h.deliverAll();
    await h.service.drainOutbox();
    expect((await row("skipme@convt.test")).status).toBe("skipped");
    expect(h.mock.resend.sent.some((m) => m.to[0] === "skipme@convt.test")).toBe(false);
  });

  test("the 23-hour rule: an unknown outcome followed by failures becomes ambiguous, never a second copy", async () => {
    await desktopEmail("ambiguous@convt.test");
    h.setMailTimeout(200);
    // Resend accepts the first request but the answer never arrives in time.
    h.mock.resend.addFault({ kind: "timeout", hangMs: 1000 });
    await h.service.drainOutbox();
    let r = await row("ambiguous@convt.test");
    expect(r.status).toBe("pending");
    expect(r.last_error).toContain("timeout");
    // Every later attempt fails with a 5xx, until the next one would land past 23 hours.
    for (let i = 0; i < 8 && r.status === "pending"; i++) {
      h.mock.resend.addFault({ kind: "5xx" });
      h.mock.advance(13 * 3600_000);
      await h.service.drainOutbox();
      r = await row("ambiguous@convt.test");
    }
    h.setMailTimeout(2000);
    h.mock.resend.clearFaults();
    expect(r.status).toBe("ambiguous");
    // Exactly one copy went out (the one whose answer was lost).
    expect(h.mock.resend.sent.filter((m) => m.to[0] === "ambiguous@convt.test").length).toBe(1);
    // Why: past 24 hours Resend has forgotten the key, so a retry would send a second copy.
    h.mock.advance(25 * 3600_000);
    const replay = await fetch(`${h.base}/emails`, {
      method: "POST",
      headers: {
        authorization: "Bearer re_test",
        "content-type": "application/json",
        "idempotency-key": r.id,
      },
      body: JSON.stringify(r.payload),
    });
    expect(replay.status).toBe(200);
    expect(h.mock.resend.sent.filter((m) => m.to[0] === "ambiguous@convt.test").length).toBe(2);
    // Leo resolves it: `sent` records his decision.
    const resolved = await h.service.withCtx((c) => resolveOutbox(c, r.id, "sent"));
    expect(resolved.status).toBe("sent");
  });

  test("a crash after Resend accepted, then a retry past 23 hours, is ambiguous, never a second copy", async () => {
    // Only this row may be due when the crash is injected: earlier clock moves
    // renewed other tests' subscriptions in the mock, so drop their events.
    for (let i = 0; i < 10 && (await h.service.drainOutbox()).claimed > 0; i++);
    h.mock.takeHeld();
    await desktopEmail("crashlate@convt.test");
    h.failAt("after-send-before-mark");
    await expect(h.service.drainOutbox()).rejects.toThrow("injected fault");
    h.clearFaults();
    // The row is left `sending`; nobody reclaims it until a day later.
    h.mock.advance(25 * 3600_000);
    await h.service.drainOutbox();
    expect((await row("crashlate@convt.test")).status).toBe("ambiguous");
    expect(h.mock.resend.sent.filter((m) => m.to[0] === "crashlate@convt.test").length).toBe(1);
  });

  test("last_error holds no address or token", async () => {
    const e = safeError(
      422,
      "validation_error",
      "Invalid `to`: someone@example.com with key eyJpZCI6ImxpY18xMjM0NTY3ODkwIn0.c2lnbmF0dXJlc2lnbmF0dXJl",
    );
    expect(e).not.toContain("someone@example.com");
    expect(e).not.toContain("eyJpZCI6");
    expect(e.length).toBeLessThanOrEqual(200);
    const rows = await h.q<{ last_error: string }>(
      sql`select last_error from email_outbox where last_error is not null`,
    );
    expect(rows.length).toBeGreaterThan(0);
    for (const r of rows) {
      expect(r.last_error).not.toMatch(/@/);
      expect(r.last_error).not.toMatch(/eyJ/);
    }
  });

  test("payloads are nulled 25 hours after a final status", async () => {
    await desktopEmail("retention@convt.test");
    await h.service.drainOutbox();
    expect((await row("retention@convt.test")).payload).not.toBeNull();
    h.mock.advance(24 * 3600_000);
    await h.service.withCtx((c) => outboxRetention(c));
    expect((await row("retention@convt.test")).payload).not.toBeNull();
    h.mock.advance(2 * 3600_000);
    await h.service.withCtx((c) => outboxRetention(c));
    expect((await row("retention@convt.test")).payload).toBeNull();
  });
});

test("Sequenzy outbox honors Retry-After and sends frozen HTML with the row key", async () => {
  await desktopEmail("sequenzy@convt.test");
  const before = await row("sequenzy@convt.test");
  const requests: string[] = [];
  const mail = sequenzyTransport({
    apiKey: "sq_fake",
    fetch: Object.assign(
      async (_input: Parameters<typeof fetch>[0], init?: RequestInit) => {
        expect(new Headers(init?.headers).get("idempotency-key")).toBe(before.id);
        requests.push(String(init?.body));
        if (requests.length === 1)
          return Response.json(
            { success: false, retryable: true },
            { status: 429, headers: { "Retry-After": "180" } },
          );
        return Response.json({ success: true, emailSendId: "seq_send_1", jobId: "seq_job_1" });
      },
      { preconnect: fetch.preconnect },
    ),
  });
  const drain = () => h.service.withCtx((c) => drainOutbox({ ...c, mail }));
  expect((await drain()).retried).toBe(1);
  const pending = await row("sequenzy@convt.test");
  expect(pending.status).toBe("pending");
  const [stored] = await h.q<{ delay: number; unknown_outcome_at: Date | null }>(
    sql`select extract(epoch from (next_attempt_at - last_attempt_at))::int as delay, unknown_outcome_at from email_outbox where id = ${before.id}`,
  );
  expect(stored.delay).toBe(180);
  expect(stored.unknown_outcome_at).toBeNull();
  h.mock.advance(179000);
  expect((await drain()).claimed).toBe(0);
  h.mock.advance(2000);
  expect((await drain()).sent).toBe(1);
  expect(requests[0]).toBe(requests[1]);
  const [sent] = await h.q<{ status: string; provider_message_id: string }>(
    sql`select status, provider_message_id from email_outbox where id = ${before.id}`,
  );
  expect(sent).toEqual({ status: "sent", provider_message_id: "seq_send_1" });
});
