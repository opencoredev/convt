import { expect, mock, test } from "bun:test";
import { renderToStaticMarkup } from "react-dom/server";

mock.module("@tanstack/react-router", () => ({
  Link: ({
    to,
    children,
    ...props
  }: {
    to: string;
    children: React.ReactNode;
    onClick?: () => void;
    className?: string;
  }) => (
    <a href={to} {...props}>
      {children}
    </a>
  ),
}));

import { Nav } from "../../src/components/landing/nav";
import { headerLinks } from "../../src/components/site/links";
import { routes } from "../../src/lib/site";
import type { Account } from "../../src/lib/types";

const leo: Account = {
  name: "Leo",
  email: "leo@convt.test",
  emailVerified: true,
  avatarUrl: null,
};

function downloads(html: string) {
  return html.match(/>Download</g)?.length ?? 0;
}

test("header links are the launched set, never Coming soon", () => {
  expect(headerLinks.map((l) => l.label)).toEqual(["Download", "Formats", "Pricing", "API docs"]);
  const html = renderToStaticMarkup(<Nav account={null} path="/" />);
  expect(html).toContain("Formats");
  expect(html).toContain("Pricing");
  expect(html).toContain("API docs");
  expect(html).toContain("Sign in");
  expect(html).not.toContain("Coming soon");
  expect(html).toContain("pointer-events-none absolute inset-0");
});

test("signed-out visitors get one green Download, not a second text link", () => {
  const html = renderToStaticMarkup(<Nav account={null} path="/" />);
  expect(downloads(html)).toBe(1);
  expect(html).toContain(`href="${routes.download}"`);
  expect(html).toContain("Sign in");
});

test("signed-in visitors see the avatar and one Download, never Sign in", () => {
  const html = renderToStaticMarkup(<Nav account={leo} path="/" />);
  expect(html).toContain("Account: Leo");
  expect(html).not.toContain("Sign in");
  expect(downloads(html)).toBe(1);
});

test("/download never shows Download in the header", () => {
  const out = renderToStaticMarkup(<Nav account={null} path="/download" />);
  const inn = renderToStaticMarkup(<Nav account={leo} path="/download" />);
  expect(downloads(out)).toBe(0);
  expect(downloads(inn)).toBe(0);
  expect(out).toContain("Sign in");
  expect(inn).toContain("Account: Leo");
  expect(inn).not.toContain("Sign in");
});

test("omitting account keeps a skeleton instead of flashing Sign in", () => {
  const html = renderToStaticMarkup(<Nav path="/" />);
  expect(html).not.toContain("Sign in");
  expect(html).not.toContain("Account:");
  expect(html).toContain('aria-hidden="true"');
  expect(downloads(html)).toBe(0);
});
