import { expect, test } from "bun:test";

import { footerColumns } from "#/components/site/links";
import { guidePath, guides } from "#/lib/guides";
import { routes } from "#/lib/site";

test("the footer links every guide and the guides index", () => {
  const hrefs = footerColumns.flatMap((column) => column.links.map((link) => link.href));
  for (const guide of guides) {
    expect(hrefs).toContain(guidePath(guide.slug));
  }
  expect(hrefs).toContain(routes.guides);
});

test("footer links are unique, since React keys them by label", () => {
  for (const column of footerColumns) {
    const labels = column.links.map((link) => link.label);
    expect(new Set(labels).size).toBe(labels.length);
  }
  const hrefs = footerColumns.flatMap((column) => column.links.map((link) => link.href));
  expect(new Set(hrefs).size).toBe(hrefs.length);
});
