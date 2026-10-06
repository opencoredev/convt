// Auth integration tests: Better Auth in process against a disposable Postgres,
// connected as convt_web. Run through packages/db/scripts/test-db.sh
// (`bun run --cwd apps/web test:integration`, or `bun run db:ci`).

import { afterAll, beforeAll, describe, expect, test } from "bun:test";

import { fixtureId, runSeed } from "@convt/db/seed";
import * as t from "@convt/db/schema";
import { importSigningKey } from "@convt/license";
import { and, eq, isNull, sql } from "drizzle-orm";
import pg from "pg";

import { Jar, oauth, signInWithCode, startHarness, type Harness } from "./harness";

let h: Harness;
let ipCounter = 10;
/** A fresh client IP per test, so per-IP limits from one test never touch another. */
const nextIp = () => `198.51.100.${ipCounter++}`;

beforeAll(async () => {
  h = await startHarness();
  await runSeed(
    h.owner,
    new Date(),
    await importSigningKey(crypto.getRandomValues(new Uint8Array(32))),
  );
});
afterAll(async () => h?.close());

const userByEmail = async (email: string) =>
  (await h.owner.select().from(t.users).where(eq(t.users.email, email)))[0] ?? null;
const sessionsOf = async (userId: string) =>
  h.owner.select().from(t.sessions).where(eq(t.sessions.userId, userId));
const accountsOf = async (userId: string) =>
  h.owner.select().from(t.accounts).where(eq(t.accounts.userId, userId));
const mailsTo = (email: string) => h.mail.filter((m) => m.to === email).length;
const json = async (res: Response) =>
  (await res.json().catch(() => ({}))) as Record<string, unknown>;
const ageSessions = (userId: string, minutes: number) =>
  h.owner.execute(
    sql`update sessions set created_at = now() - make_interval(mins => ${minutes}) where user_id = ${userId}`,
  );

