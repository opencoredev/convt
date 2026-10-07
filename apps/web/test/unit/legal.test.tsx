import { expect, test } from "bun:test";
import { renderToStaticMarkup } from "react-dom/server";

import { Route as Privacy } from "../../src/routes/_site/privacy";
import { Route as Terms } from "../../src/routes/_site/terms";

const render = (route: typeof Terms | typeof Privacy) => {
  const Page = route.options.component;
  if (!Page) throw new Error("legal route has no component");
  return renderToStaticMarkup(<Page />);
};

for (const [name, route] of [
  ["terms", Terms],
  ["privacy", Privacy],
] as const) {
  test(`/${name} shows no draft notice or bracket placeholder`, () => {
    const html = render(route);
    expect(html).not.toContain("Draft");
    expect(html).not.toContain("<mark");
    expect(html).not.toMatch(/\[[A-Z][^\]]*\]/);
    expect(html).toContain("Effective ");
  });
}

test("the terms state the refund policy, termination, governing law and contact", () => {
  const html = render(Terms);
  for (const id of ["payment", "warranty", "termination", "changes", "law", "contact"]) {
    expect(html).toContain(`id="${id}"`);
  }
  expect(html).toContain("merchant of record");
  expect(html).toContain("These terms are governed by");
});
