import { expect, test } from "bun:test";
import { renderToStaticMarkup } from "react-dom/server";

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
  const cloud = renderToStaticMarkup(<Pricing sales="all" launched />);
  expect(cloud).toContain('href="/checkout/pro?interval=month"');
  expect(cloud).not.toContain("API is not on sale yet");
});
