import { expect, test } from "bun:test";

import {
  attributionFromSearch,
  parseAttributionCookie,
  sanitizeAttribution,
  signupMethodFromAuthPath,
} from "../../src/lib/analytics-attribution";
import { syncIdentifiedUser } from "../../src/lib/identify-user";
import {
  signupEventFromAuthHook,
  signupProperties,
  userSignedUpEvent,
  withoutPii,
} from "../../src/server/analytics";

test("signup method comes from the auth path, never from an email", () => {
  expect(signupMethodFromAuthPath("/sign-in/email-otp")).toBe("email");
  expect(signupMethodFromAuthPath("/callback/github")).toBe("github");
  expect(signupMethodFromAuthPath("/oauth2/callback/google")).toBe("google");
  expect(signupMethodFromAuthPath("/get-session")).toBe("unknown");
});

test("signup properties keep the method, source and UTM and drop PII", () => {
  const props = signupProperties({
    path: "/sign-in/email-otp",
    cookie: `convt_signup=${encodeURIComponent(
      JSON.stringify({
        source: "/pricing",
        utm_source: "twitter",
        utm_medium: "social",
        email: "leo@convt.test",
      }),
    )}`,
    callbackURL: "/dashboard?email=hidden@convt.test",
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
  expect(props).not.toHaveProperty("email");
});

test("withoutPii strips emails, names and overlong strings", () => {
  expect(
    withoutPii({
      signup_method: "github",
      email: "a@b.test",
      name: "Leo",
      note: "ok",
      leaked: "user@convt.test",
    }),
  ).toEqual({ signup_method: "github", note: "ok" });
});

test("user_signed_up uses the user id and a stable insert id", () => {
  const event = userSignedUpEvent("usr_abc", { signup_method: "google", email: "nope@x.test" });
  expect(event.event).toBe("user_signed_up");
  expect(event.distinctId).toBe("usr_abc");
  expect(event.insertId).toBe("user_signed_up:usr_abc");
  expect(event.properties).toEqual({ signup_method: "google" });
});

test("the auth hook builds the event from the request, not the user row", () => {
  const event = signupEventFromAuthHook(
    "usr_1",
    {
      path: "/callback/github",
      headers: new Headers({
        cookie: `convt_signup=${encodeURIComponent(JSON.stringify({ source: "/download" }))}`,
        referer: "https://github.com/login",
      }),
      body: { callbackURL: "/dashboard" },
    },
    "https://convt.app",
  );
  expect(event.properties).toEqual({ signup_method: "github", source: "/download" });
});

test("attribution sanitizes the landing path and UTM and refuses emails", () => {
  const url = new URL("https://convt.app/download?utm_source=ph&utm_campaign=a@b.test&x=1");
  expect(attributionFromSearch(url)).toEqual({ source: "/download", utm_source: "ph" });
  expect(sanitizeAttribution({ source: "/pricing?token=abc", utm_source: "ok" })).toEqual({
    source: "/pricing",
    utm_source: "ok",
  });
  expect(parseAttributionCookie("other=1")).toBeNull();
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
