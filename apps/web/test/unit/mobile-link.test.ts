// The mobile download-link capture: who counts as mobile, address validation, the
// limits, the double-tap guard and the email. The database half of the limits runs
// against Postgres in test/integration/mobile-link.test.ts.

import { describe, expect, spyOn, test } from "bun:test";
import { loadCatalog } from "@convt/billing/catalog";

import { activeOffer, launchOffer } from "../../src/lib/launch-offer";
import { isMobileDevice, isMobileUserAgent, normalizeEmail } from "../../src/lib/mobile";
import type { MailMessage } from "../../src/server/mail";
import {
  mobileLinkLimits,
  parseMobileLinkInput,
  requestMobileLink,
  type MobileLinkDeps,
} from "../../src/server/mobile-link";

const ua = {
  iphone:
    "Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Mobile/15E148 Safari/604.1",
  android:
    "Mozilla/5.0 (Linux; Android 15; Pixel 9) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/129.0.0.0 Mobile Safari/537.36",
  mac: "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Safari/605.1.15",
  windows:
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/129.0.0.0 Safari/537.36",
};

describe("mobile detection", () => {
  test("phones and tablets at narrow widths are mobile", () => {
    expect(isMobileDevice({ userAgent: ua.iphone, maxTouchPoints: 5, viewportWidth: 390 })).toBe(
      true,
    );
    expect(isMobileDevice({ userAgent: ua.android, maxTouchPoints: 5, viewportWidth: 412 })).toBe(
      true,
    );
    // iPadOS Safari sends a Mac user agent; touch gives it away.
    expect(isMobileDevice({ userAgent: ua.mac, maxTouchPoints: 5, viewportWidth: 820 })).toBe(true);
  });

  test("a desktop browser in a narrow window keeps the normal download", () => {
    expect(isMobileDevice({ userAgent: ua.mac, maxTouchPoints: 0, viewportWidth: 390 })).toBe(
      false,
    );
    expect(isMobileDevice({ userAgent: ua.windows, maxTouchPoints: 0, viewportWidth: 390 })).toBe(
      false,
    );
    // A Windows touch laptop is still a computer.
    expect(isMobileDevice({ userAgent: ua.windows, maxTouchPoints: 10, viewportWidth: 390 })).toBe(
      false,
    );
  });

  test("a tablet at a wide viewport gets the normal page", () => {
    expect(isMobileDevice({ userAgent: ua.mac, maxTouchPoints: 5, viewportWidth: 1366 })).toBe(
      false,
    );
  });

  test("the server, without touch information, sees only the user agent", () => {
    expect(isMobileUserAgent(ua.iphone)).toBe(true);
    expect(isMobileUserAgent(ua.mac)).toBe(false);
  });
});

describe("validation", () => {
  test("accepts ordinary addresses, trimmed and lowercased", () => {
    expect(normalizeEmail("  Someone@Example.COM ")).toBe("someone@example.com");
    expect(normalizeEmail("a.b+tag@mail.example.co.uk")).toBe("a.b+tag@mail.example.co.uk");
  });

  test("rejects what can't be an address", () => {
    for (const bad of [
      "",
      "someone",
      "someone@",
      "@example.com",
      "someone@example",
      "some one@example.com",
      "a@b@example.com",
      "someone@.example.com",
      "someone@example..com",
      "<a@example.com>",
      `${"a".repeat(65)}@example.com`,
      `a@${"b".repeat(250)}.com`,
      42,
      null,
    ])
      expect(normalizeEmail(bad)).toBeNull();
  });

  test("the request needs a string email and a known source", () => {
    expect(parseMobileLinkInput({ email: "a@example.com", source: "landing" })).toEqual({
      email: "a@example.com",
      source: "landing",
    });
    expect(() => parseMobileLinkInput({ email: "a@example.com", source: "elsewhere" })).toThrow();
    expect(() => parseMobileLinkInput({ email: 1, source: "download" })).toThrow();
    expect(() => parseMobileLinkInput({ email: "x".repeat(1001), source: "download" })).toThrow();
    expect(() => parseMobileLinkInput(null)).toThrow();
  });
});

/** consumeSendBucket and releaseSendBucket over a Map, with a clock the test moves. */
function fakeDeps(options: { now?: Date; failSends?: number } = {}) {
  const buckets = new Map<string, { count: number; expires: number }>();
  const sent: MailMessage[] = [];
  let fails = options.failSends ?? 0;
  const deps: MobileLinkDeps & { sent: MailMessage[]; buckets: typeof buckets } = {
    now: options.now ?? new Date("2026-10-07T12:00:00Z"),
    siteUrl: "https://convt.app",
    sent,
    buckets,
    async consume(key, windowMs) {
      const t = deps.now.getTime();
      const b = buckets.get(key);
      if (!b || b.expires <= t) {
        buckets.set(key, { count: 1, expires: t + windowMs });
        return 1;
      }
      b.count += 1;
      return b.count;
    },
    async release(key) {
      const b = buckets.get(key);
      if (b) b.expires = deps.now.getTime();
    },
    async send(message) {
      if (fails > 0) {
        fails -= 1;
        throw new Error("sequenzy answered 503");
      }
      sent.push(message);
    },
  };
  return deps;
}

const later = (d: Date, ms: number) => new Date(d.getTime() + ms);

