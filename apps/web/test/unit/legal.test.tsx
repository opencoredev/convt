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
    const text = html.replace(/<[^>]*>/g, "");
    expect(text).not.toMatch(/draft|placeholder|todo/i);
    expect(html).not.toContain("<mark");
    expect(text).not.toMatch(/\[[^\]]*\]/);
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

test("the terms name convt as seller, Polar as merchant of record and Florida law", () => {
  const text = render(Terms).replace(/<[^>]*>/g, "");
  expect(text).toContain("you and convt (");
  expect(text).toContain("Polar (polar.sh) as merchant of record");
  expect(text).toContain("within 14 days");
  expect(text).toContain("governed by the laws of the State of Florida, USA");
  expect(text).toContain("courts located in Florida");
  expect(text).toContain("mandatory law of the country where you live");
  expect(text).toContain("Effective October 7, 2026");
});

test("the privacy policy does not claim the app has no analytics or crash reporting", () => {
  const text = render(Privacy).replace(/<[^>]*>/g, "");
  expect(text).not.toMatch(/no analytics|crash reporting|never in the app/i);
});
