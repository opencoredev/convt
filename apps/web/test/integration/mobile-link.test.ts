// The mobile download-link limits against Postgres, connected as convt_web like a
// Worker request: the real send buckets, a concurrent double tap, and the release
// after a failed send. Run through packages/db/scripts/test-db.sh
// (`bun run --cwd apps/web test:integration`).

import { afterAll, beforeAll, expect, test } from "bun:test";

import { consumeSendBucket, releaseSendBucket, type Db } from "@convt/db";
import { freshDatabase, type TestDatabase } from "@convt/db/testing";
import { sql } from "drizzle-orm";

import type { MailMessage } from "../../src/server/mail";
import {
  mobileLinkLimits,
  requestMobileLink,
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
    send,
    siteUrl: "http://localhost:3999",
    now,
  };
}

test("concurrent taps send one email; the limit holds across windows", async () => {
  const sent: MailMessage[] = [];
  const send = async (m: MailMessage) => void sent.push(m);
  let now = new Date("2026-10-07T12:00:00Z");
  const input = { email: "tap@convt.test", ip: "198.51.100.1" };
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
  const input = { email: "retry@convt.test", ip: "198.51.100.2" };
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
  expect(await requestMobileLink(input, deps(now, send))).toEqual({ ok: true });
  expect(sent).toHaveLength(1);
});
