import { expect, test } from "bun:test";
import { renderToStaticMarkup } from "react-dom/server";

import { Pricing } from "../../src/components/landing/pricing";
import { LAUNCHED } from "../../src/lib/site";

test("launch stays off; flipping it enables Desktop without enabling cloud sales", () => {
  expect(LAUNCHED).toBe(false);
  const before = renderToStaticMarkup(<Pricing sales="desktop" launched={false} />);
  expect(before).not.toContain('href="/checkout/desktop"');
  const after = renderToStaticMarkup(<Pricing sales="desktop" launched />);
  expect(after).toContain('href="/checkout/desktop"');
  expect(after).not.toContain('href="/checkout/pro');
  expect(after).toContain("Coming soon");
  const cloud = renderToStaticMarkup(<Pricing sales="all" launched />);
  expect(cloud).toContain('href="/checkout/pro?interval=month"');
});
