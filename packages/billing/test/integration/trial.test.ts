// The desktop trial and the key the app keeps (CNV-56), as convt_billing: one trial
// per account and per computer, race-safe, kept through account deletion; and
// currentKey's choice between paid keys and a Pro subscription's trial.

import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { sql } from "drizzle-orm";

import { importVerifyKey, verify, type License } from "@convt/license";

import { createHarness, type Harness } from "../../src/testing";
import { currentKey, startTrial } from "../../src/trial";

let h: Harness;
let vk: CryptoKey;
beforeAll(async () => {
  h = await createHarness({ startMs: Date.UTC(2026, 9, 7, 15, 30) });
  vk = await importVerifyKey(h.publicKey);
});
afterAll(async () => h?.close());

/** A test mailbox, put together at run time so no address sits in the source. */
const fixture = (name: string) => [name, "convt.test"].join("@");

const dayMs = 24 * 60 * 60 * 1000;
const hash = () =>
  [...crypto.getRandomValues(new Uint8Array(32))]
    .map((b) => b.toString(16).padStart(2, "0"))
    .join("");

const start = (userId: string, deviceHash: string, now = h.mock.now()) =>
  h.service.withCtx((c) => startTrial(c, { userId, deviceHash, now }));
const keyFor = (userId: string, now = h.mock.now()) =>
  h.service.withCtx((c) => currentKey(c, { userId, now }));

async function open(token: string): Promise<License> {
  const r = await verify(token, vk);
  if (!r.ok) throw new Error(`token does not verify: ${r.error}`);
  return r.license;
}

const trialRows = (userId: string) =>
  h.q<{ id: string; device_hash: string | null }>(
    sql`select id, device_hash from trials where user_id = ${userId}`,
  );

