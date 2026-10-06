import { Outlet, createFileRoute } from "@tanstack/react-router";

import { themeScript, useSystemTheme } from "#/components/app/theme";
import { SitePage } from "#/components/site/layout";

// Pathless layout for the public pages besides the landing page: download, formats,
// changelog, API docs, contact and the legal pages. They follow the OS theme like the
// account pages, so this layout runs the same no-flash theme script.
export const Route = createFileRoute("/_site")({
  head: () => ({ scripts: [{ children: themeScript }] }),
  component: SiteLayout,
});

function SiteLayout() {
  useSystemTheme();
  return (
    <SitePage>
      <Outlet />
    </SitePage>
  );
}
