import { expect, test } from "bun:test";
import { renderToStaticMarkup } from "react-dom/server";

import { CallToAction } from "../../src/components/landing/closing";
import { Hero } from "../../src/components/landing/hero";
import { Pricing } from "../../src/components/landing/pricing";

test("flipping launch enables Desktop and Desktop + Cloud without enabling API sales", () => {
  const before = renderToStaticMarkup(<Pricing sales="desktop" launched={false} />);
  expect(before).not.toContain('href="/checkout/desktop"');
  expect(before).not.toContain('href="/checkout/bundle"');
  const after = renderToStaticMarkup(<Pricing sales="desktop" launched />);
  expect(after).toContain('href="/checkout/desktop"');
  expect(after).toContain('href="/checkout/bundle"');
  expect(after).not.toContain("Desktop + Cloud is not on sale yet");
  expect(after).toContain("API is not on sale yet");
  expect(after).toContain("macOS, Windows and Linux");
  expect(after).not.toContain("Linux now");
  const cloud = renderToStaticMarkup(<Pricing sales="all" launched />);
  expect(cloud).not.toContain("API is not on sale yet");
});

test("pricing sells once: $39 and $69, no Pro checkout, no trial, no billing toggle", () => {
  const html = renderToStaticMarkup(<Pricing sales="all" launched />);
  expect(html).toContain("$39");
  expect(html).toContain("$69");
  expect(html).not.toContain("/checkout/pro");
  expect(html).not.toMatch(/trial|per month|\$29|\$12/i);
  expect(html).not.toContain("Billing period");
});

test("public copy around pricing drops the trial", () => {
  for (const html of [
    renderToStaticMarkup(<CallToAction signedIn={false} />),
    renderToStaticMarkup(<Pricing sales="all" launched />),
  ])
    expect(html).not.toMatch(/7 days|7-day|free trial/i);
});

test("the landing hero CTA is Get convt and does not hedge", () => {
  const html = renderToStaticMarkup(<Hero signedIn={false} />);
  expect(html).toContain('href="/sign-in?redirect=%2Fdownload"');
  expect(renderToStaticMarkup(<Hero signedIn />)).toContain('href="/download"');
  expect(html).not.toContain("os=linux");
  expect(html).not.toContain("Out now for Linux");
  expect(html).not.toContain("in progress");
  expect(html).not.toContain("Download for Linux");
  expect(html).toContain("Get convt</a>");
});

test("the closing Get convt CTA uses the same /download entry", () => {
  const html = renderToStaticMarkup(<CallToAction signedIn={false} />);
  expect(html).toContain('href="/sign-in?redirect=%2Fdownload"');
  expect(renderToStaticMarkup(<CallToAction signedIn />)).toContain('href="/download"');
  expect(html).not.toContain("os=linux");
  expect(html).not.toContain("Download for Linux");
  expect(html).toContain("Get convt</a>");
});