describe("sign-in codes", () => {
  test("a code is mailed, works once, and signs in", async () => {
    const ip = nextIp();
    const email = "codes-once@convt.test";
    const jar = new Jar();
    expect(
      (
        await h.request("/email-otp/send-verification-otp", {
          body: { email, type: "sign-in" },
          ip,
        })
      ).status,
    ).toBe(200);
    const code = h.codeFor(email);
    const message = h.mail.at(-1)!;
    expect(message.text).toContain(
      `/sign-in/verify#email=${encodeURIComponent(email)}&code=${code}`,
    );
    const ok = await h.request("/sign-in/email-otp", { body: { email, otp: code }, jar, ip });
    expect(ok.status).toBe(200);
    expect(jar.session).toBeTruthy();
    const again = await h.request("/sign-in/email-otp", {
      body: { email, otp: code },
      jar: new Jar(),
      ip,
    });
    expect(again.status).toBe(400);
    expect((await userByEmail(email))?.emailVerified).toBe(true);
  });

  test("an expired code is rejected", async () => {
    const ip = nextIp();
    const email = "codes-expired@convt.test";
    await h.request("/email-otp/send-verification-otp", { body: { email, type: "sign-in" }, ip });
    await h.owner.execute(
      sql`update verifications set expires_at = now() - interval '1 second' where identifier = ${`sign-in-otp-${email}`}`,
    );
    const res = await h.request("/sign-in/email-otp", {
      body: { email, otp: h.codeFor(email) },
      ip,
    });
    expect(res.status).toBe(400);
    expect((await json(res)).code).toBe("OTP_EXPIRED");
  });

  test("three wrong tries lock the code", async () => {
    const ip = nextIp();
    const email = "codes-locked@convt.test";
    await h.request("/email-otp/send-verification-otp", { body: { email, type: "sign-in" }, ip });
    const code = h.codeFor(email);
    const wrong = code === "000000" ? "111111" : "000000";
    for (let i = 0; i < 3; i++) {
      expect(
        (await h.request("/sign-in/email-otp", { body: { email, otp: wrong }, ip })).status,
      ).toBe(400);
    }
    const res = await h.request("/sign-in/email-otp", { body: { email, otp: code }, ip });
    expect(res.status).toBe(403);
    expect((await json(res)).code).toBe("TOO_MANY_ATTEMPTS");
  });

  test("a resend replaces the previous code", async () => {
    const ip = nextIp();
    const email = "codes-resend@convt.test";
    await h.request("/email-otp/send-verification-otp", { body: { email, type: "sign-in" }, ip });
    const first = h.codeFor(email);
    await h.request("/email-otp/send-verification-otp", { body: { email, type: "sign-in" }, ip });
    const second = h.codeFor(email);
    if (first !== second) {
      expect(
        (await h.request("/sign-in/email-otp", { body: { email, otp: first }, ip })).status,
      ).toBe(400);
    }
    expect(
      (await h.request("/sign-in/email-otp", { body: { email, otp: second }, ip })).status,
    ).toBe(200);
    const rows = await h.owner
      .select()
      .from(t.verifications)
      .where(eq(t.verifications.identifier, `sign-in-otp-${email}`));
    expect(rows.length).toBe(0);
  });

  test("a wrong guess cannot bring back a code that a resend replaced", async () => {
    const ip = nextIp();
    const email = "codes-race@convt.test";
    const identifier = `sign-in-otp-${email}`;
    await h.request("/email-otp/send-verification-otp", { body: { email, type: "sign-in" }, ip });
    const first = h.codeFor(email);
    const [a] = await h.owner
      .select()
      .from(t.verifications)
      .where(eq(t.verifications.identifier, identifier));
    // The plugin's wrong-guess path consumes the row, then writes it back with one
    // more attempt. A resend lands in between.
    await h.owner.delete(t.verifications).where(eq(t.verifications.identifier, identifier));
    await h.request("/email-otp/send-verification-otp", { body: { email, type: "sign-in" }, ip });
    const second = h.codeFor(email);
    await h.withAuth(async (auth) =>
      (await auth.$context).internalAdapter.createVerificationValue({
        identifier,
        value: `${a.value.split(":")[0]}:1`,
        expiresAt: a.expiresAt,
      }),
    );
    const [row] = await h.owner
      .select()
      .from(t.verifications)
      .where(eq(t.verifications.identifier, identifier));
    expect(row.value.endsWith(":0")).toBe(true);
    if (first !== second) {
      expect(
        (await h.request("/sign-in/email-otp", { body: { email, otp: first }, ip })).status,
      ).toBe(400);
    }
    expect(
      (await h.request("/sign-in/email-otp", { body: { email, otp: second }, ip })).status,
    ).toBe(200);
    // A write-back that arrives after the newer code was used finds an empty slot,
    // and still must not bring the old code back.
    await h.withAuth(async (auth) =>
      (await auth.$context).internalAdapter.createVerificationValue({
        identifier,
        value: `${a.value.split(":")[0]}:1`,
        expiresAt: a.expiresAt,
      }),
    );
    const left = await h.owner
      .select()
      .from(t.verifications)
      .where(eq(t.verifications.identifier, identifier));
    expect(left.length).toBe(0);
    if (first !== second) {
      expect(
        (await h.request("/sign-in/email-otp", { body: { email, otp: first }, ip })).status,
      ).toBe(400);
    }
  });

  const writeBack = (identifier: string, value: string, expiresAt: Date) =>
    h.withAuth(async (auth) =>
      (await auth.$context).internalAdapter.createVerificationValue({
        identifier,
        value,
        expiresAt,
      }),
    );
  const codeRows = (identifier: string) =>
    h.owner.select().from(t.verifications).where(eq(t.verifications.identifier, identifier));

  test("a later send that repeats the same six digits is still a different code", async () => {
    const email = "codes-repeat@convt.test";
    const identifier = `sign-in-otp-${email}`;
    await h.request("/email-otp/send-verification-otp", {
      body: { email, type: "sign-in" },
      ip: nextIp(),
    });
    const [a] = await codeRows(identifier);
    const hash = a.value.split(":")[0];
    await h.owner.delete(t.verifications).where(eq(t.verifications.identifier, identifier));
    // A second issuance of the same code, then used.
    await writeBack(identifier, `${hash}:0`, new Date(a.expiresAt.getTime() + 5_000));
    await h.owner.delete(t.verifications).where(eq(t.verifications.identifier, identifier));
    await writeBack(identifier, `${hash}:1`, a.expiresAt);
    expect((await codeRows(identifier)).length).toBe(0);
  });

  test("a repeat of the same code in the same millisecond is still a different issuance", async () => {
    const email = "codes-same-ms@convt.test";
    const identifier = `sign-in-otp-${email}`;
    await h.request("/email-otp/send-verification-otp", {
      body: { email, type: "sign-in" },
      ip: nextIp(),
    });
    const [a] = await codeRows(identifier);
    const hash = a.value.split(":")[0];
    await h.owner.delete(t.verifications).where(eq(t.verifications.identifier, identifier));
    await writeBack(identifier, `${hash}:0`, a.expiresAt);
    const [b] = await codeRows(identifier);
    expect(b.expiresAt.getTime()).toBeGreaterThan(a.expiresAt.getTime());
    await h.owner.delete(t.verifications).where(eq(t.verifications.identifier, identifier));
    await writeBack(identifier, `${hash}:1`, a.expiresAt);
    expect((await codeRows(identifier)).length).toBe(0);
  });

  test("a write-back that overlaps a resend waits for it and then writes nothing", async () => {
    const email = "codes-overlap@convt.test";
    const identifier = `sign-in-otp-${email}`;
    await h.request("/email-otp/send-verification-otp", {
      body: { email, type: "sign-in" },
      ip: nextIp(),
    });
    const [a] = await codeRows(identifier);
    await h.owner.delete(t.verifications).where(eq(t.verifications.identifier, identifier));
    // A resend holds the marker, issues code B and B is used, all before it commits.
    const resend = new pg.Client({ connectionString: h.tdb.webUrl });
    await resend.connect();
    try {
      await resend.query("begin");
      await resend.query("update verifications set value = 'other-code' where identifier = $1", [
        `otp-issued:${identifier}`,
      ]);
      let settled = false;
      const pending = writeBack(identifier, `${a.value.split(":")[0]}:1`, a.expiresAt).finally(
        () => {
          settled = true;
        },
      );
      await new Promise((r) => setTimeout(r, 300));
      expect(settled).toBe(false);
      await resend.query("commit");
      await pending;
    } finally {
      await resend.end();
    }
    expect((await codeRows(identifier)).length).toBe(0);
  });

  test("codes are stored hashed", async () => {
    const ip = nextIp();
    const email = "codes-hashed@convt.test";
    await h.request("/email-otp/send-verification-otp", { body: { email, type: "sign-in" }, ip });
    const [row] = await h.owner
      .select()
      .from(t.verifications)
      .where(eq(t.verifications.identifier, `sign-in-otp-${email}`));
    expect(row.value).not.toContain(h.codeFor(email));
  });
});

