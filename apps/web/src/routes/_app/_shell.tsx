import { Outlet, createFileRoute, redirect } from "@tanstack/react-router";

import { AppShell } from "#/components/app/shell";
import { accountFromSession } from "#/lib/account";
import { getSession } from "#/server/session";

// Pathless layout with the account header and tabs, shared by /dashboard/* and
// /account. Signed-out visitors go to sign-in and come back; an account whose
// email convt has not verified goes to the email check first. Every server
// function behind these pages checks the session again.
export const Route = createFileRoute("/_app/_shell")({
  beforeLoad: async ({ location }) => {
    const session = await getSession();
    if (!session) throw redirect({ to: "/sign-in", search: { redirect: location.href } });
    if (!session.user.emailVerified) throw redirect({ to: "/sign-in/verify-email" });
    return { session };
  },
  loader: ({ context }) => accountFromSession(context.session),
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
