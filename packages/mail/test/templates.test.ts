// Template snapshots and escaping. A changed snapshot means templateVersion must
// change too, so queued emails keep the bytes they were frozen with.

import { describe, expect, test } from "bun:test";

import {
  alertDigest,
  codeEmail,
  escapeHtml,
  licenseIssued,
  renewalFailed,
  templateVersion,
  trialEnding,
} from "../src";

const site = "https://convt.app";

describe("templates", () => {
  test("license_issued for Desktop", () => {
    const m = licenseIssued({
      product: "desktop",
      token: "eyJpZCI6ImxpY18xIn0.c2ln",
      updatesUntil: "2027-10-05",
      siteUrl: site,
      downloadUrl: `${site}/download/mac`,
    });
    expect(m.subject).toBe("Your convt Desktop license key");
    expect(m.text).toMatchSnapshot();
    expect(m.html).toContain("convt://activate?key=eyJpZCI6ImxpY18xIn0.c2ln");
    expect(m.text).toContain("lifetime desktop updates");
  });

  test("license_issued for Pro", () => {
    const m = licenseIssued({
      product: "pro",
      token: "a.b",
      updatesUntil: "2026-11-05",
      siteUrl: site,
      downloadUrl: `${site}/download/mac`,
    });
    expect(m.subject).toBe("Your convt Pro license key");
    expect(m.text).toMatchSnapshot();
  });

  test("trial_ending, renewal_failed and the digest", () => {
    expect(
      trialEnding({
        trialEndsAt: "2026-10-12T09:30:00Z",
        amountCents: 1200,
        interval: "month",
        siteUrl: site,
      }),
    ).toMatchSnapshot();
    expect(
      renewalFailed({ kind: "pro", periodStart: "2026-10-05T00:00:00Z", siteUrl: site }),
    ).toMatchSnapshot();
    expect(
      renewalFailed({ kind: "api", periodStart: "2026-10-05T00:00:00Z", siteUrl: site }).text,
    ).toContain("New API conversions are refused");
    expect(
      alertDigest({
        date: "2026-10-05",
        counts: { rejected_events: 1, dead_emails: 0 },
        items: [{ kind: "rejected_event", subject: "whe_1", detail: "unknown_price" }],
      }),
    ).toMatchSnapshot();
  });

  test("everything a template interpolates is escaped in HTML", () => {
    const m = alertDigest({
      date: "2026-10-05",
      counts: {},
      items: [{ kind: "<script>", subject: '"x"', detail: "a & b <img onerror=1>" }],
    });
    expect(m.html).not.toContain("<script>");
    expect(m.html).not.toContain("<img");
    expect(m.html).toContain("&#60;script&#62;");
    expect(escapeHtml(`<a href="x">'&`)).toBe("&#60;a href=&#34;x&#34;&#62;&#39;&#38;");
    const lic = licenseIssued({
      product: "desktop",
      token: 'x"><script>',
      updatesUntil: "2027-01-01",
      siteUrl: site,
      downloadUrl: site,
    });
    expect(lic.html).not.toContain("<script>");
  });

  test("no em dashes in any template", () => {
    const all = [
      licenseIssued({
        product: "pro",
        token: "t",
        updatesUntil: "2027-01-01",
        siteUrl: site,
        downloadUrl: site,
      }),
      trialEnding({
        trialEndsAt: "2026-10-12T00:00:00Z",
        amountCents: 9600,
        interval: "year",
        siteUrl: site,
      }),
      renewalFailed({ kind: "pro", periodStart: "2026-10-05T00:00:00Z", siteUrl: site }),
    ];
    for (const m of all) expect(`${m.subject}${m.text}`).not.toContain("—");
    expect(templateVersion).toBe(3);
  });
});

describe("code email", () => {
  // Built at runtime so no address appears in the source.
  const address = ["reader", "convt.test"].join("@");
  // The real link also carries the address; the template only passes it through.
  const link = `${site}/sign-in/verify#code=123456`;

  test("sign-in has the code in text and HTML, and the link", () => {
    const m = codeEmail({ kind: "sign-in", email: address, code: "123456", link, minutes: 15 });
    expect(m.subject).toBe("123456 is your convt sign-in code");
    expect(m.text).toMatchSnapshot();
    expect(m.html).toContain(">123456</p>");
    expect(m.html).toContain(`href="${escapeHtml(link)}"`);
    expect(m.html).toContain("expires in 15 minutes");
    expect(m.html).toContain('<meta name="color-scheme" content="light dark">');
  });

  test("confirmation codes name the address, escaped, and carry no link", () => {
    const hostile = ["<b>x", "convt.test"].join("@");
    const m = codeEmail({
      kind: "change-email",
      email: hostile,
      code: "654321",
      link,
      minutes: 15,
    });
    expect(m.subject).toBe("654321 is your convt confirmation code");
    expect(m.text).toContain(`confirm ${hostile} as the email`);
    expect(m.html).not.toContain("<b>x");
    expect(m.html).not.toContain("sign-in/verify");
    expect(
      codeEmail({ kind: "confirm", email: address, code: "1", link, minutes: 5 }).text,
    ).toContain(`confirm ${address} for your convt account`);
  });
});