describe("send limits", () => {
  test("the fourth send in 15 minutes gets 429 and the third code still works", async () => {
    const ip = nextIp();
    const email = "limit-four@convt.test";
    for (let i = 0; i < 3; i++) {
      expect(
        (
          await h.request("/email-otp/send-verification-otp", {
            body: { email, type: "sign-in" },
            ip,
          })
        ).status,
      ).toBe(200);
    }
    const third = h.codeFor(email);
    const fourth = await h.request("/email-otp/send-verification-otp", {
      body: { email, type: "sign-in" },
      ip,
    });
    expect(fourth.status).toBe(429);
    expect(mailsTo(email)).toBe(3);
    expect(
      (await h.request("/sign-in/email-otp", { body: { email, otp: third }, ip })).status,
    ).toBe(200);
  });

  test("the limit holds across both send paths", async () => {
    const ip = nextIp();
    const jar = await signInWithCode(h, "limit-paths@convt.test", new Jar(), ip);
    const target = "limit-paths-new@convt.test";
    await h.request("/email-otp/send-verification-otp", {
      body: { email: target, type: "sign-in" },
      ip,
    });
    await h.request("/email-otp/send-verification-otp", {
      body: { email: target, type: "sign-in" },
      ip,
    });
    expect(
      (await h.request("/email-otp/request-email-change", { body: { newEmail: target }, jar, ip }))
        .status,
    ).toBe(200);
    const over = await h.request("/email-otp/request-email-change", {
      body: { newEmail: target },
      jar,
      ip,
    });
    expect(over.status).toBe(429);
    expect(mailsTo(target)).toBe(3);
  });

  test("20 concurrent sends to one email issue exactly 3 codes", async () => {
    const email = "limit-race@convt.test";
    const results = await Promise.all(
      Array.from({ length: 20 }, (_, i) =>
        h.request("/email-otp/send-verification-otp", {
          body: { email, type: "sign-in" },
          ip: `192.0.2.${i + 1}`,
        }),
      ),
    );
    const statuses = results.map((r) => r.status);
    expect(statuses.filter((s) => s === 200).length).toBe(3);
    expect(statuses.filter((s) => s === 429).length).toBe(17);
    expect(mailsTo(email)).toBe(3);
  });

  test("the window resets after it expires", async () => {
    const ip = nextIp();
    const email = "limit-reset@convt.test";
    for (let i = 0; i < 3; i++)
      await h.request("/email-otp/send-verification-otp", { body: { email, type: "sign-in" }, ip });
    expect(
      (
        await h.request("/email-otp/send-verification-otp", {
          body: { email, type: "sign-in" },
          ip,
        })
      ).status,
    ).toBe(429);
    await h.owner.execute(
      sql`update otp_send_limits set expires_at = now() - interval '1 second' where key like 'email:%' or key = ${`ip:${ip}`}`,
    );
    expect(
      (
        await h.request("/email-otp/send-verification-otp", {
          body: { email, type: "sign-in" },
          ip,
        })
      ).status,
    ).toBe(200);
  });

  test("the IP bucket limits sends across different emails", async () => {
    const ip = nextIp();
    const statuses: number[] = [];
    for (let i = 0; i < 11; i++) {
      statuses.push(
        (
          await h.request("/email-otp/send-verification-otp", {
            body: { email: `ipbucket-${i}@convt.test`, type: "sign-in" },
            ip,
          })
        ).status,
      );
    }
    expect(statuses.slice(0, 10).every((s) => s === 200)).toBe(true);
    expect(statuses[10]).toBe(429);
    expect(mailsTo("ipbucket-10@convt.test")).toBe(0);
  });

  test("code checks are rate limited per IP", async () => {
    const ip = nextIp();
    const statuses: number[] = [];
    for (let i = 0; i < 11; i++) {
      statuses.push(
        (
          await h.request("/sign-in/email-otp", {
            body: { email: `guess-${i}@convt.test`, otp: "123456" },
            ip,
          })
        ).status,
      );
    }
    expect(statuses.slice(0, 10).every((s) => s === 400)).toBe(true);
    expect(statuses[10]).toBe(429);
  });
});