describe("startTrial", () => {
  test("an account gets one trial: started today, through today + 6, the same on every call", async () => {
    const u = await h.user(fixture("trial-once"));
    const device = hash();
    const first = await start(u.id, device);
    if (!first.ok) throw new Error("refused");
    const lic = await open(first.key);
    expect(lic).toMatchObject({
      email: fixture("trial-once"),
      plan: "trial",
      issued: "2026-10-07",
      updates_until: "2026-10-13",
    });
    expect(lic.id).toStartWith("trl_");
    expect(first.endsAt.toISOString()).toBe("2026-10-14T00:00:00.000Z");

    // Again from another computer, a day later: the same trial and token.
    const again = await start(u.id, hash(), new Date(h.mock.now().getTime() + dayMs));
    expect(again).toEqual(first);
    const rows = await trialRows(u.id);
    expect(rows).toEqual([{ id: lic.id, device_hash: device }]);
  });

  test("a computer that started one account's trial cannot start another's", async () => {
    const a = await h.user(fixture("trial-dev-a"));
    const b = await h.user(fixture("trial-dev-b"));
    const device = hash();
    expect((await start(a.id, device)).ok).toBe(true);
    expect(await start(b.id, device)).toEqual({ ok: false, reason: "device_used" });
    expect(await trialRows(b.id)).toEqual([]);
    // The account can still start one from a computer that never had a trial.
    expect((await start(b.id, hash())).ok).toBe(true);
  });

  test("a computer that got an account's trial back counts as used too", async () => {
    const a = await h.user(fixture("trial-roam-a"));
    const b = await h.user(fixture("trial-roam-b"));
    const x = hash();
    const y = hash();
    expect((await start(a.id, x)).ok).toBe(true);
    // The site records y on A's device on computer Y before asking billing.
    await h.q(sql`
      insert into devices (id, user_id, name, os, device_hash, created_at, updated_at)
      values (${"dev_" + y.slice(0, 26)}, ${a.id}, 'Y', 'Linux', ${y}, now(), now())`);
    expect((await start(a.id, y)).ok).toBe(true);
    expect(await start(b.id, y)).toEqual({ ok: false, reason: "device_used" });
    expect(await trialRows(b.id)).toEqual([]);
  });

  test("an expired trial still comes back, with its past last day", async () => {
    const u = await h.user(fixture("trial-expired"));
    const device = hash();
    const first = await start(u.id, device);
    const later = await start(u.id, device, new Date(h.mock.now().getTime() + 40 * dayMs));
    expect(later).toEqual(first);
    if (!later.ok) throw new Error("refused");
    expect((await open(later.key)).updates_until).toBe("2026-10-13");
  });

  test("concurrent starts create one trial, per account and per computer", async () => {
    const u = await h.user(fixture("trial-race"));
    const results = await Promise.all(Array.from({ length: 6 }, () => start(u.id, hash())));
    expect(results.every((r) => r.ok)).toBe(true);
    expect(new Set(results.map((r) => (r.ok ? r.key : ""))).size).toBe(1);
    expect((await trialRows(u.id)).length).toBe(1);

    const users = await Promise.all(
      Array.from({ length: 6 }, (_, i) => h.user(fixture(`trial-race-${i}`))),
    );
    const device = hash();
    const byDevice = await Promise.all(users.map((x) => start(x.id, device)));
    expect(byDevice.filter((r) => r.ok).length).toBe(1);
    expect(byDevice.filter((r) => !r.ok && r.reason === "device_used").length).toBe(5);
    const [n] = await h.q<{ n: number }>(
      sql`select count(*)::int as n from trials where device_hash = ${device}`,
    );
    expect(n.n).toBe(1);
  });

  test("a malformed device hash is refused before any write", async () => {
    const u = await h.user(fixture("trial-bad"));
    await expect(start(u.id, "ABC")).rejects.toThrow(/device hash/);
    await expect(start(u.id, hash().toUpperCase())).rejects.toThrow(/device hash/);
    expect(await trialRows(u.id)).toEqual([]);
  });

  test("deleting the account keeps the computer's trial, without the account", async () => {
    const u = await h.user(fixture("trial-deleted"));
    const device = hash();
    expect((await start(u.id, device)).ok).toBe(true);
    const d = await h.service.requestDeletion(u.id);
    expect(await h.service.advanceDeletion(d.id)).toBe("done");
    expect((await h.q(sql`select 1 from users where id = ${u.id}`)).length).toBe(0);
    const [row] = await h.q<{ user_id: string | null }>(
      sql`select user_id from trials where device_hash = ${device}`,
    );
    expect(row).toEqual({ user_id: null });
    // Signing up again on that computer does not start a second trial.
    const again = await h.user(fixture("trial-deleted"));
    expect(await start(again.id, device)).toEqual({ ok: false, reason: "device_used" });
  });
});

