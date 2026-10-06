import { expect, test } from "bun:test";
import { renderToStaticMarkup } from "react-dom/server";

import { Pricing } from "../../src/components/landing/pricing";

test("flipping launch enables Desktop without enabling cloud sales", () => {
  const before = renderToStaticMarkup(<Pricing sales="desktop" launched={false} />);
  expect(before).not.toContain('href="/checkout/desktop"');
  const after = renderToStaticMarkup(<Pricing sales="desktop" launched />);
  expect(after).toContain('href="/checkout/desktop"');
  expect(after).not.toContain('href="/checkout/pro');
  expect(after).toContain("Coming soon");
  const cloud = renderToStaticMarkup(<Pricing sales="all" launched />);
  expect(cloud).toContain('href="/checkout/pro?interval=month"');
});