describe("OAuth sign-in and the verified-email rule", () => {
  test("Gmail and Workspace identities are verified; GitHub and third-party Google are not", async () => {
    const cases: Array<["github" | "google", string, string, boolean]> = [
      ["google", "google-gmail", "gail.mock@gmail.com", true],
      ["google", "google-workspace", "walt@workspace.test", true],
      ["google", "google-thirdparty", "theo@thirdparty.test", false],
      ["google", "google-unverified", "una@unverified.test", false],
      ["github", "github-verified", "octo@github-user.test", false],
      ["github", "github-public-differs", "public@github-user.test", false],
    ];
    for (const [provider, identity, email, verified] of cases) {
      const jar = new Jar();
      const result = await oauth(h, "sign-in", provider, identity, jar);
      expect(`${identity}: ${result.location}`).toBe(`${identity}: /dashboard`);
      const user = await userByEmail(email);
      expect(`${identity}: ${user?.emailVerified}`).toBe(`${identity}: ${verified}`);
      expect(jar.session).toBeTruthy();
    }
  });

  test("a GitHub identity without an email cannot sign up", async () => {
    const result = await oauth(h, "sign-in", "github", "github-no-email", new Jar());
    expect(result.location).toContain("error=");
  });

  test("an identity whose email belongs to an existing account fails with account_not_linked and creates nothing", async () => {
    const before = await h.owner.select({ n: sql<number>`count(*)::int` }).from(t.accounts);
    const result = await oauth(h, "sign-in", "github", "github-verified", new Jar(), {
      email: "desktop@convt.test",
    });
    expect(result.location).toContain("error=account_not_linked");
    const after = await h.owner.select({ n: sql<number>`count(*)::int` }).from(t.accounts);
    expect(after[0].n).toBe(before[0].n);
    expect((await accountsOf(fixtureId("usr", "desktop"))).length).toBe(0);
  });

  test("the seeded GitHub identity signs in to pro@", async () => {
    const jar = new Jar();
    expect((await oauth(h, "sign-in", "github", "github-pro", jar)).location).toBe("/dashboard");
    const res = await h.request("/get-session", { jar });
    expect(((await json(res)).user as { email: string }).email).toBe("pro@convt.test");
  });
});

describe("claiming purchases", () => {
  const unclaimedLicense = fixtureId("lic", "unclaimed");
  const owner = async () =>
    (await h.owner.select().from(t.licenses).where(eq(t.licenses.id, unclaimedLicense)))[0].userId;
  const resetClaim = async () => {
    // These tests all mail unclaimed@, more often than one address may get codes.
    await h.owner.delete(t.otpSendLimits);
    await h.owner.execute(sql`delete from users where email = 'unclaimed@convt.test'`);
    for (const table of ["orders", "licenses", "invoices"]) {
      await h.owner.execute(
        sql`update ${sql.identifier(table)} set user_id = null where email = 'unclaimed@convt.test'`,
      );
    }
  };

  test("an unverified GitHub account with the email claims nothing, then claims after verifying by code", async () => {
    await resetClaim();
    const ip = nextIp();
    const jar = new Jar();
    await oauth(h, "sign-in", "github", "github-verified", jar, { email: "unclaimed@convt.test" });
    const user = await userByEmail("unclaimed@convt.test");
    expect(user?.emailVerified).toBe(false);
    expect(await owner()).toBeNull();
    // From the account's own session, verify-email keeps the identity.
    await h.request("/email-otp/send-verification-otp", {
      body: { email: "unclaimed@convt.test", type: "email-verification" },
      ip,
    });
    const res = await h.request("/email-otp/verify-email", {
      body: { email: "unclaimed@convt.test", otp: h.codeFor("unclaimed@convt.test") },
      jar,
      ip,
    });
    expect(res.status).toBe(200);
    expect(await owner()).toBe(user!.id);
    expect((await accountsOf(user!.id)).length).toBe(1);
  });

  test("verifying the email from outside the account is refused and claims nothing", async () => {
    await resetClaim();
    const ip = nextIp();
    const squatter = new Jar();
    await oauth(h, "sign-in", "github", "github-verified", squatter, {
      email: "unclaimed@convt.test",
    });
    const user = (await userByEmail("unclaimed@convt.test"))!;
    await h.request("/email-otp/send-verification-otp", {
      body: { email: "unclaimed@convt.test", type: "email-verification" },
      ip,
    });
    const code = h.codeFor("unclaimed@convt.test");
    // Without a session, and from another account's session.
    const outsider = await signInWithCode(h, "outsider@convt.test", new Jar(), ip);
    for (const jar of [new Jar(), outsider]) {
      const res = await h.request("/email-otp/verify-email", {
        body: { email: "unclaimed@convt.test", otp: code },
        jar,
        ip,
      });
      expect(res.status).toBe(403);
      expect((await json(res)).code).toBe("VERIFY_FROM_ACCOUNT");
    }
    expect((await userByEmail("unclaimed@convt.test"))!.emailVerified).toBe(false);
    expect(await owner()).toBeNull();
    // The mailbox owner signs in with a code instead, which removes the squatter.
    await signInWithCode(h, "unclaimed@convt.test", new Jar(), ip);
    expect((await accountsOf(user.id)).length).toBe(0);
    expect(await (await h.request("/get-session", { jar: squatter })).json()).toBeNull();
    expect(await owner()).toBe(user.id);
  });

  test("a third-party Google account with the email claims nothing", async () => {
    await resetClaim();
    await oauth(h, "sign-in", "google", "google-thirdparty", new Jar(), {
      email: "unclaimed@convt.test",
    });
    expect((await userByEmail("unclaimed@convt.test"))?.emailVerified).toBe(false);
    expect(await owner()).toBeNull();
  });

  test("a verified sign-up by code claims at once", async () => {
    await resetClaim();
    await signInWithCode(h, "unclaimed@convt.test", new Jar(), nextIp());
    expect(await owner()).toBe((await userByEmail("unclaimed@convt.test"))!.id);
  });

  test("an email change to that address claims after confirmation", async () => {
    await resetClaim();
    const ip = nextIp();
    const jar = await signInWithCode(h, "claim-mover@convt.test", new Jar(), ip);
    await h.request("/email-otp/request-email-change", {
      body: { newEmail: "unclaimed@convt.test" },
      jar,
      ip,
    });
    expect(await owner()).toBeNull();
    const res = await h.request("/email-otp/change-email", {
      body: { newEmail: "unclaimed@convt.test", otp: h.codeFor("unclaimed@convt.test") },
      jar,
      ip,
    });
    expect(res.status).toBe(200);
    expect(await owner()).toBe((await userByEmail("unclaimed@convt.test"))!.id);
  });
});

