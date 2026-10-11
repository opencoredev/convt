// Marketing templates: every one carries the unsubscribe footer, and none of the
// transactional templates do. A changed snapshot needs marketingTemplateVersion
// bumped with it.

import { describe, expect, test } from "bun:test";

import {
  licenseIssued,
  marketingTemplates,
  marketingTemplateVersion,
  preferencesTag,
  productUpdateEmail,
  renderMarketing,
  renewalFailed,
  welcomeEmailForSegment,
  trialEnding,
  unsubscribeTag,
} from "../src";

const footer = { siteUrl: "https://convt.app/", postalAddress: "1 Example Street, Test City" };

describe("marketing templates", () => {
  const all = marketingTemplates(footer);

  test("each one renders with the footer, both merge tags and the address", () => {
    expect(Object.keys(all).sort()).toEqual([
      "lifetime",
      "pro-trial",
      "product-update",
      "welcome-free",
      "welcome-paid",
    ]);
    for (const email of Object.values(all)) {
      for (const part of [email.html, email.text]) {
        expect(part).toContain(unsubscribeTag);
        expect(part).toContain(preferencesTag);
        expect(part).toContain(
          email.html.includes("{{postalAddress}}")
            ? "{{postalAddress}}"
            : "1 Example Street, Test City",
        );
      }
      expect(email.html).toContain(`href="${unsubscribeTag}"`);
      if (email.html.includes("convt-lockup-on-light.png")) {
        expect(email.html).toContain("https://convt.app/brand/convt-lockup-on-dark.png");
      } else {
        expect(email.html).toContain("https://convt.app/brand/convt-app-icon.png");
      }
      expect(email.html).not.toContain("convt.app//");
      expect(email.preheader.length).toBeGreaterThan(10);
    }
  });

  test("selects the welcome template by segment", () => {
    expect(welcomeEmailForSegment("https://convt.app", "free").subject).toBe("welcome to convt");
    expect(welcomeEmailForSegment("https://convt.app", "paid").subject).toBe(
      "thanks for buying convt",
    );
    expect(all["welcome-paid"].html).toContain("<title>thank you</title>");
  });

  test("snapshots", () => {
    expect(marketingTemplateVersion).toBe(3);
    for (const [name, email] of Object.entries(all)) {
      expect({ name, subject: email.subject, text: email.text }).toMatchSnapshot();
    }
    expect(all["welcome-free"].html).toMatchSnapshot();
  });

  test("the lifetime announcement promises nothing but lifetime updates", () => {
    const t = all.lifetime.text;
    expect(all.lifetime.html).toContain("desktop licenses are now lifetime");
    expect(all.lifetime.html).toContain("desktop licenses are now lifetime");
    expect(t).not.toMatch(/refund/i);
  });

  test("campaign content is escaped; merge tags stay intact", () => {
    const email = renderMarketing(
      productUpdateEmail({
        siteUrl: "https://convt.app",
        subject: "x",
        preheader: "<b>p</b>",
        heading: "A & B",
        intro: "<script>alert(1)</script>",
        items: [{ title: '"quoted"', body: "it's" }],
        cta: { href: 'https://convt.app/?a=1&b="2"', label: "Go" },
      }),
      footer,
    );
    expect(email.html).not.toContain("<script>");
    expect(email.html).toContain("&#60;script&#62;");
    expect(email.html).toContain("A &#38; B");
    expect(email.html).toContain("https://convt.app/?a=1&#38;b=&#34;2&#34;");
    expect(email.html).toContain("{{unsubscribeUrl}}");
  });

  test("transactional templates carry no marketing footer", () => {
    const site = "https://convt.app";
    for (const m of [
      licenseIssued({
        product: "desktop",
        token: "a.b",
        updatesUntil: "9999-12-31",
        siteUrl: site,
        downloadUrl: site,
      }),
      trialEnding({
        trialEndsAt: "2026-10-12T00:00:00Z",
        amountCents: 1200,
        interval: "month",
        siteUrl: site,
      }),
      renewalFailed({ kind: "pro", periodStart: "2026-10-05T00:00:00Z", siteUrl: site }),
    ]) {
      expect(m.html).not.toContain("{{");
      expect(m.html.toLowerCase()).not.toContain("unsubscribe");
    }
  });
});
