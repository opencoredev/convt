import { describe, expect, test } from "bun:test";

import { identities } from "@convt/oauth-mock/identities";

import { safeRedirect } from "../../src/lib/safe-redirect";
import {
  googleEmailIsAuthoritative,
  githubEmailIsAuthoritative,
} from "../../src/server/authoritative";
import { readEnv } from "../../src/server/env";
import { isSameOriginRequest } from "../../src/server/origin";
import { redact, redactText } from "../../src/server/redact";

const origin = "https://convt.app";

describe("safeRedirect", () => {
  const rejected = [
    "//evil.example",
    "/\\evil.example",
    "/%5Cevil.example",
    "%2F%2Fevil.example",
    "/%2F/evil.example",
    "https://evil.example",
    "https://convt.app.evil.example/x",
    "javascript:alert(1)",
    "/dash\tboard",
    "/dash\nboard",
    "/dash%0Aboard",
    "/dash%09board",
    "",
    "dashboard",
    "%E0%A4%A",
    "/x/..//evil.example",
    "/.//evil.example",
    "/%2e//evil.example",
    "/x/%2e%2e//evil.example",
    "/./%5Cevil.example",
  ];
  for (const value of rejected) {
    test(`rejects ${JSON.stringify(value)}`, () =>
      expect(safeRedirect(value, origin)).toBe("/dashboard"));
  }
  test("rejects non-strings", () => {
    expect(safeRedirect(undefined, origin)).toBe("/dashboard");
    expect(safeRedirect(["/account"], origin)).toBe("/dashboard");
  });
  test("keeps a normal path with a query and hash", () => {
    expect(safeRedirect("/dashboard/billing?tab=invoices#latest", origin)).toBe(
      "/dashboard/billing?tab=invoices#latest",
    );
    expect(safeRedirect("/account", origin)).toBe("/account");
  });
  test("an encoded slash after dot segments stays a path on this site", () => {
    expect(safeRedirect("/x/../%2Fevil.example", origin)).toBe("/%2Fevil.example");
  });
  test("an absolute URL on this origin keeps only its path", () => {
    expect(safeRedirect("https://convt.app/account", origin)).toBe("/dashboard");
  });
});

describe("Origin check for state-changing requests", () => {
  const req = (method: string, headers: Record<string, string>) =>
    new Request("https://convt.app/_serverFn/x", { method, headers });
  test("GET and HEAD pass", () => {
    expect(isSameOriginRequest(req("GET", {}), origin)).toBe(true);
    expect(isSameOriginRequest(req("HEAD", {}), origin)).toBe(true);
  });
  test("POST needs this origin or Sec-Fetch-Site: same-origin", () => {
    expect(isSameOriginRequest(req("POST", { origin }), origin)).toBe(true);
    expect(isSameOriginRequest(req("POST", { "sec-fetch-site": "same-origin" }), origin)).toBe(
      true,
    );
    expect(isSameOriginRequest(req("POST", {}), origin)).toBe(false);
    expect(isSameOriginRequest(req("POST", { origin: "https://evil.example" }), origin)).toBe(
      false,
    );
    expect(isSameOriginRequest(req("POST", { origin: "null" }), origin)).toBe(false);
    expect(isSameOriginRequest(req("POST", { "sec-fetch-site": "cross-site" }), origin)).toBe(
      false,
    );
    // A present but wrong Origin is not rescued by Sec-Fetch-Site.
    expect(
      isSameOriginRequest(
        req("PUT", { origin: "https://evil.example", "sec-fetch-site": "same-origin" }),
        origin,
      ),
    ).toBe(false);
  });
});

