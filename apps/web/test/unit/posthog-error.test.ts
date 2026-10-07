import { expect, test } from "bun:test";

import { requestAllowsServerExceptions, scrub } from "../../src/server/posthog-scrub";

test("scrubs paths, emails, license keys, and credentials", () => {
  const mailbox = ["user", "gmail.com"].join("@");
  const value = scrub(
    `/Users/alice/input.pdf ${mailbox} license_key=cvt_PROD_12345678 token=secret`,
  );
  expect(value).toBe("<path> <email> <credential>=<redacted> <credential>=<redacted>");
  expect(value).not.toContain("alice");
  expect(value).not.toContain("gmail.com");
  expect(value).not.toContain("cvt_PROD_12345678");
  expect(value).not.toContain("secret");
});

test("honors request privacy signals", () => {
  expect(
    requestAllowsServerExceptions(
      new Request("https://convt.app", { headers: { "Sec-GPC": "1" } }),
    ),
  ).toBe(false);
  expect(
    requestAllowsServerExceptions(new Request("https://convt.app", { headers: { DNT: "1" } })),
  ).toBe(false);
  expect(
    requestAllowsServerExceptions(
      new Request("https://convt.app", { headers: { Cookie: "convt:analytics-opt-out=1" } }),
    ),
  ).toBe(false);
  expect(requestAllowsServerExceptions(new Request("https://convt.app"))).toBe(true);
});
