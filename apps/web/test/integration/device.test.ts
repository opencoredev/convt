// Desktop sign-in and Pro renewal (P8) against a disposable Postgres: the site's
// functions run as convt_web, the Pro key comes from convt-billing's query as
// convt_billing, like the two Workers. See src/server/device-auth.ts.

import { afterAll, beforeAll, describe, expect, test } from "bun:test";

import { currentProKey } from "@convt/billing";
import type { Db } from "@convt/db";
import { activeDevices, revokeDevice, revokeOtherSessions } from "@convt/db/queries";
import * as t from "@convt/db/schema";
import { runSeed } from "@convt/db/seed";
import { base64urlEncode, importSigningKey } from "@convt/license";
import { eq, sql } from "drizzle-orm";

import {
  approveDevice,
  bearer,
  deviceLimits,
  exchangeCode,
  parseDeviceRequest,
  readSmallJson,
  renewDevice,
  signOutDevice,
  tokenHash,
  type DeviceRequest,
} from "../../src/server/device-auth";
import { startHarness, type Harness } from "./harness";

let h: Harness;
let web: Db;
let billingDb: Db;
let ipCounter = 10;
const nextIp = () => `198.51.100.${ipCounter++}`;

beforeAll(async () => {
  h = await startHarness();
  await runSeed(
    h.owner,
    new Date(),
    await importSigningKey(crypto.getRandomValues(new Uint8Array(32))),
  );
  web = (await h.tdb.open("web")).db;
  billingDb = (await h.tdb.open("billing")).db;
});
afterAll(async () => h?.close());

const userId = async (email: string) =>
  (await h.owner.select().from(t.users).where(eq(t.users.email, email)))[0].id;
const proKey = (id: string) => currentProKey(billingDb, id);

/** What the app does: a random state and verifier, and the verifier's hash. */
async function appFlow(name = "Leo's PC") {
  const rand = () => base64urlEncode(crypto.getRandomValues(new Uint8Array(32)));
  const verifier = rand();
  const challenge = base64urlEncode(
    new Uint8Array(await crypto.subtle.digest("SHA-256", new TextEncoder().encode(verifier))),
  );
  const req = parseDeviceRequest({
    state: rand(),
    challenge,
    name,
    os: "Linux",
    version: "0.1.0",
  }) as DeviceRequest;
  return { req, verifier };
}

const codeOf = (link: string) => new URL(link.replace("convt://", "https://x/")).searchParams;

/** Approves as `email`, then exchanges. Returns the token. */
async function signIn(email: string, now = new Date()) {
  const { req, verifier } = await appFlow();
  const approved = await approveDevice(web, await userId(email), req, now);
  if (!approved.ok) throw new Error("approval refused");
  const params = codeOf(approved.link);
  expect(params.get("state")).toBe(req.state);
  const res = await exchangeCode(web, { code: params.get("code"), verifier }, nextIp(), now);
  expect(res.status).toBe(200);
  return res.body.token as string;
}