describe("taking over an unverified account", () => {
  test("a code sign-in from elsewhere drops unproven identities, sessions and devices", async () => {
    const email = "takeover@convt.test";
    const squatter = new Jar();
    await oauth(h, "sign-in", "github", "github-verified", squatter, { email });
    const user = (await userByEmail(email))!;
    await h.owner
      .insert(t.devices)
      .values({ id: "dev_takeover", userId: user.id, name: "Squatter Mac", os: "macOS" });
    expect((await accountsOf(user.id)).length).toBe(1);
    const owner = await signInWithCode(h, email, new Jar(), nextIp());
    expect((await userByEmail(email))!.emailVerified).toBe(true);
    expect((await accountsOf(user.id)).length).toBe(0);
    const sessions = await sessionsOf(user.id);
    expect(sessions.length).toBe(1);
    expect(sessions[0].token).toBe(owner.session!.split(".")[0]);
    const [device] = await h.owner.select().from(t.devices).where(eq(t.devices.id, "dev_takeover"));
    expect(device.revokedAt).not.toBeNull();
    // The squatter's cookie no longer works.
    expect(await (await h.request("/get-session", { jar: squatter })).json()).toBeNull();
  });
});

test("a takeover still holding its lock does not skip the cleanup", async () => {
  const email = "takeover-live-lock@convt.test";
  const squatter = new Jar();
  await oauth(h, "sign-in", "github", "github-verified", squatter, { email });
  const user = (await userByEmail(email))!;
  await h.owner
    .insert(t.devices)
    .values({ id: "dev_live_lock", userId: user.id, name: "Squatter Mac", os: "macOS" });
  // An attempt that stalled after taking the lock: Better Auth waits two seconds,
  // then gives up on its own cleanup.
  await h.owner.insert(t.verifications).values({
    id: await reservationId(`revoke-unproven-account-access:${user.id}`),
    identifier: `revoke-unproven-account-access:${user.id}`,
    value: user.id,
    expiresAt: new Date(Date.now() + 30_000),
  });
  const owner = await signInWithCode(h, email, new Jar(), nextIp());
  expect((await userByEmail(email))!.emailVerified).toBe(true);
  expect((await accountsOf(user.id)).length).toBe(0);
  const sessions = await sessionsOf(user.id);
  expect(sessions.map((x) => x.token)).toEqual([owner.session!.split(".")[0]]);
  expect(await (await h.request("/get-session", { jar: squatter })).json()).toBeNull();
  const [device] = await h.owner.select().from(t.devices).where(eq(t.devices.id, "dev_live_lock"));
  expect(device.revokedAt).not.toBeNull();
});

test("a lock left behind by an interrupted takeover does not skip the cleanup", async () => {
  const email = "takeover-stale-lock@convt.test";
  const squatter = new Jar();
  await oauth(h, "sign-in", "github", "github-verified", squatter, { email });
  const user = (await userByEmail(email))!;
  await h.owner.insert(t.verifications).values({
    id: await reservationId(`revoke-unproven-account-access:${user.id}`),
    identifier: `revoke-unproven-account-access:${user.id}`,
    value: user.id,
    expiresAt: new Date(Date.now() - 60_000),
  });
  await signInWithCode(h, email, new Jar(), nextIp());
  expect((await userByEmail(email))!.emailVerified).toBe(true);
  expect((await accountsOf(user.id)).length).toBe(0);
  expect(await (await h.request("/get-session", { jar: squatter })).json()).toBeNull();
});

