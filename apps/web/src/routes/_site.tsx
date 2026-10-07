import { Outlet, createFileRoute } from "@tanstack/react-router";

import { themeScript, useSystemTheme } from "#/components/app/theme";
import { SitePage } from "#/components/site/layout";
import { accountFromSession } from "#/lib/account";
import { getSession } from "#/server/session";

// Pathless layout for the public pages besides the landing page: download, formats,
// changelog, API docs, contact and the legal pages. They follow the OS theme like the
// account pages, so this layout runs the same no-flash theme script. The session is
// read on the server so the header shows the signed-in account on the first paint.
export const Route = createFileRoute("/_site")({
  head: () => ({ scripts: [{ children: themeScript }] }),
  loader: async () => {
    const session = await getSession();
    return { account: session ? accountFromSession(session) : null };
  },
  component: SiteLayout,
});

function SiteLayout() {
  useSystemTheme();
  const { account } = Route.useLoaderData();
  return (
    <SitePage account={account}>
      <Outlet />
    </SitePage>
  );
}
