import { expect, test } from "bun:test";

import { authSearch, downloadGate, isNewAccount, landingAfterSignIn } from "../../src/lib/sign-in";

const origin = "https://convt.app";
const now = Date.parse("2026-10-08T12:00:00Z");

test("next is an alias for redirect, and redirect wins when both are set", () => {
  expect(authSearch({ next: "/download" })).toEqual({ redirect: "/download" });
  expect(authSearch({ next: "/download", redirect: "/account" })).toEqual({ redirect: "/account" });
  expect(authSearch({ next: "" })).toEqual({});
});

test("an account created in the last minutes is new; an older or missing one is not", () => {
  expect(isNewAccount(new Date(now - 30_000).toISOString(), now)).toBe(true);
  expect(isNewAccount(new Date(now - 60 * 60_000).toISOString(), now)).toBe(false);
  expect(isNewAccount(undefined, now)).toBe(false);
  expect(isNewAccount("not a date", now)).toBe(false);
});

test("a new account lands on /download, a returning one on the dashboard", () => {
  expect(landingAfterSignIn({ redirect: undefined, newAccount: true, origin })).toBe("/download");
  expect(landingAfterSignIn({ redirect: undefined, newAccount: false, origin })).toBe("/dashboard");
});

test("a page that asked to come back wins, and only same-origin paths count", () => {
  expect(landingAfterSignIn({ redirect: "/device?state=x", newAccount: true, origin })).toBe(
    "/device?state=x",
  );
  expect(landingAfterSignIn({ redirect: "https://evil.example/x", newAccount: true, origin })).toBe(
    "/dashboard",
  );
  expect(landingAfterSignIn({ redirect: "//evil.example", newAccount: false, origin })).toBe(
    "/dashboard",
  );
});

test("/download sends signed-out visitors to sign-in with next, and unverified ones to confirm", () => {
  expect(downloadGate(null, "/download")).toBe("/sign-in?next=%2Fdownload");
  expect(downloadGate(null, "/download?os=linux")).toBe("/sign-in?next=%2Fdownload%3Fos%3Dlinux");
  expect(downloadGate({ emailVerified: false }, "/download")).toBe(
    "/sign-in/verify-email?redirect=%2Fdownload",
  );
  expect(downloadGate({ emailVerified: true }, "/download")).toBeNull();
});
