// Signup analytics: user_signed_up fires once per new account, from the auth
// insert hook, for email and OAuth.

import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { eq } from "drizzle-orm";

import * as t from "@convt/db/schema";

import { Jar, oauth, signInWithCode, startHarness, type Harness } from "./harness";

let h: Harness;
let ipCounter = 40;
const nextIp = () => `198.51.100.${ipCounter++}`;

beforeAll(async () => {
  h = await startHarness();
});
afterAll(async () => h?.close());

const userByEmail = async (email: string) =>
  (await h.owner.select().from(t.users).where(eq(t.users.email, email)))[0] ?? null;

const signupsFor = (userId: string) =>
  h.analytics.filter((e) => e.event === "user_signed_up" && e.distinctId === userId);

describe("user_signed_up", () => {
  test("an email code creates one event and a later sign-in does not", async () => {
    const email = "signup-email@convt.test";
    const ip = nextIp();
    await h.request("/email-otp/send-verification-otp", {
      body: { email, type: "sign-in" },
      ip,
    });
    expect(h.analytics.filter((e) => e.event === "user_signed_up")).toEqual([]);

    const jar = new Jar();
    const first = await h.request("/sign-in/email-otp", {
      body: { email, otp: h.codeFor(email) },
      jar,
      ip,
      headers: {
        cookie: `convt_signup=${encodeURIComponent(JSON.stringify({ source: "/pricing", utm_source: "hn" }))}`,
        referer: "https://convt.app/sign-in",
      },
    });
    expect(first.status).toBe(200);
    const user = await userByEmail(email);
    expect(user).toBeTruthy();
    const firstEvents = signupsFor(user!.id);
    expect(firstEvents.length).toBe(1);
    expect(firstEvents[0]).toMatchObject({
      event: "user_signed_up",
      distinctId: user!.id,
      insertId: `user_signed_up:${user!.id}`,
      properties: { signup_method: "email", source: "/pricing", utm_source: "hn" },
    });
    expect(JSON.stringify(firstEvents[0])).not.toContain(email);
    expect(firstEvents[0].properties).not.toHaveProperty("email");

    await h.request("/sign-out", { jar, ip });
    await signInWithCode(h, email, new Jar(), nextIp());
    expect(signupsFor(user!.id).length).toBe(1);
  });

  test("GitHub and Google each fire once", async () => {
    const github = await oauth(h, "sign-in", "github", "github-verified", new Jar(), {
      email: "signup-gh@convt.test",
    });
    expect(github.status).toBe(302);
    const ghUser = await userByEmail("signup-gh@convt.test");
    expect(ghUser).toBeTruthy();
    const ghEvents = signupsFor(ghUser!.id);
    expect(ghEvents.length).toBe(1);
    expect(ghEvents[0].properties).toMatchObject({ signup_method: "github" });
    expect(JSON.stringify(ghEvents[0])).not.toContain("signup-gh@convt.test");

    const google = await oauth(h, "sign-in", "google", "google-gmail", new Jar(), {
      email: "signup-google@convt.test",
    });
    expect(google.status).toBe(302);
    const goUser = await userByEmail("signup-google@convt.test");
    expect(goUser).toBeTruthy();
    expect(signupsFor(goUser!.id).length).toBe(1);
    expect(signupsFor(goUser!.id)[0].properties).toMatchObject({ signup_method: "google" });
  });

  test("linking an OAuth account does not fire user_signed_up", async () => {
    const email = "signup-link@convt.test";
    const jar = await signInWithCode(h, email, new Jar(), nextIp());
    const user = await userByEmail(email);
    expect(signupsFor(user!.id).length).toBe(1);
    await oauth(h, "link", "github", "github-public-differs", jar, {
      email: "signup-link-gh@convt.test",
    });
    expect(signupsFor(user!.id).length).toBe(1);
  });
});