describe("an address that changes accounts during a code sign-in", () => {
  /**
   * Sends a code to `email`, then signs in with it while the code row is held, so
   * the request has run its before hook but not yet consumed the code. `move`
   * runs in that pause.
   */
  async function signInAcrossMove(email: string, ip: string, move: () => Promise<void>) {
    const jar = new Jar();
    await h.request("/email-otp/send-verification-otp", {
      body: { email, type: "sign-in" },
      jar,
      ip,
    });
    const otp = h.codeFor(email);
    const holder = new pg.Client({ connectionString: h.tdb.webUrl });
    await holder.connect();
    try {
      await holder.query("begin");
      await holder.query("select 1 from verifications where identifier = $1 for update", [
        `sign-in-otp-${email}`,
      ]);
      const pending = h.request("/sign-in/email-otp", { body: { email, otp }, jar, ip });
      const deadline = Date.now() + 10_000;
      for (;;) {
        const waiting = await h.owner.execute<{ n: number }>(
          sql`select count(*)::int as n from pg_locks where not granted`,
        );
        if (waiting.rows[0].n > 0) break;
        if (Date.now() > deadline) throw new Error("the sign-in never reached the code row");
        await new Promise((r) => setTimeout(r, 25));
      }
      await move();
      await holder.query("commit");
      return { res: await pending, jar };
    } finally {
      await holder.end();
    }
  }

  /** An unverified GitHub account takes `email`, with a device and a live cleanup lock. */
  async function squat(email: string, deviceId: string) {
    const jar = new Jar();
    await oauth(h, "sign-in", "github", "github-verified", jar, { email });
    const user = (await userByEmail(email))!;
    expect(user.emailVerified).toBe(false);
    await h.owner
      .insert(t.devices)
      .values({ id: deviceId, userId: user.id, name: "Squatter Mac", os: "macOS" });
    await h.owner.insert(t.verifications).values({
      id: await reservationId(`revoke-unproven-account-access:${user.id}`),
      identifier: `revoke-unproven-account-access:${user.id}`,
      value: user.id,
      expiresAt: new Date(Date.now() + 30_000),
    });
    return { jar, user };
  }

  /** The sign-in was refused, left no session behind, and a fresh code removes the squatter. */
  async function expectRefusedThenCleaned(
    email: string,
    ip: string,
    attempt: { res: Response; jar: Jar },
    squatter: { jar: Jar; user: typeof t.users.$inferSelect },
    deviceId: string,
  ) {
    expect(`${attempt.res.status} ${(await json(attempt.res)).code}`).toBe("409 ACCOUNT_CHANGED");
    // The response expires the session cookie instead of setting the new session's.
    const sessionCookies = attempt.res.headers
      .getSetCookie()
      .filter((c) => c.startsWith("convt.session_token="));
    expect(sessionCookies.length).toBeGreaterThan(0);
    expect(sessionCookies.every((c) => /max-age=0/i.test(c))).toBe(true);
    expect(attempt.jar.session).toBeUndefined();
    const squatterToken = squatter.jar.session!.split(".")[0];
    expect((await sessionsOf(squatter.user.id)).map((x) => x.token)).toEqual([squatterToken]);
    // The mailbox owner signs in again with a new code: the squatter is gone.
    const owner = await signInWithCode(h, email, new Jar(), ip);
    expect((await userByEmail(email))!.emailVerified).toBe(true);
    expect((await accountsOf(squatter.user.id)).length).toBe(0);
    expect((await sessionsOf(squatter.user.id)).map((x) => x.token)).toEqual([
      owner.session!.split(".")[0],
    ]);
    expect(await (await h.request("/get-session", { jar: squatter.jar })).json()).toBeNull();
    const [device] = await h.owner.select().from(t.devices).where(eq(t.devices.id, deviceId));
    expect(device.revokedAt).not.toBeNull();
  }

  test("from an unverified account to an unverified squatter holding the lock", async () => {
    const ip = nextIp();
    const email = "handoff-unverified@convt.test";
    const original = new Jar();
    await oauth(h, "sign-in", "google", "google-thirdparty", original, { email });
    const a = (await userByEmail(email))!;
    expect(a.emailVerified).toBe(false);
    let squatter!: Awaited<ReturnType<typeof squat>>;
    const attempt = await signInAcrossMove(email, ip, async () => {
      await h.owner
        .update(t.users)
        .set({ email: "handoff-unverified-moved@convt.test" })
        .where(eq(t.users.id, a.id));
      squatter = await squat(email, "dev_handoff_unverified");
    });
    // The account the code was checked against is left as it was.
    expect((await accountsOf(a.id)).length).toBe(1);
    expect((await sessionsOf(a.id)).length).toBe(1);
    await expectRefusedThenCleaned(email, ip, attempt, squatter, "dev_handoff_unverified");
  });

  test("from a verified account to an unverified squatter holding the lock", async () => {
    const ip = nextIp();
    const email = "handoff-verified@convt.test";
    const original = await signInWithCode(h, email, new Jar(), ip);
    const a = (await userByEmail(email))!;
    let squatter!: Awaited<ReturnType<typeof squat>>;
    const attempt = await signInAcrossMove(email, ip, async () => {
      await h.owner
        .update(t.users)
        .set({ email: "handoff-verified-moved@convt.test" })
        .where(eq(t.users.id, a.id));
      squatter = await squat(email, "dev_handoff_verified");
    });
    expect(await (await h.request("/get-session", { jar: original })).json()).not.toBeNull();
    await expectRefusedThenCleaned(email, ip, attempt, squatter, "dev_handoff_verified");
  });

  test("from no account to an unverified squatter holding the lock", async () => {
    const ip = nextIp();
    const email = "handoff-new@convt.test";
    let squatter!: Awaited<ReturnType<typeof squat>>;
    const attempt = await signInAcrossMove(email, ip, async () => {
      squatter = await squat(email, "dev_handoff_new");
    });
    await expectRefusedThenCleaned(email, ip, attempt, squatter, "dev_handoff_new");
  });
});

