import { Outlet, createFileRoute } from "@tanstack/react-router";

import { AppShell } from "#/components/app/shell";
import { getAccount } from "#/lib/account";

// Pathless layout with the account header and tabs, shared by /dashboard/* and /account.
// PLACEHOLDER: there is no session check yet; every visitor sees the placeholder account.
export const Route = createFileRoute("/_app/_shell")({
  loader: () => getAccount(),
  component: ShellLayout,
});

function ShellLayout() {
  const account = Route.useLoaderData();
  return (
    <AppShell account={account}>
      <Outlet />
    </AppShell>
  );
}