describe("log redactor", () => {
  test("masks codes in text and fields", () => {
    expect(redactText("verify 123456 for a@b.c")).toBe("verify [redacted] for a@b.c");
    expect(redactText("/sign-in/verify#email=a%40b.c&code=123456")).toBe(
      "/sign-in/verify#email=a%40b.c&code=[redacted]",
    );
    expect(redactText('{"email":"a@b.c","otp":"654321"}')).not.toContain("654321");
    expect(redact({ body: { email: "a@b.c", otp: "111222", nested: { code: "x" } } })).toEqual({
      body: { email: "a@b.c", otp: "[redacted]", nested: { code: "[redacted]" } },
    });
    expect((redact(new Error("bad code 998877")) as Error).message).toBe("bad code [redacted]");
  });
  test("leaves longer numbers alone", () => {
    expect(redactText("order 1234567 and 12345")).toBe("order 1234567 and 12345");
  });
});

describe("authoritative email per mock identity", () => {
  const expected: Record<string, boolean> = {
    "google-gmail": true,
    "google-workspace": true,
    "google-thirdparty": false,
    "google-unverified": false,
    "github-verified": false,
    "github-public-differs": false,
    "github-no-email": false,
    "github-pro": false,
  };
  for (const [name, identity] of Object.entries(identities)) {
    test(name, () => {
      const result =
        identity.provider === "google"
          ? googleEmailIsAuthoritative({
              email: identity.email,
              email_verified: identity.email_verified,
              hd: identity.hd,
            })
          : githubEmailIsAuthoritative();
      expect(result).toBe(expected[name]);
    });
  }
  test("hd must match the address's domain", () => {
    expect(
      googleEmailIsAuthoritative({
        email: "a@other.test",
        email_verified: true,
        hd: "workspace.test",
      }),
    ).toBe(false);
    expect(googleEmailIsAuthoritative({ email: "A@GoogleMail.com", email_verified: true })).toBe(
      true,
    );
    expect(googleEmailIsAuthoritative({ email: "a@gmail.com", email_verified: false })).toBe(false);
  });
});

describe("readEnv", () => {
  const base = { BETTER_AUTH_URL: "https://convt.app", BETTER_AUTH_SECRET: "x".repeat(64) };
  test("production refuses the dev mail sinks and the OAuth mock", () => {
    expect(() =>
      readEnv({
        ...base,
        ENV: "production",
        MAIL_TRANSPORT: "mailpit",
        MAILPIT_URL: "http://127.0.0.1:8025",
      }),
    ).toThrow(/refused/);
    expect(() => readEnv({ ...base, ENV: "production", MAIL_TRANSPORT: "log" })).toThrow(/refused/);
    expect(() =>
      readEnv({ ...base, ENV: "production", OAUTH_MOCK_URL: "http://127.0.0.1:4100" }),
    ).toThrow(/refused/);
    expect(readEnv({ ...base, ENV: "production" }).mail.transport).toBe("resend");
    expect(() => readEnv({ ...base, BETTER_AUTH_URL: "http://convt.app" })).toThrow(/https/);
  });
  test("development refuses non-loopback sinks", () => {
    const dev = { ...base, ENV: "development", BETTER_AUTH_URL: "http://localhost:3000" };
    expect(() =>
      readEnv({ ...dev, MAIL_TRANSPORT: "mailpit", MAILPIT_URL: "http://mail.example:8025" }),
    ).toThrow(/loopback/);
    expect(() => readEnv({ ...dev, OAUTH_MOCK_URL: "http://10.0.0.5:4100" })).toThrow(/loopback/);
    const ok = readEnv({
      ...dev,
      MAIL_TRANSPORT: "mailpit",
      MAILPIT_URL: "http://127.0.0.1:8025",
      OAUTH_MOCK_URL: "http://127.0.0.1:4100",
    });
    expect(ok.oauthMock?.url).toBe("http://127.0.0.1:4100");
  });
  test("a missing or short secret is refused", () => {
    expect(() => readEnv({ BETTER_AUTH_URL: "https://convt.app" })).toThrow(/SECRET/);
    expect(() => readEnv({ ...base, BETTER_AUTH_SECRET: "short" })).toThrow(/SECRET/);
  });
});