describe("requestMobileLink", () => {
  test("a valid address gets one email with the download link and the offer", async () => {
    const deps = fakeDeps();
    expect(await requestMobileLink({ email: " Me@Example.com", ip: "1.1.1.1" }, deps)).toEqual({
      ok: true,
    });
    expect(deps.sent).toHaveLength(1);
    const [m] = deps.sent;
    expect(m.to).toBe("me@example.com");
    expect(m.subject).toBe("Your convt download link");
    expect(m.text).toContain("https://convt.app/download");
    expect(m.html).toContain('href="https://convt.app/download"');
    expect(m.text).toContain(`Use code ${launchOffer.code} at checkout`);
    expect(m.text).toContain(launchOffer.terms);
    expect(m.idempotencyKey).toMatch(/^mobile-link-[0-9a-f]{32}-\d+$/);
    expect(m.idempotencyKey).not.toContain("example");
    expect(`${m.subject}${m.text}${m.html}`).not.toContain("—");
  });

  test("the address is never what the buckets are keyed by", async () => {
    const deps = fakeDeps();
    await requestMobileLink({ email: "me@example.com", ip: "1.1.1.1" }, deps);
    for (const key of deps.buckets.keys()) expect(key).not.toContain("example");
  });

  test("an invalid address sends nothing and counts nothing", async () => {
    const deps = fakeDeps();
    expect(await requestMobileLink({ email: "me@", ip: "1.1.1.1" }, deps)).toEqual({
      ok: false,
      error: "invalid_email",
    });
    expect(deps.sent).toHaveLength(0);
    expect(deps.buckets.size).toBe(0);
  });

  test("a double tap sends one email", async () => {
    const deps = fakeDeps();
    const input = { email: "me@example.com", ip: "1.1.1.1" };
    const results = await Promise.all([
      requestMobileLink(input, deps),
      requestMobileLink(input, deps),
    ]);
    expect(results).toEqual([{ ok: true }, { ok: true }]);
    expect(deps.sent).toHaveLength(1);
  });

  test("past the per-address limit: too many tries and no more email", async () => {
    const deps = fakeDeps();
    const input = { email: "me@example.com", ip: "1.1.1.1" };
    for (let i = 0; i < mobileLinkLimits.email.max; i++) {
      expect(await requestMobileLink(input, deps)).toEqual({ ok: true });
      deps.now = later(deps.now, mobileLinkLimits.duplicateWindowMs);
    }
    expect(deps.sent).toHaveLength(mobileLinkLimits.email.max);
    expect(await requestMobileLink(input, deps)).toEqual({ ok: false, error: "too_many" });
    // An immediate retry is refused too, not mistaken for an answered double tap.
    expect(await requestMobileLink(input, deps)).toEqual({ ok: false, error: "too_many" });
    expect(deps.sent).toHaveLength(mobileLinkLimits.email.max);
    // A new window allows it again.
    deps.now = later(deps.now, mobileLinkLimits.email.windowMs);
    expect(await requestMobileLink(input, deps)).toEqual({ ok: true });
  });

  test("past the per-IP limit: too many tries, whatever the address", async () => {
    const deps = fakeDeps();
    for (let i = 0; i < mobileLinkLimits.ip.max; i++)
      expect(await requestMobileLink({ email: `u${i}@example.com`, ip: "2.2.2.2" }, deps)).toEqual({
        ok: true,
      });
    expect(await requestMobileLink({ email: "new@example.com", ip: "2.2.2.2" }, deps)).toEqual({
      ok: false,
      error: "too_many",
    });
    expect(deps.sent).toHaveLength(mobileLinkLimits.ip.max);
    expect(await requestMobileLink({ email: "new@example.com", ip: "3.3.3.3" }, deps)).toEqual({
      ok: true,
    });
  });

  test("a failed send says so, logs no address, and a retry sends", async () => {
    const deps = fakeDeps({ failSends: 1 });
    const errors = spyOn(console, "error").mockImplementation(() => {});
    try {
      const input = { email: "me@example.com", ip: "1.1.1.1" };
      expect(await requestMobileLink(input, deps)).toEqual({ ok: false, error: "send_failed" });
      expect(JSON.stringify(errors.mock.calls)).not.toContain("example.com");
      expect(await requestMobileLink(input, deps)).toEqual({ ok: true });
      expect(deps.sent).toHaveLength(1);
    } finally {
      errors.mockRestore();
    }
  });

  test("after the offer ends, the email has no discount", async () => {
    const deps = fakeDeps({ now: launchOffer.endsAt });
    await requestMobileLink({ email: "me@example.com", ip: "1.1.1.1" }, deps);
    expect(deps.sent[0].text).not.toContain(launchOffer.code);
    expect(deps.sent[0].text).toContain("https://convt.app/download");
  });
});

describe("launch offer", () => {
  test("runs through 31 October 2026, Pacific time", () => {
    expect(activeOffer(new Date("2026-10-31T23:59:00-07:00"))).toBe(launchOffer);
    expect(activeOffer(new Date("2026-11-01T00:00:00-07:00"))).toBeNull();
  });

  test("its code is one billing accepts in every environment", () => {
    for (const env of ["local", "sandbox", "production"] as const) {
      const codes = Object.values(loadCatalog(env).discounts).map((d) => d.code);
      expect(codes).toContain(launchOffer.code);
    }
  });

  test("its text has no em dashes", () => {
    expect(`${launchOffer.terms}${launchOffer.endsLabel}`).not.toContain("—");
  });
});
