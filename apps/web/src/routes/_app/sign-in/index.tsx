import { createFileRoute, redirect, useNavigate } from "@tanstack/react-router";
import { useId, useState } from "react";

import { AuthLayout } from "#/components/app/auth-layout";
import { FormError } from "#/components/app/form-error";
import { usePlaceholderAction } from "#/components/app/notice";
import { cx, focusRing } from "#/components/app/ui";
import { authErrorMessage, sendSignInCode, startSocialSignIn } from "#/lib/auth-client";
import { safeRedirect } from "#/lib/safe-redirect";
import { authSearch, siteOrigin, type AuthSearch } from "#/lib/sign-in";
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
  head: () => ({ meta: [{ title: "Sign in · convt" }] }),
  component: SignInPage,
});

// Sign-in providers. Buttons are text-only by Leo's decision. Apple waits for the
// Apple Developer account (plan P4).
const providers = [
  { id: "github", label: "GitHub" },
  { id: "google", label: "Google" },
  { id: "apple", label: "Apple" },
] as const;

function SignInPage() {
  const { email: initialEmail, redirect: redirectTo, error: callbackError } = Route.useSearch();
  const navigate = useNavigate();
  const placeholder = usePlaceholderAction();
  const emailId = useId();
  const hintId = useId();
  const errorId = useId();
  const [email, setEmail] = useState(initialEmail ?? "");
  const [error, setError] = useState<string | null>(
    callbackError ? authErrorMessage(callbackError) : null,
  );
  const [busy, setBusy] = useState(false);

  return (
    <AuthLayout>
      <div className="flex flex-col gap-2">
        <h1 className="text-[28px]/[34px] font-semibold tracking-[-0.025em]">Sign in to convt</h1>
        <p className="text-sm/[22px] text-ink-2">
          Use your email or an account you already have. New here? Signing in creates your account.
        </p>
      </div>

      <form
        className="flex flex-col gap-6"
        onSubmit={async (event) => {
          event.preventDefault();
          if (busy) return;
          setBusy(true);
          setError(null);
          const result = await sendSignInCode(email);
          setBusy(false);
          if (!result.ok) return setError(result.message);
          void navigate({
            to: "/sign-in/check-email",
            search: { email: email.trim(), ...(redirectTo ? { redirect: redirectTo } : {}) },
          });
        }}
      >
        <div className="flex flex-col gap-2">
          <label htmlFor={emailId} className="text-[13px]/4 font-medium">
            Email
          </label>
          <input
            id={emailId}
            type="email"
            name="email"
            required
            autoComplete="email"
            inputMode="email"
            placeholder="you@example.com"
            value={email}
            onChange={(event) => setEmail(event.target.value)}
            aria-describedby={error ? `${errorId} ${hintId}` : hintId}
            aria-invalid={error ? true : undefined}
            className="h-10 rounded-lg bg-raised px-3 text-sm/4.5 text-ink shadow-input outline-none placeholder:text-ink-3 focus-visible:ring-2 focus-visible:ring-green"
          />
        </div>
        <div id={errorId} className="-mt-3 empty:hidden">
          <FormError>{error}</FormError>
        </div>
        <button
          type="submit"
          disabled={busy}
          className={cx(
            "btn-primary h-10 cursor-pointer rounded-lg text-sm/4.5 font-medium disabled:cursor-wait disabled:opacity-70",
            focusRing,
          )}
        >
          Email me a link
        </button>
      </form>

      <div className="flex flex-col gap-2.5">
        <div className="flex items-center gap-3">
          <span className="h-px flex-1 bg-line" />
          <span className="text-xs/4 text-ink-3">or continue with</span>
          <span className="h-px flex-1 bg-line" />
        </div>
        <div className="flex gap-2">
          {providers.map((provider) => (
            <button
              key={provider.id}
              type="button"
              onClick={async () => {
                if (provider.id === "apple") return placeholder("Signing in with Apple");
                setError(null);
                const result = await startSocialSignIn(
                  provider.id,
                  safeRedirect(redirectTo, siteOrigin()),
                );
                if (!result.ok) setError(result.message);
              }}
              className={cx(
                "h-10 flex-1 cursor-pointer rounded-lg bg-raised text-[13px]/4 font-medium shadow-button hover:bg-hover dark:bg-sunken",
                focusRing,
              )}
            >
              {provider.label}
            </button>
          ))}
        </div>
      </div>

      <p id={hintId} className="text-xs/[18px] text-ink-3">
        Bought a license without an account? Use the email from your receipt and it will be waiting
        for you.
      </p>
    </AuthLayout>
  );
}
