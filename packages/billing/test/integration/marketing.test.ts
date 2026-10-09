// Campaign email: enrollment, consent changes, the push to Sequenzy (the mock's
// subscriber routes through the real client), preferences links, Sequenzy's
// unsubscribe webhook, and account deletion. Nothing here reaches Sequenzy.

import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { applyMarketingBackfill, planMarketingBackfill } from "@convt/db/queries";
import { signSequenzyWebhook } from "@convt/mail";
import { sql } from "drizzle-orm";

import { checkPreferencesToken, preferencesToken } from "../../src/marketing";
import { createHarness, testMailbox, type Harness } from "../../src/testing";

let h: Harness;
beforeAll(async () => {
  h = await createHarness();
});
afterAll(async () => h?.close());

type Row = {
  status: string;
  source: string;
  sync_state: string;
  reactivate: boolean;
  sync_attempts: number;
  last_error: string | null;
};
const row = async (userId: string) =>
  (
    await h.q<Row>(
      sql`select status, source, sync_state, reactivate, sync_attempts, last_error from marketing_subscriptions where user_id = ${userId}`,
    )
  )[0];
const events = async (userId: string) =>
  (
    await h.q<{ status: string; source: string; detail: string | null }>(
      sql`select status, source, detail from marketing_consent_events where user_id = ${userId} order by id`,
    )
  ).map((e) => `${e.status}/${e.source}${e.detail ? ` ${e.detail}` : ""}`);
const contact = (userId: string) => h.mock.sequenzy.contacts.get(userId);

/** An account from before marketing email: no row, no events. */
async function legacyUser(local: string, verified = true) {
  const u = await h.user(testMailbox(local), verified);
  await h.q(sql`delete from marketing_consent_events where user_id = ${u.id}`);
  await h.q(sql`delete from marketing_subscriptions where user_id = ${u.id}`);
  return u;
}

async function webhook(body: unknown, secret = h.marketing.webhookSecret) {
  const raw = new TextEncoder().encode(JSON.stringify(body));
  const ts = Math.floor(Date.now() / 1000);
  const headers = new Headers({
    "x-sequenzy-timestamp": String(ts),
    "x-sequenzy-signature": `v1=${await signSequenzyWebhook(secret, ts, raw)}`,
  });
  return h.service.handleMarketingWebhook("POST", raw, headers);
}

