import { expect, mock, test } from "bun:test";
import { renderToStaticMarkup } from "react-dom/server";

mock.module("@tanstack/react-router", () => ({
  Link: ({
    to,
    children,
    className,
  }: {
    to: string;
    children: React.ReactNode;
    className?: string;
  }) => (
    <a href={to} className={className}>
      {children}
    </a>
  ),
  useRouterState: ({ select }: { select: (state: { location: { pathname: string } }) => string }) =>
    select({ location: { pathname: "/dashboard/cloud" } }),
}));

import { AppShell } from "../../src/components/app/shell";
import type { Account } from "../../src/lib/types";

const leo: Account = {
  name: "Leo",
  email: "leo@convt.test",
  emailVerified: true,
  avatarUrl: null,
};

test("dashboard nav lists Cloud next to Billing and API", () => {
  const html = renderToStaticMarkup(
    <AppShell account={leo}>
      <p>body</p>
    </AppShell>,
  );
  const nav = html.match(/aria-label="Account"[\s\S]*?<\/nav>/)?.[0] ?? "";
  const labels = [...nav.matchAll(/href="([^"]+)"[^>]*>([^<]+)</g)].map(
    (m) => [m[1], m[2]] as const,
  );
  expect(labels).toEqual([
    ["/dashboard", "Overview"],
    ["/dashboard/licenses", "Licenses"],
    ["/dashboard/billing", "Billing"],
    ["/dashboard/cloud", "Cloud"],
    ["/dashboard/api", "API"],
    ["/account", "Settings"],
  ]);
  expect(html).not.toContain("/dashboard/api/convert");
});
