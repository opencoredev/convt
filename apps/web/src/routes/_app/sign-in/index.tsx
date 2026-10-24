import { createFileRoute, redirect } from "@tanstack/react-router";

import { AuthScreen } from "#/components/app/auth-screen";
import { SignInPanel } from "#/components/app/sign-in-panel";
import { safeRedirect } from "#/lib/safe-redirect";
import { authSearch, siteOrigin, type AuthSearch } from "#/lib/sign-in";
import { getPublicConfig } from "#/server/public-config";
import { getSession } from "#/server/session";

export const Route = createFileRoute("/_app/sign-in/")({
  validateSearch: (search: Record<string, unknown>): AuthSearch & { error?: string } => ({
    ...authSearch(search),
    ...(typeof search.error === "string" ? { error: search.error } : {}),
  }),
  beforeLoad: async ({ search }) => {
    const session = await getSession();
    if (session && !session.user.emailVerified) throw redirect({ to: "/sign-in/verify-email" });
    if (session) throw redirect({ href: safeRedirect(search.redirect, siteOrigin()) });
  },
  loader: () => getPublicConfig(),
  head: () => ({ meta: [{ title: "Sign in · convt" }] }),
  component: SignInPage,
});

function SignInPage() {
  const { email, redirect: redirectTo, error } = Route.useSearch();
  const available = Route.useLoaderData().providers;
  return (
    <AuthScreen>
      <SignInPanel
        available={available}
        redirect={redirectTo}
        initialEmail={email}
        initialError={error}
      />
    </AuthScreen>
  );
}