describe("marketing email", () => {
  test("a new account is subscribed at signup and pushed with a preferences link", async () => {
    const u = await h.user(testMailbox("mkt-new"));
    expect(await row(u.id)).toMatchObject({
      status: "subscribed",
      source: "signup",
      sync_state: "pending",
    });
    expect(await events(u.id)).toEqual(["subscribed/signup"]);
    await h.service.syncMarketing();
    expect(await row(u.id)).toMatchObject({ sync_state: "synced", sync_attempts: 0 });
    const c = contact(u.id)!;
    expect(c.status).toBe("active");
    expect(c.tags).toEqual(["convt-account"]);
    // A signup gets no createdAt, so Sequenzy may enroll it in the welcome sequence.
    expect(c.createdAt).toBeNull();
    const link = new URL(String(c.attributes.preferencesUrl));
    expect(link.pathname).toBe("/email/preferences");
    expect(
      await checkPreferencesToken(h.marketing.linkSecret, link.searchParams.get("t")!, {
        userId: u.id,
        email: u.email,
      }),
    ).toBe(true);
    expect(c.attributes).toMatchObject({ desktopBuyer: false, proStatus: "none" });
  });

  test("an unverified address is held until it is verified", async () => {
    const u = await h.user(testMailbox("mkt-unverified"), false);
    await h.service.syncMarketing();
    expect((await row(u.id)).sync_state).toBe("held");
    expect(contact(u.id)).toBeUndefined();
    await h.q(sql`update users set email_verified = true where id = ${u.id}`);
    expect((await row(u.id)).sync_state).toBe("pending");
    await h.service.syncMarketing();
    expect(contact(u.id)?.status).toBe("active");
  });

  test("the backfill enrolls only accounts without a row, with their signup date", async () => {
    const a = await legacyUser("mkt-legacy-a");
    const b = await legacyUser("mkt-legacy-b", false);
    const opted = await legacyUser("mkt-legacy-out");
    await h.service.withCtx((c) =>
      c.db.execute(sql`select set_marketing_consent(${opted.id}, false, 'settings', null, null)`),
    );
    const plan = await planMarketingBackfill(h.owner);
    const ids = plan.missing.map((m) => m.userId);
    expect(ids).toContain(a.id);
    expect(ids).toContain(b.id);
    expect(ids).not.toContain(opted.id);
    expect(plan.missing.find((m) => m.userId === b.id)?.verified).toBe(false);
    expect(await row(a.id)).toBeUndefined();

    const applied = await applyMarketingBackfill(h.owner, { resync: false });
    expect(applied.enrolled).toBeGreaterThanOrEqual(2);
    expect(await row(a.id)).toMatchObject({ status: "subscribed", source: "backfill" });
    expect((await row(opted.id)).status).toBe("unsubscribed");
    // A second run changes nothing.
    expect((await applyMarketingBackfill(h.owner, { resync: false })).enrolled).toBe(0);

    await h.service.syncMarketing();
    expect(contact(a.id)?.createdAt).not.toBeNull();
    expect(contact(b.id)).toBeUndefined();
    expect(contact(opted.id)).toBeUndefined();
  });

  test("a purchase refreshes the segment attributes", async () => {
    const u = await h.user(testMailbox("mkt-buyer"));
    await h.service.syncMarketing();
    await h.buy("desktop", u);
    await h.deliverAll();
    expect((await row(u.id)).sync_state).toBe("pending");
    await h.service.syncMarketing();
    expect(contact(u.id)?.attributes.desktopBuyer).toBe(true);
  });

  test("unsubscribing and subscribing again in Settings; only the latter reactivates", async () => {
    const u = await h.user(testMailbox("mkt-settings"));
    await h.service.syncMarketing();
    expect(await h.service.setMarketingPreference(u.id, false)).toEqual({ subscribed: false });
    // Repeating it is a no-op: no second event.
    await h.service.setMarketingPreference(u.id, false);
    await h.service.syncMarketing();
    expect(contact(u.id)?.status).toBe("unsubscribed");

    // An email change pushes the new address but never resubscribes.
    await h.q(sql`update users set email = ${testMailbox("mkt-settings-2")} where id = ${u.id}`);
    await h.service.syncMarketing();
    expect(contact(u.id)?.status).toBe("unsubscribed");

    expect(await h.service.setMarketingPreference(u.id, true)).toEqual({ subscribed: true });
    expect((await row(u.id)).reactivate).toBe(true);
    await h.service.syncMarketing();
    expect(contact(u.id)?.status).toBe("active");
    expect(await row(u.id)).toMatchObject({ reactivate: false, sync_state: "synced" });
    expect(await events(u.id)).toEqual([
      "subscribed/signup",
      "unsubscribed/settings",
      "subscribed/settings",
    ]);
  });

  test("a preferences link reads and changes only its own account", async () => {
    const u = await h.user(testMailbox("mkt-link"));
    const token = await preferencesToken(h.marketing.linkSecret, u.id, u.email);
    expect(await h.service.preferenceByToken(token)).toEqual({
      subscribed: true,
      maskedEmail: "m***@convt.test",
    });
    expect(await h.service.setPreferenceByToken(token, false)).toMatchObject({ subscribed: false });
    expect(await events(u.id)).toEqual(["subscribed/signup", "unsubscribed/email_link"]);

    const other = await h.user(testMailbox("mkt-link-other"));
    const forged = `${other.id}.${token.split(".")[1]}`;
    expect(await h.service.preferenceByToken(forged)).toBeNull();
    expect(await h.service.setPreferenceByToken(forged, false)).toBeNull();
    expect(await h.service.preferenceByToken(`${token}x`)).toBeNull();
    expect(await h.service.preferenceByToken("usr_nothing.short")).toBeNull();
    expect((await row(other.id)).status).toBe("subscribed");

    // After an email change the old address's link no longer works.
    await h.q(sql`update users set email = ${testMailbox("mkt-link-new")} where id = ${u.id}`);
    expect(await h.service.preferenceByToken(token)).toBeNull();
    const fresh = await preferencesToken(h.marketing.linkSecret, u.id, testMailbox("mkt-link-new"));
    expect(await h.service.preferenceByToken(fresh)).toMatchObject({ subscribed: false });
  });

  test("a late or repeated Sequenzy unsubscribe does not undo a later resubscribe", async () => {
    const u = await h.user(testMailbox("mkt-late"));
    await h.service.syncMarketing();
    const pause = () => new Promise((r) => setTimeout(r, 20));
    await pause();
    const unsubscribedAt = new Date();
    const hook = {
      id: "evt_late",
      type: "subscriber.unsubscribed",
      created_at: unsubscribedAt.toISOString(),
      data: { external_id: u.id },
    };
    await webhook(hook);
    expect((await row(u.id)).status).toBe("unsubscribed");
    await pause();
    await h.service.setMarketingPreference(u.id, true);
    // The same delivery again, and a delayed one from before the resubscribe.
    await webhook(hook);
    await webhook({
      ...hook,
      id: "evt_late_2",
      created_at: new Date(unsubscribedAt.getTime() + 5).toISOString(),
    });
    expect((await row(u.id)).status).toBe("subscribed");
    // An unsubscribe that happened after the resubscribe still applies.
    await pause();
    await webhook({ ...hook, id: "evt_new", created_at: new Date().toISOString() });
    expect((await row(u.id)).status).toBe("unsubscribed");
  });

  test("a name change is pushed, and an unsubscribed contact keeps a current address", async () => {
    const u = await h.user(testMailbox("mkt-name"));
    await h.service.syncMarketing();
    await h.q(sql`update users set name = 'Alex Example' where id = ${u.id}`);
    expect((await row(u.id)).sync_state).toBe("pending");
    await h.service.syncMarketing();
    expect(contact(u.id)?.firstName).toBe("Alex");

    await h.service.setMarketingPreference(u.id, false);
    await h.service.syncMarketing();
    await h.q(sql`update users set email = ${testMailbox("mkt-name-2")} where id = ${u.id}`);
    await h.service.syncMarketing();
    expect(contact(u.id)).toMatchObject({
      status: "unsubscribed",
      email: testMailbox("mkt-name-2"),
    });
  });

  test("a complimentary Desktop order is not a buyer; --resync includes delayed retries", async () => {
    const u = await h.user(testMailbox("mkt-comp"));
    await h.buy("desktop", u);
    await h.deliverAll();
    await h.q(sql`update orders set amount_cents = 0 where user_id = ${u.id}`);
    await h.q(sql`update marketing_subscriptions set next_sync_at = now() + interval '6 hours',
      sync_state = 'pending' where user_id = ${u.id}`);
    await applyMarketingBackfill(h.owner, { resync: true });
    await h.service.syncMarketing();
    expect(contact(u.id)?.attributes.desktopBuyer).toBe(false);
  });

  test("Sequenzy's unsubscribe, complaint and bounce webhooks unsubscribe; bad ones are refused", async () => {
    const byId = await h.user(testMailbox("mkt-hook-id"));
    const byEmail = await h.user(testMailbox("mkt-hook-email"));
    await h.service.syncMarketing();

    expect(
      await webhook({
        id: "evt_t1",
        type: "subscriber.unsubscribed",
        data: { external_id: byId.id },
      }),
    ).toEqual({ status: 200, body: "ok" });
    expect(await row(byId.id)).toMatchObject({
      status: "unsubscribed",
      source: "provider",
      // Pushed back too, so an older push that lands late cannot leave Sequenzy active.
      sync_state: "pending",
    });
    expect(await events(byId.id)).toEqual([
      "subscribed/signup",
      "unsubscribed/provider subscriber.unsubscribed evt_t1",
    ]);
    // A retry of the same delivery changes nothing.
    await webhook({
      id: "evt_t1",
      type: "subscriber.unsubscribed",
      data: { external_id: byId.id },
    });
    expect((await events(byId.id)).length).toBe(2);

    await webhook({ id: "evt_t2", type: "email.complained", data: { recipient: byEmail.email } });
    expect((await row(byEmail.id)).status).toBe("unsubscribed");

    const before = await h.counts();
    expect(
      await webhook(
        { id: "evt_t3", type: "email.unsubscribed", data: { external_id: byId.id } },
        "wrong",
      ),
    ).toEqual({ status: 401, body: "signature" });
    expect(
      await webhook({ id: "evt_t4", type: "email.opened", data: { external_id: byId.id } }),
    ).toEqual({ status: 200, body: "ignored" });
    expect(
      await webhook({
        id: "evt_t5",
        type: "email.bounced",
        data: { recipient: testMailbox("nobody") },
      }),
    ).toEqual({ status: 200, body: "no account" });
    expect(await h.counts()).toEqual(before);
  });

  test("a change while a push is in flight is pushed next, in order", async () => {
    const u = await h.user(testMailbox("mkt-race"));
    await h.service.syncMarketing();
    await h.service.setMarketingPreference(u.id, false);
    await h.service.syncMarketing();
    await h.service.setMarketingPreference(u.id, true);
    const realUpdate = h.marketing.contacts.update;
    // The person unsubscribes again while the reactivating push is on its way.
    h.marketing.contacts.update = async (input) => {
      const result = await realUpdate(input);
      await h.service.setMarketingPreference(u.id, false);
      // A second run meanwhile finds the row leased and leaves it alone.
      expect((await h.service.syncMarketing()).synced).toBe(0);
      return result;
    };
    try {
      await h.service.syncMarketing();
    } finally {
      h.marketing.contacts.update = realUpdate;
    }
    // The stale push's outcome was not recorded; the same run claimed the row
    // again and pushed the newer unsubscribe after it.
    expect(contact(u.id)?.status).toBe("unsubscribed");
    const [r] = await h.q<{ sync_state: string; lease: Date | null }>(
      sql`select sync_state, sync_lease_until as lease from marketing_subscriptions where user_id = ${u.id}`,
    );
    expect(r).toEqual({ sync_state: "synced", lease: null });
  });

  test("an account being deleted is not pushed", async () => {
    const u = await h.user(testMailbox("mkt-deleting"));
    await h.q(sql`insert into account_deletions (id, user_id, status, next_attempt_at, created_at, updated_at)
      values (${`del_test_${u.id}`}, ${u.id}, 'pending', now() + interval '1 day', now(), now())`);
    await h.service.syncMarketing();
    expect(contact(u.id)).toBeUndefined();
    expect((await row(u.id)).sync_state).toBe("pending");
  });

  test("retryable failures back off and alert; a refusal fails the row and alerts", async () => {
    const u = await h.user(testMailbox("mkt-flaky"));
    const realUpdate = h.marketing.contacts.update;
    h.marketing.contacts.update = async () => ({
      kind: "retry",
      status: 503,
      code: "http_503",
      retryAfterMs: null,
    });
    try {
      await h.service.syncMarketing();
      expect(await row(u.id)).toMatchObject({
        sync_state: "pending",
        sync_attempts: 1,
        last_error: "503 http_503",
      });
      // Not due again for a minute.
      expect((await h.service.syncMarketing()).retry).toBe(0);
    } finally {
      h.marketing.contacts.update = realUpdate;
    }
    const refuse = await h.user(testMailbox("mkt-refused"));
    h.marketing.contacts.update = async () => ({ kind: "refused", status: 409, code: "CONFLICT" });
    try {
      await h.service.syncMarketing();
    } finally {
      h.marketing.contacts.update = realUpdate;
    }
    expect(await row(refuse.id)).toMatchObject({
      sync_state: "failed",
      last_error: "409 CONFLICT",
    });
    const alerts = await h.q<{ subject: string; detail: string }>(
      sql`select subject, detail from billing_alerts where kind = 'marketing_sync'`,
    );
    expect(alerts).toContainEqual({ subject: refuse.id, detail: "refused: 409 CONFLICT" });
    // Alerts carry ids and codes, never addresses.
    expect(JSON.stringify(alerts)).not.toContain("@");
  });

  test("account deletion removes the Sequenzy contact before the account", async () => {
    const u = await h.user(testMailbox("mkt-delete"));
    await h.service.syncMarketing();
    expect(contact(u.id)).toBeDefined();
    // A push still holding the row makes deletion wait instead of racing it.
    await h.q(
      sql`update marketing_subscriptions set sync_lease_until = now() + interval '1 minute' where user_id = ${u.id}`,
    );
    const waiting = await h.service.requestDeletion(u.id);
    expect(await h.service.advanceDeletion(waiting.id)).toBe("failed");
    expect(contact(u.id)).toBeDefined();
    await h.q(
      sql`update marketing_subscriptions set sync_lease_until = null where user_id = ${u.id}`,
    );
    const realRemove = h.marketing.contacts.remove;
    h.marketing.contacts.remove = async () => ({
      kind: "retry",
      status: null,
      code: "timeout",
      retryAfterMs: null,
    });
    const d = await h.service.requestDeletion(u.id);
    expect(d.id).toBe(waiting.id);
    try {
      expect(await h.service.advanceDeletion(d.id)).toBe("failed");
    } finally {
      h.marketing.contacts.remove = realRemove;
    }
    expect((await h.q(sql`select 1 from users where id = ${u.id}`)).length).toBe(1);
    expect(await h.service.advanceDeletion(d.id)).toBe("done");
    expect(contact(u.id)).toBeUndefined();
    expect(
      (await h.q(sql`select 1 from marketing_consent_events where user_id = ${u.id}`)).length,
    ).toBe(0);
  });

  test("the database refuses consent changes outside the functions", async () => {
    const u = await h.user(testMailbox("mkt-guard"));
    await h.service.withCtx(async (c) => {
      // Drizzle wraps the Postgres error; its cause carries the message.
      const run = async (query: ReturnType<typeof sql>) => {
        try {
          await c.db.execute(query);
        } catch (e) {
          throw e instanceof Error && e.cause instanceof Error ? e.cause : e;
        }
      };
      await expect(
        run(
          sql`update marketing_subscriptions set status = 'unsubscribed' where user_id = ${u.id}`,
        ),
      ).rejects.toThrow("permission denied");
      await expect(
        run(sql`select set_marketing_consent(${u.id}, true, 'provider', null, null)`),
      ).rejects.toThrow("never subscribes");
      await expect(run(sql`select enroll_marketing(${u.id}, 'backfill')`)).rejects.toThrow(
        "permission denied",
      );
    });
    const rewrite = h.q(
      sql`update marketing_consent_events set source = 'settings' where user_id = ${u.id}`,
    );
    await expect(rewrite.catch((e: Error) => Promise.reject(e.cause ?? e))).rejects.toThrow(
      "append-only",
    );
  });
});
