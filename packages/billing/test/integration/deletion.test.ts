// Account deletion: subscriptions end first, then delete_user; every failure mode
// resumes on the next run.

import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { sql } from "drizzle-orm";

import { createHarness, type Harness } from "../../src/testing";

let h: Harness;
beforeAll(async () => {
  h = await createHarness();
});
afterAll(async () => h?.close());

async function withBoth(email: string) {
  const u = await h.user(email);
  await h.buy("desktop", u);
  await h.buy("pro_month", u);
  await h.buy("api", u, { cap: 2000 });
  await h.deliverAll();
  return u;
}

const status = async (id: string) =>
  (
    await h.q<{ status: string; attempts: number; last_error: string | null }>(
      sql`select status, attempts, last_error from account_deletions where id = ${id}`,
    )
  )[0];

describe("account deletion", () => {
  test("Pro usage never delays API revocation, but pending usage for that API subscription does", async () => {
    const u = await withBoth("delete-usage@convt.test");
    const subs = await h.q<{ id: string; kind: string }>(
      sql`select id, kind from subscriptions where user_id = ${u.id}`,
    );
    const pro = subs.find((s) => s.kind === "pro")!.id;
    const api = subs.find((s) => s.kind === "api")!.id;
    await h.q(sql`insert into usage_events (id,user_id,subscription_id,job_id,kind,quantity,amount_cents,occurred_at) values
      ('delete-pro-usage',${u.id},${pro},'delete-pro-job','pro_bytes',100,0,now()),
      ('delete-api-usage',${u.id},${api},'delete-api-job','api_conversion',1,1,now())`);
    const d = await h.service.requestDeletion(u.id);
    expect(await h.service.advanceDeletion(d.id)).toBe("failed");
    expect((await status(d.id)).last_error).toContain("waiting for API usage");
    await h.q(
      sql`update usage_events set reported_at = now(), provider_event_id = job_id where id = 'delete-api-usage'`,
    );
    expect(await h.service.advanceDeletion(d.id)).toBe("done");
    expect((await h.q(sql`select 1 from users where id = ${u.id}`)).length).toBe(0);
  });

  test("with live Pro and API subscriptions, both are revoked, then the user is deleted; financial rows keep the email", async () => {
    const u = await withBoth("delete1@convt.test");
    const d = await h.service.requestDeletion(u.id);
    // Repeating the request returns the same row.
    expect((await h.service.requestDeletion(u.id)).id).toBe(d.id);
    expect(await h.service.advanceDeletion(d.id)).toBe("done");
    await h.deliverAll();
    expect((await h.q(sql`select 1 from users where id = ${u.id}`)).length).toBe(0);
    const subs = await h.q<{ status: string; user_id: string | null; email: string }>(
      sql`select status, user_id, email from subscriptions where email = 'delete1@convt.test'`,
    );
    expect(subs.length).toBe(2);
    expect(
      subs.every(
        (s) => s.status === "canceled" && s.user_id === null && s.email === "delete1@convt.test",
      ),
    ).toBe(true);
    const orders = await h.q<{ user_id: string | null }>(
      sql`select user_id from orders where email = 'delete1@convt.test'`,
    );
    expect(orders).toEqual([{ user_id: null }]);
    const lic = await h.q<{ user_id: string | null }>(
      sql`select user_id from licenses where email = 'delete1@convt.test'`,
    );
    expect(lic.length).toBeGreaterThan(0);
    expect(
      h.mock
        .state()
        .subscriptions.filter((s) => s.customer.email === "delete1@convt.test")
        .every((s) => s.status === "canceled"),
    ).toBe(true);
    expect((await status(d.id)).status).toBe("done");
  });

  test("granted API credit ends with the account, without the provider", async () => {
    const u = await h.user("delete-grant@convt.test");
    await h.q(sql`insert into subscriptions (id, provider, provider_subscription_id, user_id, email, kind, status,
      current_period_start, spend_cap_cents, card_seen_at)
      values ('sub_delete_grant', 'grant', 'grant_sub_delete_grant', ${u.id}, 'delete-grant@convt.test', 'api', 'active',
      now(), 2500, now())`);
    const d = await h.service.requestDeletion(u.id);
    expect(await h.service.advanceDeletion(d.id)).toBe("done");
    expect((await h.q(sql`select 1 from users where id = ${u.id}`)).length).toBe(0);
    const [grant] = await h.q<{ status: string; ended_at: Date | null }>(
      sql`select status, ended_at from subscriptions where id = 'sub_delete_grant'`,
    );
    expect(grant.status).toBe("canceled");
    expect(grant.ended_at).not.toBeNull();
  });

  test("a provider failure retries and alerts after 24 hours", async () => {
    const u = await withBoth("delete2@convt.test");
    const d = await h.service.requestDeletion(u.id);
    h.mock.failApi("DELETE", /^\/v1\/subscriptions\//, 500, 100);
    expect(await h.service.advanceDeletion(d.id)).toBe("failed");
    let s = await status(d.id);
    expect(s.attempts).toBe(1);
    expect(s.last_error).toContain("500");
    expect((await h.q(sql`select 1 from users where id = ${u.id}`)).length).toBe(1);
    h.mock.advance(25 * 3600_000);
    await h.service.runDeletions();
    expect(
      (
        await h.q(
          sql`select 1 from billing_alerts where kind = 'deletion_stuck' and subject = ${d.id}`,
        )
      ).length,
    ).toBe(1);
    h.mock.clearApiFaults();
    h.mock.advance(3600_000);
    await h.service.runDeletions();
    s = await status(d.id);
    expect(s.status).toBe("done");
  });

  test("a partial cancellation (one of two) resumes on the next run", async () => {
    const u = await withBoth("delete3@convt.test");
    const d = await h.service.requestDeletion(u.id);
    h.failAt("after-revoke");
    expect(await h.service.advanceDeletion(d.id)).toBe("failed");
    const live = await h.q(
      sql`select 1 from subscriptions where user_id = ${u.id} and status <> 'canceled'`,
    );
    expect(live.length).toBe(1);
    h.clearFaults();
    h.mock.advance(2 * 60_000);
    await h.service.runDeletions();
    expect((await status(d.id)).status).toBe("done");
  });

  test("a crash before delete_user resumes", async () => {
    const u = await withBoth("delete4@convt.test");
    const d = await h.service.requestDeletion(u.id);
    h.failAt("before-delete-user");
    expect(await h.service.advanceDeletion(d.id)).toBe("deleting");
    expect((await status(d.id)).status).toBe("deleting");
    expect((await h.q(sql`select 1 from users where id = ${u.id}`)).length).toBe(1);
    h.clearFaults();
    h.mock.advance(2 * 60_000);
    await h.service.runDeletions();
    expect((await status(d.id)).status).toBe("done");
    expect((await h.q(sql`select 1 from users where id = ${u.id}`)).length).toBe(0);
  });

  test("an already-ended subscription counts as success after a fetch", async () => {
    const u = await withBoth("delete5@convt.test");
    // The provider ended one already, but we never heard.
    const pro = (
      await h.q<{ provider_subscription_id: string }>(
        sql`select provider_subscription_id from subscriptions where user_id = ${u.id} and kind = 'pro'`,
      )
    )[0];
    await h.provider.revokeSubscription(pro.provider_subscription_id);
    h.mock.takeHeld();
    const d = await h.service.requestDeletion(u.id);
    expect(await h.service.advanceDeletion(d.id)).toBe("done");
  });

  test("a subscription whose webhooks have not arrived is found through its checkout and ended", async () => {
    const u = await h.user("inflight@convt.test");
    await h.buy("pro_month", u);
    h.mock.takeHeld(); // the webhooks are late
    const d = await h.service.requestDeletion(u.id);
    // No new purchases while the deletion is open.
    expect(await h.service.createCheckout({ product: "desktop", user: u })).toEqual({
      ok: false,
      refusal: "deleting",
    });
    expect(await h.service.advanceDeletion(d.id)).toBe("done");
    const subs = h.mock
      .state()
      .subscriptions.filter((s) => s.customer.email === "inflight@convt.test");
    expect(subs.length).toBe(1);
    expect(subs[0].status).toBe("canceled");
  });

  test("a checkout whose provider id was never stored is still found and ended", async () => {
    const u = await h.user("unstored@convt.test");
    h.failAt("before-checkout-store");
    await expect(h.service.createCheckout({ product: "pro_month", user: u })).rejects.toThrow(
      "injected fault",
    );
    h.clearFaults();
    const co = h.mock.lastCheckout()!;
    const d = await h.service.requestDeletion(u.id);
    // While the provider checkout is open, deletion waits.
    expect(await h.service.advanceDeletion(d.id)).toBe("failed");
    h.mock.completeCheckout(co.id, "4242", u.email);
    h.mock.takeHeld();
    h.mock.advance(2 * 60_000);
    await h.service.runDeletions();
    expect(
      h.mock
        .state()
        .subscriptions.filter((s) => s.customer.email === "unstored@convt.test")
        .map((s) => s.status),
    ).toEqual(["canceled"]);
    expect((await h.q(sql`select 1 from users where id = ${u.id}`)).length).toBe(0);
  });

  test("pending emails other than keys are skipped for a deleted user", async () => {
    const u = await h.user("delete6@convt.test");
    await h.buy("pro_month", u);
    await h.deliverAll();
    h.mock.advance(6 * 86_400_000);
    const { trialEndingScan } = await import("../../src/reconcile");
    await h.service.withCtx((c) => trialEndingScan(c));
    const d = await h.service.requestDeletion(u.id);
    expect(await h.service.advanceDeletion(d.id)).toBe("done");
    const [r] = await h.q<{ status: string }>(
      sql`select status from email_outbox where to_email = 'delete6@convt.test' and kind = 'trial_ending'`,
    );
    expect(r.status).toBe("skipped");
  });
});
