import { expect, test } from "bun:test";

import {
  attributionFromSearch,
  nextAttribution,
  parseAttributionCookie,
  sanitizeAttribution,
  signupMethodFromAuthContext,
  signupMethodFromAuthPath,
} from "../../src/lib/analytics-attribution";
import { syncIdentifiedUser } from "../../src/lib/identify-user";
import {
  captureEvent,
  claimedPurchaseEvents,
  signupEventFromAuthHook,
  signupProperties,
  userSignedUpEvent,
  withoutPii,
} from "../../src/server/analytics";
import { testMailbox } from "../mailbox";

test("signup method comes from the auth path, never from a mailbox", () => {
  expect(signupMethodFromAuthPath("/sign-in/email-otp")).toBe("email");
  expect(signupMethodFromAuthPath("/callback/github")).toBe("github");
  expect(signupMethodFromAuthPath("/oauth2/callback/google")).toBe("google");
  expect(signupMethodFromAuthPath("/callback/:id")).toBe("unknown");
  expect(signupMethodFromAuthPath("/get-session")).toBe("unknown");
  expect(signupMethodFromAuthContext({ path: "/callback/:id", providerId: "github" })).toBe(
    "github",
  );
  expect(
    signupMethodFromAuthContext({
      path: "/callback/:id",
      requestUrl: "http://localhost:3999/api/auth/callback/google?code=x",
    }),
  ).toBe("google");
});

test("signup properties keep the method, source and UTM and drop PII", () => {
  const leaked = testMailbox("hidden");
  const props = signupProperties({
    path: "/sign-in/email-otp",
    cookie: `convt_signup=${encodeURIComponent(
      JSON.stringify({
        source: "/pricing",
        utm_source: "twitter",
        utm_medium: "social",
        email: testMailbox("leo"),
      }),
    )}`,
    callbackURL: `/dashboard?email=${encodeURIComponent(leaked)}`,
    referer: "https://convt.app/sign-in?redirect=%2Fdashboard",
    siteOrigin: "https://convt.app",
  });
  expect(props).toEqual({
    signup_method: "email",
    source: "/pricing",
    utm_source: "twitter",
    utm_medium: "social",
  });
  expect(JSON.stringify(props)).not.toContain("@");
  expect(JSON.stringify(props)).not.toContain(leaked);
  expect(props).not.toHaveProperty("email");
});

test("withoutPii strips mailboxes, names and overlong strings", () => {
  const leaked = testMailbox("user");
  expect(
    withoutPii({
      signup_method: "github",
      email: testMailbox("a", ["b", "test"].join(".")),
      name: "Leo",
      note: "ok",
      leaked,
    }),
  ).toEqual({ signup_method: "github", note: "ok" });
});

test("user_signed_up uses the user id and a stable insert id", () => {
  const event = userSignedUpEvent("usr_abc", {
    signup_method: "google",
    email: testMailbox("nope", ["x", "test"].join(".")),
  });
  expect(event.event).toBe("user_signed_up");
  expect(event.distinctId).toBe("usr_abc");
  expect(event.insertId).toBe("user_signed_up:usr_abc");
  expect(event.properties).toEqual({ signup_method: "google" });
  expect(event.properties).not.toHaveProperty("email");
});

test("the auth hook builds the event from the request, not the user row", () => {
  const event = signupEventFromAuthHook(
    "usr_1",
    {
      path: "/callback/:id",
      params: { id: "github" },
      headers: new Headers({
        cookie: `convt_signup=${encodeURIComponent(JSON.stringify({ source: "/download" }))}`,
        referer: "https://github.com/login",
      }),
      body: { callbackURL: "/dashboard" },
      request: new Request("http://localhost:3999/api/auth/callback/github?code=x"),
    },
    "https://convt.app",
  );
  expect(event?.properties).toEqual({ signup_method: "github", source: "/download" });
  expect(event?.distinctId).toBe("usr_1");
});

test("the auth hook skips capture when the request opted out", () => {
  const ctx = {
    path: "/sign-in/email-otp",
    headers: new Headers({ dnt: "1" }),
    body: { callbackURL: "/dashboard" },
  };
  expect(signupEventFromAuthHook("usr_1", ctx, "https://convt.app")).toBeNull();
  expect(
    signupEventFromAuthHook(
      "usr_1",
      { ...ctx, headers: new Headers({ "sec-gpc": "1" }) },
      "https://convt.app",
    ),
  ).toBeNull();
  expect(
    signupEventFromAuthHook(
      "usr_1",
      { ...ctx, headers: new Headers({ cookie: "convt_analytics=off" }) },
      "https://convt.app",
    ),
  ).toBeNull();
});

