import { expect, test } from "bun:test";
import { renderToStaticMarkup } from "react-dom/server";

import { CallToAction } from "../../src/components/landing/closing";
import { Hero } from "../../src/components/landing/hero";
import { Pricing } from "../../src/components/landing/pricing";

test("flipping launch enables Desktop and Pro without enabling API sales", () => {
  const before = renderToStaticMarkup(<Pricing sales="desktop" launched={false} />);
  expect(before).not.toContain('href="/checkout/desktop"');
  expect(before).not.toContain('href="/checkout/pro');
  const after = renderToStaticMarkup(<Pricing sales="desktop" launched />);
  expect(after).toContain('href="/checkout/desktop"');
  expect(after).toContain('href="/checkout/pro?interval=month"');
  expect(after).not.toContain("Pro is not on sale yet");
  expect(after).toContain("API is not on sale yet");
  expect(after).toContain("macOS, Windows and Linux");
  expect(after).not.toContain("Linux now");
  const cloud = renderToStaticMarkup(<Pricing sales="all" launched />);
  expect(cloud).toContain('href="/checkout/pro?interval=month"');
  expect(cloud).not.toContain("API is not on sale yet");
});

test("the landing hero Download CTA is OS-aware and does not hedge", () => {
  const html = renderToStaticMarkup(<Hero />);
  expect(html).toContain('href="/download"');
  expect(html).not.toContain("os=linux");
  expect(html).not.toContain("Out now for Linux");
  expect(html).not.toContain("in progress");
  expect(html).not.toContain("Download for Linux");
  expect(html).toContain("Download</a>");
});

test("the closing Download CTA uses the same /download entry", () => {
  const html = renderToStaticMarkup(<CallToAction />);
  expect(html).toContain('href="/download"');
  expect(html).not.toContain("os=linux");
  expect(html).not.toContain("Download for Linux");
  expect(html).toContain("Download</a>");
});
