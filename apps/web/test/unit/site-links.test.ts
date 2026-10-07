import { expect, test } from "bun:test";

import { footerColumns, headerLinks } from "../../src/components/site/links";

test("the public footer has no status page until one exists", () => {
  const labels = footerColumns.flatMap((column) => column.links.map((link) => link.label));
  expect(labels).not.toContain("Status");
  expect(labels).toContain("GitHub");
  expect(labels).toContain("Contact");
});

test("marketing header links stay Download, Formats, Pricing, API docs", () => {
  expect(headerLinks.map((link) => link.label)).toEqual([
    "Download",
    "Formats",
    "Pricing",
    "API docs",
  ]);
});
