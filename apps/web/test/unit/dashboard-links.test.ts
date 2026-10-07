import { expect, test } from "bun:test";

import { links } from "../../src/lib/config";

test("account pages download without forcing macOS and link straight to checkout", () => {
  expect(links.download).toBe("/download");
  expect(links.download).not.toContain("os=");
  expect(links.buyDesktop).toBe("/checkout/desktop");
  expect(links.buyPro).toBe("/checkout/pro?interval=month");
});
