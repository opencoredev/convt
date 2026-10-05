import { Outlet, createFileRoute } from "@tanstack/react-router";

import { NoticeProvider } from "#/components/app/notice";
import { themeScript, useSystemTheme } from "#/components/app/theme";

// Pathless layout for the account pages (sign-in, dashboard, settings). It owns the
// no-flash theme script because the root route belongs to the landing page.
export const Route = createFileRoute("/_app")({
  head: () => ({
    meta: [{ name: "robots", content: "noindex" }],
    scripts: [{ children: themeScript }],
  }),
  component: AppLayout,
});

function AppLayout() {
  useSystemTheme();
  return (
    <NoticeProvider>
      <Outlet />
    </NoticeProvider>
  );
}
