import { expect, test } from "bun:test";

import {
  authSearch,
  downloadEntryHref,
  downloadGate,
  isNewAccount,
  landingAfterSignIn,
  rememberSignInRedirect,
  takeSignInRedirect,
  type RedirectStore,
} from "../../src/lib/sign-in";

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

test("/download sends signed-out visitors to sign-in with redirect, and unverified ones to confirm", () => {
  expect(downloadGate(null, "/download")).toBe("/sign-in?redirect=%2Fdownload");
  expect(downloadGate(null, "/download?os=linux")).toBe(
    "/sign-in?redirect=%2Fdownload%3Fos%3Dlinux",
  );
  expect(downloadGate({ emailVerified: false }, "/download")).toBe(
    "/sign-in/verify-email?redirect=%2Fdownload",
  );
  expect(downloadGate({ emailVerified: true }, "/download")).toBeNull();
});

test("Get convt goes straight to sign-in with one redirect when signed out", () => {
  expect(downloadEntryHref(false)).toBe("/sign-in?redirect=%2Fdownload");
  expect(downloadEntryHref(true)).toBe("/download");
});

function memoryStore(): RedirectStore & { size: () => number } {
  const items = new Map<string, string>();
  return {
    getItem: (key) => items.get(key) ?? null,
    setItem: (key, value) => void items.set(key, value),
    removeItem: (key) => void items.delete(key),
    size: () => items.size,
  };
}

test("the emailed link returns to the page that asked, for that address, once", () => {
  // Built at run time: the repository holds no address literals.
  const address = ["Returning", "convt.test"].join("@");
  const store = memoryStore();
  rememberSignInRedirect(store, address, "/download", now);
  expect(takeSignInRedirect(store, ["someone", "convt.test"].join("@"), now)).toBeUndefined();
  expect(takeSignInRedirect(store, ` ${address.toLowerCase()} `, now + 60_000)).toBe("/download");
  expect(takeSignInRedirect(store, address, now + 60_000)).toBeUndefined();
  expect(store.size()).toBe(0);
});

test("an emailed-link redirect lasts as long as the code, and a plain sign-in clears it", () => {
  const address = ["returning", "convt.test"].join("@");
  const store = memoryStore();
  rememberSignInRedirect(store, address, "/download", now);
  expect(takeSignInRedirect(store, address, now + 16 * 60_000)).toBeUndefined();
  rememberSignInRedirect(store, address, "/download", now);
  rememberSignInRedirect(store, address, undefined, now);
  expect(takeSignInRedirect(store, address, now)).toBeUndefined();
  store.setItem("convt.sign-in-redirect", "{not json");
  expect(takeSignInRedirect(store, address, now)).toBeUndefined();
});

test("storage that refuses never breaks sign-in", () => {
  const refusing: RedirectStore = {
    getItem: () => {
      throw new Error("denied");
    },
    setItem: () => {
      throw new Error("denied");
    },
    removeItem: () => {
      throw new Error("denied");
    },
  };
  const address = ["returning", "convt.test"].join("@");
  expect(() => rememberSignInRedirect(refusing, address, "/download")).not.toThrow();
  expect(takeSignInRedirect(refusing, address)).toBeUndefined();
});
