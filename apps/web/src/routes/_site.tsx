import { Outlet, createFileRoute } from "@tanstack/react-router";

import { SitePage } from "#/components/site/layout";
import { accountFromSession } from "#/lib/account";
import { getSession } from "#/server/session";

// Pathless layout for the public pages besides the landing page: download, formats,
// changelog, API docs, contact and the legal pages. The root route sets the theme. The
// session is read on the server so the header shows the signed-in account on the first
// paint.
export const Route = createFileRoute("/_site")({
  loader: async () => {
    const session = await getSession();
    return { account: session ? accountFromSession(session) : null };
  },
  component: SiteLayout,
});

function SiteLayout() {
  const { account } = Route.useLoaderData();
  return (
    <SitePage account={account}>
      <Outlet />
    </SitePage>
  );
}