describe("linking from settings", () => {
  test("a fresh session links GitHub", async () => {
    const ip = nextIp();
    const jar = await signInWithCode(h, "linker@convt.test", new Jar(), ip);
    const result = await oauth(h, "link", "github", "github-public-differs", jar, {
      email: "linker-gh@convt.test",
    });
    expect(result.location).toBe("/account");
    const user = (await userByEmail("linker@convt.test"))!;
    expect((await accountsOf(user.id)).map((a) => a.providerId)).toEqual(["github"]);
  });

  test("a session older than an hour gets SESSION_NOT_FRESH from /link-social, called directly", async () => {
    const jar = await signInWithCode(h, "stale-linker@convt.test", new Jar(), nextIp());
    await ageSessions((await userByEmail("stale-linker@convt.test"))!.id, 61);
    const res = await h.request("/link-social", {
      body: { provider: "github", callbackURL: "/account", disableRedirect: true },
      jar,
    });
    expect(res.status).toBe(403);
    expect((await json(res)).code).toBe("SESSION_NOT_FRESH");
  });

  test("an idToken body is rejected", async () => {
    const jar = await signInWithCode(h, "idtoken-linker@convt.test", new Jar(), nextIp());
    const res = await h.request("/link-social", {
      body: { provider: "google", idToken: { token: "header.payload.signature" } },
      jar,
    });
    expect(res.status).toBe(400);
    expect((await json(res)).code).toBe("ID_TOKEN_NOT_ALLOWED");
  });

  async function tamperedLink(
    name: string,
    tamper: (state: Record<string, unknown>, userId: string) => Promise<void> | void,
  ) {
    const email = `${name}@convt.test`;
    const jar = await signInWithCode(h, email, new Jar(), nextIp());
    const user = (await userByEmail(email))!;
    const result = await oauth(h, "link", "github", "github-verified", jar, {
      email: `${name}-gh@convt.test`,
      beforeCallback: async () => {
        const rows = await h.owner.execute<{ id: string; value: string }>(
          sql`select id, value from verifications where value like ${`%"userId":"${user.id}"%`} and value like '%oauthState%'`,
        );
        expect(rows.rows.length).toBe(1);
        const state = JSON.parse(rows.rows[0].value);
        await tamper(state, user.id);
        await h.owner.execute(
          sql`update verifications set value = ${JSON.stringify(state)} where id = ${rows.rows[0].id}`,
        );
      },
    });
    return { result, accounts: await accountsOf(user.id) };
  }

  test("a callback with no link intent links nothing", async () => {
    const { result, accounts } = await tamperedLink(
      "no-intent",
      (s) => void delete s.serverContext,
    );
    expect(result.location).toContain("error=");
    expect(accounts.length).toBe(0);
  });

  test("a callback whose intent names another user links nothing", async () => {
    const { result, accounts } = await tamperedLink("other-user", (s) => {
      (s.serverContext as { linkIntent: { userId: string } }).linkIntent.userId = fixtureId(
        "usr",
        "pro",
      );
    });
    expect(result.location).toContain("error=");
    expect(accounts.length).toBe(0);
  });

  test("a callback whose intent names a revoked session links nothing", async () => {
    const { result, accounts } = await tamperedLink("revoked-session", async (_s, userId) => {
      await h.owner.delete(t.sessions).where(eq(t.sessions.userId, userId));
    });
    expect(result.location).toContain("error=");
    expect(accounts.length).toBe(0);
  });

  test("an expired intent links nothing", async () => {
    const { result, accounts } = await tamperedLink("old-intent", (s) => {
      (s.serverContext as { linkIntent: { at: number } }).linkIntent.at =
        Date.now() - 11 * 60 * 1000;
    });
    expect(result.location).toContain("error=");
    expect(accounts.length).toBe(0);
  });

  test("unlinking needs a fresh session", async () => {
    const email = "unlinker@convt.test";
    const jar = await signInWithCode(h, email, new Jar(), nextIp());
    await oauth(h, "link", "github", "github-verified", jar, { email: "unlinker-gh@convt.test" });
    const user = (await userByEmail(email))!;
    await ageSessions(user.id, 61);
    const [linked] = await accountsOf(user.id);
    const body = { accountId: linked.id };
    const stale = await h.request("/unlink-account", { body, jar });
    expect(stale.status).toBe(403);
    expect((await json(stale)).code).toBe("SESSION_NOT_FRESH");
    await ageSessions(user.id, 0);
    const fresh = await h.request("/unlink-account", { body, jar });
    expect(`${fresh.status} ${JSON.stringify(await json(fresh))}`).toBe('200 {"status":true}');
    expect((await accountsOf(user.id)).length).toBe(0);
  });
});

