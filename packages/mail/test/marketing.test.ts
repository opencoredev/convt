// Marketing templates: every one carries the unsubscribe footer, and none of the
// transactional templates do. A changed snapshot needs marketingTemplateVersion
// bumped with it.

import { describe, expect, test } from "bun:test";

import {
  licenseIssued,
  marketingTemplates,
  marketingTemplateVersion,
  preferencesTag,
  renderProductUpdate,
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
      "lifetime-free",
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
    expect(marketingTemplateVersion).toBe(6);
    for (const [name, email] of Object.entries(all)) {
      expect({ name, subject: email.subject, text: email.text }).toMatchSnapshot();
    }
    expect(all["welcome-free"].html).toMatchSnapshot();
  });

  test("lifecycle plain text keeps the complete approved body", () => {
    expect(all["welcome-free"].text).toContain("hey {{firstName|there}},");
    expect(all["welcome-free"].text).toContain("thanks for giving it a try.");
    expect(all["welcome-free"].text).toContain("https://convt.app/download");
    expect(all["welcome-free"].text).toContain("hit reply and tell me.");
    expect(all["welcome-paid"].text).toContain("thank you for buying it.");
    expect(all["welcome-paid"].text).toContain("https://convt.app/download");
    expect(all["welcome-paid"].text).toContain("not a support queue.");
    expect(all.lifetime.text).toContain("desktop licenses are now lifetime");
    expect(all.lifetime.text).toContain("same key, no renewal, nothing to do.");
    expect(all.lifetime.text).toContain("i'd love to hear it. just reply.");
    expect(all["product-update"].text).toContain("reply and let me know.");
    for (const email of [
      all["welcome-free"],
      all["welcome-paid"],
      all.lifetime,
      all["product-update"],
    ]) {
      expect(email.text.startsWith("hey {{firstName|there}},")).toBe(true);
      expect(email.text).toContain("\n\nleo\n");
      expect(email.text).toContain(unsubscribeTag);
      expect(email.text).toContain(preferencesTag);
      expect(email.text).toContain("{{postalAddress}}");
    }
  });

  test("each letter asks for a reply in its own words", () => {
    const closings = (
      ["welcome-free", "welcome-paid", "lifetime", "lifetime-free", "product-update"] as const
    ).map((name) => all[name].text.split("\n\nleo\n")[0]?.split("\n\n").at(-1));
    expect(new Set(closings).size).toBe(closings.length);
  });

  test("lifecycle layout is fluid and responsive", () => {
    for (const name of [
      "welcome-free",
      "welcome-paid",
      "lifetime",
      "lifetime-free",
      "product-update",
    ] as const) {
      expect(all[name].html).toContain('class="wrap"');
      expect(all[name].html).toContain("max-width:620px");
      expect(all[name].html).toContain("width:100%!important");
      expect(all[name].html).toContain("padding-left:24px!important");
    }
  });

  test("the lifetime announcement promises nothing but lifetime updates", () => {
    const t = all.lifetime.text;
    expect(all.lifetime.html).toContain("desktop licenses are now lifetime");
    expect(all.lifetime.html).toContain("desktop licenses are now lifetime");
    expect(t).not.toMatch(/refund/i);
    // Buyers already own it; only accounts that never bought get the offer.
    expect(t).not.toContain("30%");
  });

  test("campaign content is escaped; merge tags stay intact", () => {
    const email = renderProductUpdate({
      siteUrl: "https://convt.app",
      subject: "A & B",
      preheader: "<b>p</b>",
      intro: "<script>alert(1)</script>",
      items: [{ title: '"quoted"', body: "it's" }],
      cta: { href: 'https://convt.app/?a=1&b="2"', label: "Go" },
      outro: "reply",
    });
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