describe("device sign-in", () => {
  test("approve, exchange and renew: the token is stored hashed and returns the Pro key", async () => {
    const pro = await userId("pro@convt.test");
    const { req, verifier } = await appFlow();
    const approved = await approveDevice(web, pro, req, new Date());
    expect(approved.ok).toBe(true);
    if (!approved.ok) return;
    expect(approved.link).toStartWith(`convt://auth?state=${req.state}&code=`);
    // The challenge is stored, never the verifier.
    const stored = await h.owner.select().from(t.verifications);
    expect(JSON.stringify(stored)).not.toContain(verifier);

    const res = await exchangeCode(
      web,
      { code: codeOf(approved.link).get("code"), verifier },
      nextIp(),
      new Date(),
    );
    expect(res.status).toBe(200);
    expect(res.body.email).toBe("pro@convt.test");
    const token = res.body.token as string;
    expect(bearer(`Bearer ${token}`)).toBe(token);
    const [device] = await h.owner
      .select()
      .from(t.devices)
      .where(eq(t.devices.tokenHash, await tokenHash(token)));
    expect(device.userId).toBe(pro);
    expect(device.name).toBe("Leo's PC");
    expect(device.os).toBe("Linux");
    expect(device.appVersion).toBe("0.1.0");
    expect(JSON.stringify(device)).not.toContain(token);
    // It shows on the dashboard's list.
    expect((await activeDevices(web, pro)).map((d) => d.id)).toContain(device.id);

    const renewed = await renewDevice(
      web,
      proKey,
      token,
      { version: "0.1.1" },
      nextIp(),
      new Date(),
    );
    expect(renewed.status).toBe(200);
    const expected = await h.owner.execute<{ token: string }>(sql`
      select token from licenses where user_id = ${pro} and plan = 'pro' and revoked_at is null
      order by updates_until desc limit 1`);
    expect(renewed.body.key).toBe(expected.rows[0].token);
    const [seen] = await h.owner.select().from(t.devices).where(eq(t.devices.id, device.id));
    expect(seen.appVersion).toBe("0.1.1");
  });

  test("a code works once, only with its verifier, and only for five minutes", async () => {
    const pro = await userId("pro@convt.test");
    const now = new Date();
    const { req, verifier } = await appFlow();
    const a = await approveDevice(web, pro, req, now);
    if (!a.ok) throw new Error("refused");
    const code = codeOf(a.link).get("code");
    // A wrong verifier fails, and burns the code.
    const other = (await appFlow()).verifier;
    expect((await exchangeCode(web, { code, verifier: other }, nextIp(), now)).status).toBe(400);
    expect((await exchangeCode(web, { code, verifier }, nextIp(), now)).status).toBe(400);
    // Replaying a used code fails.
    const b = await approveDevice(web, pro, req, now);
    if (!b.ok) throw new Error("refused");
    const codeB = codeOf(b.link).get("code");
    expect((await exchangeCode(web, { code: codeB, verifier }, nextIp(), now)).status).toBe(200);
    const replay = await exchangeCode(web, { code: codeB, verifier }, nextIp(), now);
    expect(replay).toEqual({ status: 400, body: { error: "invalid_grant" } });
    // An expired code fails.
    const c = await approveDevice(web, pro, req, now);
    if (!c.ok) throw new Error("refused");
    const late = new Date(now.getTime() + 5 * 60 * 1000 + 1);
    const codeC = codeOf(c.link).get("code");
    expect((await exchangeCode(web, { code: codeC, verifier }, nextIp(), late)).status).toBe(400);
    // Garbage is refused before any lookup.
    for (const body of [null, {}, { code: "x", verifier }, { code: codeB, verifier: 1 }]) {
      expect((await exchangeCode(web, body, nextIp(), now)).status).toBe(400);
    }
  });

  test("a code for a user who lost their verified email is refused", async () => {
    const id = await userId("new@convt.test");
    const { req, verifier } = await appFlow();
    const a = await approveDevice(web, id, req, new Date());
    if (!a.ok) throw new Error("refused");
    await h.owner.update(t.users).set({ emailVerified: false }).where(eq(t.users.id, id));
    try {
      const res = await exchangeCode(
        web,
        { code: codeOf(a.link).get("code"), verifier },
        nextIp(),
        new Date(),
      );
      expect(res.status).toBe(400);
    } finally {
      await h.owner.update(t.users).set({ emailVerified: true }).where(eq(t.users.id, id));
    }
  });

  test("the /device URL is checked and its text cleaned", () => {
    const ok = { state: "a".repeat(43), challenge: "b".repeat(43) };
    expect(parseDeviceRequest(ok)?.name).toBe("This computer");
    expect(parseDeviceRequest({ ...ok, state: "short" })).toBeNull();
    expect(parseDeviceRequest({ ...ok, challenge: "c".repeat(42) + "=" })).toBeNull();
    const r = parseDeviceRequest({
      ...ok,
      name: `  Mac\u0000‮${"x".repeat(100)}`,
      version: "1.0<script>",
    })!;
    expect(r.name).toBe(`Mac${"x".repeat(61)}`);
    expect(r.version).toBe("");
    expect(bearer("Bearer nope")).toBeNull();
    expect(bearer(null)).toBeNull();
  });
});

describe("revocation", () => {
  test("Sign out on the dashboard makes the token answer 401", async () => {
    const pro = await userId("pro@convt.test");
    const token = await signIn("pro@convt.test");
    const [device] = await h.owner
      .select()
      .from(t.devices)
      .where(eq(t.devices.tokenHash, await tokenHash(token)));
    expect(await revokeDevice(web, pro, device.id, new Date())).toBe(true);
    const res = await renewDevice(web, proKey, token, {}, nextIp(), new Date());
    expect(res).toEqual({ status: 401, body: { error: "signed_out" } });
    expect((await activeDevices(web, pro)).map((d) => d.id)).not.toContain(device.id);
  });

  test("signing out every other session signs devices out too", async () => {
    const pro = await userId("pro@convt.test");
    const token = await signIn("pro@convt.test");
    await revokeOtherSessions(web, pro, "ses_none", new Date());
    expect((await renewDevice(web, proKey, token, {}, nextIp(), new Date())).status).toBe(401);
  });

  test("the app's Sign out revokes its own device, once", async () => {
    const token = await signIn("pro@convt.test");
    expect((await signOutDevice(web, token, new Date())).status).toBe(200);
    expect((await signOutDevice(web, token, new Date())).status).toBe(401);
    expect((await renewDevice(web, proKey, token, {}, nextIp(), new Date())).status).toBe(401);
    expect((await renewDevice(web, proKey, null, {}, nextIp(), new Date())).status).toBe(401);
  });
});