describe("currentKey", () => {
  const subOf = async (userId: string) =>
    (
      await h.q<{ id: string; provider_subscription_id: string }>(
        sql`select id, provider_subscription_id from subscriptions where user_id = ${userId} and kind = 'pro'`,
      )
    )[0];

  test("nothing bought and no Pro subscription: null, even with a desktop trial", async () => {
    const u = await h.user(fixture("key-none"));
    expect((await start(u.id, hash())).ok).toBe(true);
    expect(await keyFor(u.id)).toBeNull();
  });

  test("a Desktop purchase returns its Desktop key", async () => {
    const u = await h.user(fixture("key-desktop"));
    await h.buy("desktop", u);
    await h.deliverAll();
    const [lic] = await h.q<{ token: string; updates_until: string }>(
      sql`select token, updates_until::text from licenses where user_id = ${u.id}`,
    );
    expect(await keyFor(u.id)).toEqual({ key: lic.token, updatesUntil: lic.updates_until });
    expect((await open(lic.token)).plan).toBe("desktop");
  });

  test("a Pro subscription in its trial gets a trial token through the trial's end, capped at 31 days", async () => {
    const u = await h.user(fixture("key-protrial"));
    await h.buy("pro_month", u);
    await h.deliverAll();
    const s = await subOf(u.id);
    const [row] = await h.q<{ status: string; trial_end: string }>(
      sql`select status, ((trial_ends_at - interval '1 millisecond') at time zone 'UTC')::date::text as trial_end from subscriptions where id = ${s.id}`,
    );
    expect(row.status).toBe("trialing");
    const got = await keyFor(u.id);
    if (!got) throw new Error("no key");
    const lic = await open(got.key);
    expect(lic).toMatchObject({
      id: s.id,
      email: fixture("key-protrial"),
      plan: "trial",
      issued: "2026-10-07",
      updates_until: row.trial_end,
    });
    expect(got.updatesUntil).toBe(row.trial_end);

    // A trial end far away is capped at today + 31 days.
    await h.q(
      sql`update subscriptions set trial_ends_at = now() + interval '400 days' where id = ${s.id}`,
    );
    expect((await keyFor(u.id))?.updatesUntil).toBe("2026-11-07");
    // No trial end recorded: the period end stands in.
    await h.q(
      sql`update subscriptions set trial_ends_at = null, current_period_end = '2026-10-20T12:00:00Z' where id = ${s.id}`,
    );
    // It ends mid-day, so the key works through that day.
    expect((await keyFor(u.id))?.updatesUntil).toBe("2026-10-20");
    // An end at midnight UTC keeps the day before it.
    await h.q(
      sql`update subscriptions set current_period_end = '2026-10-21T00:00:00Z' where id = ${s.id}`,
    );
    expect((await keyFor(u.id))?.updatesUntil).toBe("2026-10-20");
    // A trial ending later today still works today.
    await h.q(
      sql`update subscriptions set current_period_end = '2026-10-07T20:00:00Z' where id = ${s.id}`,
    );
    expect((await keyFor(u.id))?.updatesUntil).toBe("2026-10-07");
    // One that ended this morning gives no key.
    await h.q(
      sql`update subscriptions set current_period_end = '2026-10-07T06:00:00Z' where id = ${s.id}`,
    );
    expect(await keyFor(u.id)).toBeNull();
  });

  test("an outgrown Desktop key doesn't hide a Pro trial; a current one wins", async () => {
    const u = await h.user(fixture("key-oldpaid"));
    await h.buy("desktop", u);
    await h.deliverAll();
    await h.buy("pro_month", u);
    await h.deliverAll();
    // The Desktop key still covers today's builds: it beats the Pro trial.
    expect((await open((await keyFor(u.id))!.key)).plan).toBe("desktop");
    // Its update window ended before today: the Pro trial runs newer builds.
    await h.q(
      sql`update licenses set updates_until = '2026-09-01' where user_id = ${u.id} and plan = 'desktop'`,
    );
    expect((await open((await keyFor(u.id))!.key)).plan).toBe("trial");
    // Without the Pro trial, the old key still comes back for older builds.
    await h.q(
      sql`update subscriptions set status = 'canceled' where user_id = ${u.id} and kind = 'pro'`,
    );
    expect(await keyFor(u.id)).toMatchObject({ updatesUntil: "2026-09-01" });
  });

  test("a paid Pro key, and with a Desktop key too, the one covering more", async () => {
    const u = await h.user(fixture("key-both"));
    await h.buy("pro_month", u);
    await h.deliverAll();
    const s = await subOf(u.id);
    h.mock.cycle(s.provider_subscription_id); // the trial converts and is paid
    await h.deliverAll();
    const pro = await keyFor(u.id);
    if (!pro) throw new Error("no key");
    expect((await open(pro.key)).plan).toBe("pro");

    await h.buy("desktop", u);
    await h.deliverAll();
    const [desktop] = await h.q<{ token: string; updates_until: string }>(
      sql`select token, updates_until::text from licenses where user_id = ${u.id} and plan = 'desktop'`,
    );
    // A year of Desktop updates outlasts a month of Pro.
    expect(desktop.updates_until > pro.updatesUntil).toBe(true);
    expect(await keyFor(u.id)).toEqual({ key: desktop.token, updatesUntil: desktop.updates_until });
  });

  test("a revoked key is never returned", async () => {
    const u = await h.user(fixture("key-revoked"));
    await h.buy("desktop", u);
    await h.deliverAll();
    await h.q(
      sql`update licenses set revoked_at = now(), revoke_reason = 'refunded' where user_id = ${u.id}`,
    );
    expect(await keyFor(u.id)).toBeNull();
  });
});