test("captureEvent fails on a non-2xx PostHog response and accepts 2xx", async () => {
  const original = globalThis.fetch;
  globalThis.fetch = (async () => new Response("down", { status: 503 })) as unknown as typeof fetch;
  try {
    await expect(
      captureEvent(
        { key: "phc_test", host: "https://us.i.posthog.com" },
        { event: "user_signed_up", distinctId: "usr_1" },
      ),
    ).rejects.toThrow(/503/);
  } finally {
    globalThis.fetch = original;
  }
  globalThis.fetch = (async () => new Response("ok", { status: 200 })) as unknown as typeof fetch;
  try {
    await captureEvent(
      { key: "phc_test", host: "https://us.i.posthog.com" },
      { event: "user_signed_up", distinctId: "usr_1" },
    );
  } finally {
    globalThis.fetch = original;
  }
});

test("attribution sanitizes the landing path and UTM and refuses mailboxes", () => {
  const badCampaign = testMailbox("a", ["b", "test"].join("."));
  const url = new URL("https://convt.app/download?utm_source=ph&x=1");
  url.searchParams.set("utm_campaign", badCampaign);
  expect(attributionFromSearch(url)).toEqual({ source: "/download", utm_source: "ph" });
  expect(sanitizeAttribution({ source: "/pricing?token=abc", utm_source: "ok" })).toEqual({
    source: "/pricing",
    utm_source: "ok",
  });
  expect(parseAttributionCookie("other=1")).toBeNull();
});

test("a guide visit followed by its tagged download link keeps the path and the campaign", () => {
  const guide = nextAttribution(null, new URL("https://convt.app/guides/heic-to-jpg-mac"));
  expect(guide).toEqual({ source: "/guides/heic-to-jpg-mac" });
  // Signed out, /download redirects to sign-in before the page can read its tags.
  const signIn = new URL("https://convt.app/sign-in");
  signIn.searchParams.set(
    "redirect",
    "/download?utm_source=convt_guides&utm_medium=organic&utm_campaign=heic-to-jpg-mac",
  );
  const tagged = nextAttribution(guide, signIn);
  expect(tagged).toEqual({
    source: "/guides/heic-to-jpg-mac",
    utm_source: "convt_guides",
    utm_medium: "organic",
    utm_campaign: "heic-to-jpg-mac",
  });
  // The first campaign wins, and untagged pages change nothing.
  expect(
    nextAttribution(tagged, new URL("https://convt.app/download?utm_source=other")),
  ).toBeNull();
  expect(nextAttribution(guide, new URL("https://convt.app/pricing"))).toBeNull();
  // Only same-site paths count.
  const offsite = new URL("https://convt.app/sign-in?redirect=//evil.test/?utm_source=x");
  expect(nextAttribution(guide, offsite)).toBeNull();
});

test("identify sends only the user id and resets on sign-out or opt-out", () => {
  const calls: string[] = [];
  const client = {
    identify: (id: string) => calls.push(`identify:${id}`),
    reset: () => calls.push("reset"),
  };
  expect(syncIdentifiedUser(client, "usr_1", "on", null)).toBe("usr_1");
  expect(syncIdentifiedUser(client, "usr_1", "on", "usr_1")).toBe("usr_1");
  expect(syncIdentifiedUser(client, null, "on", "usr_1")).toBeNull();
  expect(syncIdentifiedUser(client, "usr_2", "browser", "usr_2")).toBeNull();
  expect(syncIdentifiedUser(client, "usr_3", "off", null)).toBeNull();
  expect(calls).toEqual(["identify:usr_1", "reset", "reset"]);
});

test("a claim sends purchases only for the licenses it attached", () => {
  const had = [{ plan: "desktop", orderId: "ord_1" }];
  const now = [...had, { plan: "desktop", orderId: "ord_2" }];
  expect(claimedPurchaseEvents("usr_1", had, now).map((e) => e.insertId)).toEqual([
    "license_purchased:desktop:ord_2",
  ]);
  expect(claimedPurchaseEvents("usr_1", [], had)).toHaveLength(1);
});