describe("cross-account isolation", () => {
  test("one account can neither revoke nor renew through another's device", async () => {
    const pro = await userId("pro@convt.test");
    const desktop = await userId("desktop@convt.test");
    const token = await signIn("pro@convt.test");
    const [device] = await h.owner
      .select()
      .from(t.devices)
      .where(eq(t.devices.tokenHash, await tokenHash(token)));
    expect(await revokeDevice(web, desktop, device.id, new Date())).toBe(false);
    expect((await activeDevices(web, desktop)).map((d) => d.id)).not.toContain(device.id);
    // The key comes from the token's account, never the caller's choice.
    const asked: string[] = [];
    const res = await renewDevice(
      web,
      async (id) => {
        asked.push(id);
        return proKey(id);
      },
      token,
      { userId: desktop },
      nextIp(),
      new Date(),
    );
    expect(res.status).toBe(200);
    expect(asked).toEqual([pro]);
    // A Desktop-only account has no Pro key to renew to.
    const desktopToken = await signIn("desktop@convt.test");
    const none = await renewDevice(web, proKey, desktopToken, {}, nextIp(), new Date());
    expect(none.body.key).toBeNull();
    // A lapsed account gets its last Pro key, and refunded or revoked keys never come back.
    const lapsed = await proKey(await userId("lapsed@convt.test"));
    expect(lapsed?.key).toBeTruthy();
    const refunded = await proKey(await userId("refunded@convt.test"));
    expect(refunded).toBeNull();
  });
});

describe("rate limits", () => {
  test("approvals per user, exchanges per IP and renewals per device", async () => {
    const id = await userId("api@convt.test");
    const now = new Date();
    for (let i = 0; i < deviceLimits.approvePerUser; i++) {
      expect((await approveDevice(web, id, (await appFlow()).req, now)).ok).toBe(true);
    }
    expect(await approveDevice(web, id, (await appFlow()).req, now)).toEqual({
      ok: false,
      reason: "rate_limited",
    });

    const ip = nextIp();
    for (let i = 0; i < deviceLimits.tokenPerIp; i++) {
      expect((await exchangeCode(web, {}, ip, now)).status).toBe(400);
    }
    expect((await exchangeCode(web, {}, ip, now)).status).toBe(429);
    // Another IP is not affected.
    expect((await exchangeCode(web, {}, nextIp(), now)).status).toBe(400);

    const token = await signIn("pro@convt.test");
    for (let i = 0; i < deviceLimits.renewPerDevice; i++) {
      expect((await renewDevice(web, proKey, token, {}, nextIp(), now)).status).toBe(200);
    }
    expect((await renewDevice(web, proKey, token, {}, nextIp(), now)).status).toBe(429);
    // The window passes.
    const later = new Date(now.getTime() + 61 * 60 * 1000);
    expect((await renewDevice(web, proKey, token, {}, nextIp(), later)).status).toBe(200);
  });
});

describe("request bodies", () => {
  const post = (body: BodyInit, headers: Record<string, string> = {}) =>
    new Request("http://localhost/api/device/token", { method: "POST", body, headers });

  test("a small JSON body is read; an oversized one stops early, declared or not", async () => {
    expect(await readSmallJson(post('{"code":"x"}'))).toEqual({ value: { code: "x" } });
    expect(await readSmallJson(post("not json"))).toBeNull();
    expect(await readSmallJson(post("x".repeat(10), { "content-length": "5000" }))).toBe(
      "too_large",
    );
    // A stream with no length: reading stops at the first chunk past the limit.
    let pulled = 0;
    const endless = new ReadableStream<Uint8Array>({
      pull(controller) {
        pulled++;
        controller.enqueue(new Uint8Array(1024));
        if (pulled > 2048) controller.close();
      },
    });
    expect(await readSmallJson(post(endless))).toBe("too_large");
    expect(pulled).toBeLessThan(16);
  });
});
