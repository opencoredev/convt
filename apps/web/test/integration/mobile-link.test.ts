// The mobile download-link limits against Postgres, connected as convt_web like a
// Worker request: the real send buckets, a concurrent double tap, and the release
// after a failed send. Run through packages/db/scripts/test-db.sh
// (`bun run --cwd apps/web test:integration`).

import { afterAll, beforeAll, expect, test } from "bun:test";

import {
  consumeSendBucket,
  joinLaunchList,
  leaveLaunchList,
  releaseSendBucket,
  type Db,
} from "@convt/db";
import { freshDatabase, type TestDatabase } from "@convt/db/testing";
import { sql } from "drizzle-orm";

import type { MailMessage } from "../../src/server/mail";
import {
  hashUnsubscribeToken,
  mobileLinkLimits,
  requestMobileLink,
  unsubscribeToken,
  type MobileLinkDeps,
} from "../../src/server/mobile-link";

let tdb: TestDatabase;
let web: Db;
let owner: Db;

beforeAll(async () => {
  tdb = await freshDatabase();
  web = (await tdb.open("web")).db;
  owner = (await tdb.open("owner")).db;
});
afterAll(async () => tdb?.drop());

function deps(now: Date, send: (m: MailMessage) => Promise<void>): MobileLinkDeps {
  return {
    consume: (key, windowMs) => consumeSendBucket(web, key, windowMs, now),
    release: (key) => releaseSendBucket(web, key, now),
    join: (entry) => joinLaunchList(web, { ...entry, now }),
    send,
    siteUrl: "http://localhost:3999",
    unsubscribeSecret: "test-secret",
    now,
  };
}

test("concurrent taps send one email; the limit holds across windows", async () => {
  const sent: MailMessage[] = [];
  const send = async (m: MailMessage) => void sent.push(m);
  let now = new Date("2026-10-07T12:00:00Z");
  const input = { email: "tap@convt.test", ip: "198.51.100.1", source: "landing" as const };
  // Separate connections, so the two requests really race in Postgres.
  const other = (await tdb.open("web")).db;
  const results = await Promise.all([
    requestMobileLink(input, deps(now, send)),
    requestMobileLink(input, {
      ...deps(now, send),
      consume: (key, windowMs) => consumeSendBucket(other, key, windowMs, now),
    }),
  ]);
  expect(results).toEqual([{ ok: true }, { ok: true }]);
  expect(sent).toHaveLength(1);
  expect(sent[0].text).toContain("http://localhost:3999/download");

  for (let i = 1; i < mobileLinkLimits.email.max; i++) {
    now = new Date(now.getTime() + mobileLinkLimits.duplicateWindowMs);
    expect(await requestMobileLink(input, deps(now, send))).toEqual({ ok: true });
  }
  now = new Date(now.getTime() + mobileLinkLimits.duplicateWindowMs);
  expect(await requestMobileLink(input, deps(now, send))).toEqual({
    ok: false,
    error: "too_many",
  });
  expect(sent).toHaveLength(mobileLinkLimits.email.max);

  const keys = await owner.execute<{ key: string }>(sql`select key from otp_send_limits`);
  for (const { key } of keys.rows) expect(key).not.toContain("tap@");
});

test("a failed send releases the double-tap guard, so a retry sends", async () => {
  const sent: MailMessage[] = [];
  let fail = true;
  const send = async (m: MailMessage) => {
    if (fail) {
      fail = false;
      throw new Error("mailpit answered 500");
    }
    sent.push(m);
  };
  const now = new Date("2026-10-07T13:00:00Z");
  const input = { email: "retry@convt.test", ip: "198.51.100.2", source: "download" as const };
  const quiet = console.error;
  console.error = () => {};
  try {
    expect(await requestMobileLink(input, deps(now, send))).toEqual({
      ok: false,
      error: "send_failed",
    });
  } finally {
    console.error = quiet;
  }
  const listed = async () =>
    (await owner.execute(sql`select 1 from launch_list where email = 'retry@convt.test'`)).rows
      .length;
  expect(await listed()).toBe(0);
  expect(await requestMobileLink(input, deps(now, send))).toEqual({ ok: true });
  expect(sent).toHaveLength(1);
  expect(await listed()).toBe(1);
});

const listRows = async () =>
  (
    await owner.execute<{
      email: string;
      source: string;
      consented_at: Date;
      last_requested_at: Date;
    }>(sql`select email, source, consented_at, last_requested_at from launch_list order by email`)
  ).rows;

test("joining keeps one row per address and the first consent; unsubscribing deletes it", async () => {
  const sent: MailMessage[] = [];
  const send = async (m: MailMessage) => void sent.push(m);
  const first = new Date("2026-10-07T14:00:00Z");
  const later = new Date("2026-10-07T15:30:00Z");
  await requestMobileLink(
    { email: "List@Convt.test", ip: "198.51.100.3", source: "landing" },
    deps(first, send),
  );
  await requestMobileLink(
    { email: "list@convt.test", ip: "198.51.100.3", source: "checkout_success" },
    deps(later, send),
  );
  const rows = (await listRows()).filter((r) => r.email === "list@convt.test");
  expect(rows).toHaveLength(1);
  expect(rows[0].source).toBe("checkout_success");
  expect(new Date(rows[0].consented_at).toISOString()).toBe(first.toISOString());
  expect(new Date(rows[0].last_requested_at).toISOString()).toBe(later.toISOString());
  expect(sent).toHaveLength(2);

  // The link in the first email still works: the token is stable per address.
  const token = await unsubscribeToken("test-secret", "list@convt.test");
  expect(sent[0].text).toContain(`#t=${token}`);
  const hash = await hashUnsubscribeToken(token);
  expect(await leaveLaunchList(web, hash)).toBe(true);
  expect((await listRows()).some((r) => r.email === "list@convt.test")).toBe(false);
  expect(await leaveLaunchList(web, hash)).toBe(false);
});

test("the table refuses an address that is not normalized or an unknown source", async () => {
  const now = new Date();
  await expect(
    joinLaunchList(web, {
      email: "Upper@convt.test",
      source: "landing",
      unsubscribeTokenHash: "a",
      now,
    }),
  ).rejects.toThrow();
  await expect(
    joinLaunchList(web, {
      email: "ok@convt.test",
      source: "elsewhere" as "landing",
      unsubscribeTokenHash: "b",
      now,
    }),
  ).rejects.toThrow();
});
