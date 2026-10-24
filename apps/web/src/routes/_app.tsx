import { Outlet, createFileRoute } from "@tanstack/react-router";

import { NoticeProvider } from "#/components/app/notice";

// Pathless layout for the account pages (sign-in, dashboard, settings). The root route
// sets the theme for every page.
export const Route = createFileRoute("/_app")({
  head: () => ({
    meta: [{ name: "robots", content: "noindex" }],
  }),
  component: AppLayout,
});

function AppLayout() {
  return (
    <NoticeProvider>
      <Outlet />
    </NoticeProvider>
  );
}
