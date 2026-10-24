import { Outlet, createFileRoute, redirect, type ParsedLocation } from "@tanstack/react-router";

import { AppShell } from "#/components/app/shell";
import { accountFromSession } from "#/lib/account";
import { getSession, type SessionInfo } from "#/server/session";

// Pathless layout with the account header and tabs, shared by /dashboard/* and
// /account. Signed-out visitors go to sign-in and come back; an account whose
// email convt has not verified goes to the email check first. Every server
// function behind these pages checks the session again.
export const Route = createFileRoute("/_app/_shell")({
  beforeLoad: async ({ location, cause }) => {
    // A tab switch keeps the shell mounted, and the check it ran on entry still
    // stands. Skipping it saves a round trip before every tab's own data.
    if (cause === "stay") return { session: undefined };
    return { session: requireSession(await getSession(), location) };
  },
  // Runs on entry with the session beforeLoad fetched, and again after
  // `router.invalidate()` (a rename or a new email), when it fetches its own.
  loader: async ({ context, location }) =>
    accountFromSession(context.session ?? requireSession(await getSession(), location)),
  component: ShellLayout,
});

function requireSession(session: SessionInfo | null, location: ParsedLocation): SessionInfo {
  if (!session) throw redirect({ to: "/sign-in", search: { redirect: location.href } });
  if (!session.user.emailVerified) throw redirect({ to: "/sign-in/verify-email" });
  return session;
}

function ShellLayout() {
  const account = Route.useLoaderData();
  return (
    <AppShell account={account}>
      <Outlet />
    </AppShell>
  );
}
