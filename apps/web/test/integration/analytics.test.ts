// Signup analytics: user_signed_up fires once per new account, from the auth
// insert hook, for email-code and OAuth sign-in.

import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { eq, sql } from "drizzle-orm";

import * as t from "@convt/db/schema";
import { newId } from "@convt/license";

import { testMailbox } from "../mailbox";
import { Jar, oauth, signInWithCode, startHarness, type Harness } from "./harness";

let h: Harness;
let ipCounter = 40;
const nextIp = () => `198.51.100.${ipCounter++}`;

beforeAll(async () => {
  h = await startHarness();
});
afterAll(async () => h?.close());

const userByMailbox = async (address: string) =>
  (await h.owner.select().from(t.users).where(eq(t.users.email, address)))[0] ?? null;

const signupsFor = (userId: string) =>
  h.analytics.filter((e) => e.event === "user_signed_up" && e.distinctId === userId);

describe("user_signed_up", () => {
  test("an email code creates one event and a later sign-in does not", async () => {
    const address = testMailbox("signup-email");
    const ip = nextIp();
    await h.request("/email-otp/send-verification-otp", {
      body: { email: address, type: "sign-in" },
      ip,
    });
    expect(h.analytics.filter((e) => e.event === "user_signed_up")).toEqual([]);

    const jar = new Jar();
    const first = await h.request("/sign-in/email-otp", {
      body: { email: address, otp: h.codeFor(address) },
      jar,
      ip,
      headers: {
        cookie: `convt_signup=${encodeURIComponent(JSON.stringify({ source: "/pricing", utm_source: "hn" }))}`,
        referer: "https://convt.app/sign-in",
      },
    });
    expect(first.status).toBe(200);
    const user = await userByMailbox(address);
    expect(user).toBeTruthy();
    const firstEvents = signupsFor(user!.id);
    expect(firstEvents.length).toBe(1);
    expect(firstEvents[0]).toMatchObject({
      event: "user_signed_up",
      distinctId: user!.id,
      insertId: `user_signed_up:${user!.id}`,
      properties: { signup_method: "email", source: "/pricing", utm_source: "hn" },
    });
    expect(JSON.stringify(firstEvents[0])).not.toContain(address);
    expect(firstEvents[0].properties).not.toHaveProperty("email");

    await h.request("/sign-out", { jar, ip });
    await signInWithCode(h, address, new Jar(), nextIp());
    expect(signupsFor(user!.id).length).toBe(1);
  });

  test("GitHub and Google each fire once", async () => {
    const ghAddress = testMailbox("signup-gh");
    const github = await oauth(h, "sign-in", "github", "github-verified", new Jar(), {
      email: ghAddress,
    });
    expect(github.status).toBe(302);
    const ghUser = await userByMailbox(ghAddress);
    expect(ghUser).toBeTruthy();
    const ghEvents = signupsFor(ghUser!.id);
    expect(ghEvents.length).toBe(1);
    expect(ghEvents[0].properties).toMatchObject({ signup_method: "github" });
    expect(JSON.stringify(ghEvents[0])).not.toContain(ghAddress);

    const goAddress = testMailbox("signup-google");
    const google = await oauth(h, "sign-in", "google", "google-gmail", new Jar(), {
      email: goAddress,
    });
    expect(google.status).toBe(302);
    const goUser = await userByMailbox(goAddress);
    expect(goUser).toBeTruthy();
    expect(signupsFor(goUser!.id).length).toBe(1);
    expect(signupsFor(goUser!.id)[0].properties).toMatchObject({ signup_method: "google" });
  });

  test("linking an OAuth account does not fire user_signed_up", async () => {
    const address = testMailbox("signup-link");
    const jar = await signInWithCode(h, address, new Jar(), nextIp());
    const user = await userByMailbox(address);
    expect(signupsFor(user!.id).length).toBe(1);
    await oauth(h, "link", "github", "github-public-differs", jar, {
      email: testMailbox("signup-link-gh"),
    });
    expect(signupsFor(user!.id).length).toBe(1);
  });

  test("DNT, GPC and the privacy cookie skip user_signed_up", async () => {
    for (const [local, headers] of [
      ["signup-dnt", { dnt: "1" }],
      ["signup-gpc", { "sec-gpc": "1" }],
      ["signup-optout", { cookie: "convt_analytics=off" }],
    ] as const) {
      const address = testMailbox(local);
      const ip = nextIp();
      await h.request("/email-otp/send-verification-otp", {
        body: { email: address, type: "sign-in" },
        ip,
        headers,
      });
      const res = await h.request("/sign-in/email-otp", {
        body: { email: address, otp: h.codeFor(address) },
        jar: new Jar(),
        ip,
        headers,
      });
      expect(res.status).toBe(200);
      const user = await userByMailbox(address);
      expect(user).toBeTruthy();
      expect(signupsFor(user!.id)).toEqual([]);
    }
  });
});

describe("license_purchased on claim", () => {
  test("a guest Desktop purchase fires once when the account is created", async () => {
    const address = testMailbox("signup-claim");
    const orderId = newId("ord");
    const now = new Date();
    const day = now.toISOString().slice(0, 10);
    await h.owner.execute(sql`
      insert into orders (id, provider, provider_order_id, email, product, amount_cents, currency, status, paid_at, billed_at, created_at, updated_at)
      values (${orderId}, 'polar', ${`polar_${orderId}`}, ${address}, 'desktop', 2900, 'usd', 'paid', ${now}, ${now}, ${now}, ${now})`);
    await h.owner.execute(sql`
      insert into licenses (id, email, plan, trial, order_id, issued_on, updates_until, token, created_at, updated_at)
      values (${newId("lic")}, ${address}, 'desktop', false, ${orderId}, ${day}, ${"2027-10-07"}, ${"claim-token"}, ${now}, ${now})`);

    const ip = nextIp();
    await h.request("/email-otp/send-verification-otp", {
      body: { email: address, type: "sign-in" },
      ip,
    });
    const first = await h.request("/sign-in/email-otp", {
      body: { email: address, otp: h.codeFor(address) },
      jar: new Jar(),
      ip,
    });
    expect(first.status).toBe(200);
    const user = await userByMailbox(address);
    expect(user).toBeTruthy();
    const purchases = h.analytics.filter(
      (e) => e.event === "license_purchased" && e.distinctId === user!.id,
    );
    expect(purchases.length).toBe(1);
    expect(purchases[0]).toMatchObject({
      event: "license_purchased",
      distinctId: user!.id,
      insertId: `license_purchased:desktop:${orderId}`,
      properties: { plan: "desktop" },
    });
    expect(JSON.stringify(purchases[0])).not.toContain(address);

    await signInWithCode(h, address, new Jar(), nextIp());
    expect(
      h.analytics.filter((e) => e.event === "license_purchased" && e.distinctId === user!.id)
        .length,
    ).toBe(1);
  });
});