describe("changing the email", () => {
  test("the code goes to the new address only; other sessions end; the current one shows the new address", async () => {
    const ip = nextIp();
    const old = "mover@convt.test";
    const next = "mover-new@convt.test";
    const jar = await signInWithCode(h, old, new Jar(), ip);
    const other = await signInWithCode(h, old, new Jar(), ip);
    const user = (await userByEmail(old))!;
    await h.owner
      .insert(t.devices)
      .values({ id: "dev_mover", userId: user.id, name: "Mover Mac", os: "macOS" });
    const oldMails = mailsTo(old);
    expect(
      (await h.request("/email-otp/request-email-change", { body: { newEmail: next }, jar, ip }))
        .status,
    ).toBe(200);
    expect(mailsTo(next)).toBe(1);
    expect(mailsTo(old)).toBe(oldMails);
    const res = await h.request("/email-otp/change-email", {
      body: { newEmail: next, otp: h.codeFor(next) },
      jar,
      ip,
    });
    expect(res.status).toBe(200);
    const mine = (await json(await h.request("/get-session", { jar }))) as {
      user: { email: string; emailVerified: boolean };
    };
    expect(mine.user).toMatchObject({ email: next, emailVerified: true });
    expect(await (await h.request("/get-session", { jar: other })).json()).toBeNull();
    const [device] = await h.owner.select().from(t.devices).where(eq(t.devices.id, "dev_mover"));
    expect(device.revokedAt).not.toBeNull();
  });

  test("a stale session is refused at request and at confirmation", async () => {
    const ip = nextIp();
    const jar = await signInWithCode(h, "stale-mover@convt.test", new Jar(), ip);
    const user = (await userByEmail("stale-mover@convt.test"))!;
    await ageSessions(user.id, 61);
    const req = await h.request("/email-otp/request-email-change", {
      body: { newEmail: "stale-new@convt.test" },
      jar,
      ip,
    });
    expect(req.status).toBe(403);
    expect(mailsTo("stale-new@convt.test")).toBe(0);
    const confirm = await h.request("/email-otp/change-email", {
      body: { newEmail: "stale-new@convt.test", otp: "123456" },
      jar,
      ip,
    });
    expect((await json(confirm)).code).toBe("SESSION_NOT_FRESH");
  });

  test("an address taken by another user gets no code and is refused at confirmation", async () => {
    const ip = nextIp();
    const jar = await signInWithCode(h, "grabber@convt.test", new Jar(), ip);
    const taken = "desktop@convt.test";
    const before = mailsTo(taken);
    const res = await h.request("/email-otp/request-email-change", {
      body: { newEmail: taken },
      jar,
      ip,
    });
    expect(res.status).toBe(200); // Same answer as for a free address: no account is revealed.
    expect(mailsTo(taken)).toBe(before);
    // Even with a code for that identifier, confirmation is refused.
    await h.owner.insert(t.verifications).values({
      id: "ver_forged_change",
      identifier: `change-email-otp-grabber@convt.test-${taken}`,
      value: `${await sha256b64("424242")}:0`,
      expiresAt: new Date(Date.now() + 600_000),
    });
    const confirm = await h.request("/email-otp/change-email", {
      body: { newEmail: taken, otp: "424242" },
      jar,
      ip,
    });
    expect(confirm.status).toBe(400);
    expect(await userByEmail("grabber@convt.test")).not.toBeNull();
  });
});

describe("cookies", () => {
  test("the session cookie is HttpOnly, Lax, host-only on /", async () => {
    const ip = nextIp();
    const email = "cookie@convt.test";
    await h.request("/email-otp/send-verification-otp", { body: { email, type: "sign-in" }, ip });
    const res = await h.request("/sign-in/email-otp", {
      body: { email, otp: h.codeFor(email) },
      ip,
    });
    const cookie = res.headers.getSetCookie().find((c) => c.startsWith("convt.session_token="))!;
    expect(cookie).toMatch(/HttpOnly/i);
    expect(cookie).toMatch(/SameSite=Lax/i);
    expect(cookie).toMatch(/Path=\//);
    expect(cookie).not.toMatch(/Domain=/i);
    expect(cookie).toMatch(/Max-Age=2592000/);
  });

  test("in production it is Secure with the __Secure- prefix", async () => {
    const prod = await startHarness({ production: true });
    try {
      const email = "cookie-prod@convt.test";
      await prod.request("/email-otp/send-verification-otp", { body: { email, type: "sign-in" } });
      const res = await prod.request("/sign-in/email-otp", {
        body: { email, otp: prod.codeFor(email) },
      });
      const cookie = res.headers.getSetCookie().find((c) => c.includes("session_token="))!;
      expect(cookie.startsWith("__Secure-convt.session_token=")).toBe(true);
      expect(cookie).toMatch(/; Secure/i);
      expect(cookie).toMatch(/HttpOnly/i);
    } finally {
      await prod.close();
    }
  });

  test("password reset and the code checker are disabled", async () => {
    for (const path of [
      "/email-otp/request-password-reset",
      "/forget-password/email-otp",
      "/email-otp/reset-password",
      "/email-otp/check-verification-otp",
    ]) {
      const res = await h.request(path, {
        body: {
          email: "desktop@convt.test",
          otp: "123456",
          password: "x".repeat(12),
          type: "sign-in",
        },
        ip: nextIp(),
      });
      expect(`${path} ${res.status}`).toBe(`${path} 404`);
    }
    expect(
      (
        await h.owner
          .select()
          .from(t.accounts)
          .where(and(eq(t.accounts.providerId, "credential"), isNull(t.accounts.password)))
      ).length,
    ).toBe(0);
  });
});

/** The id Better Auth gives a lock row (reserveVerificationValue), so a test row looks like its own. */
async function reservationId(identifier: string) {
  return sha256b64(`reserve:${identifier}`);
}

async function sha256b64(text: string) {
  const d = new Uint8Array(await crypto.subtle.digest("SHA-256", new TextEncoder().encode(text)));
  return btoa(String.fromCharCode(...d))
    .replace(/\+/g, "-")
    .replace(/\//g, "_")
    .replace(/=+$/, "");
}
