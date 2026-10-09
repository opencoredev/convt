import { useNavigate } from "@tanstack/react-router";
import { useId, useState } from "react";

import { FormError } from "#/components/app/form-error";
import { usePlaceholderAction } from "#/components/app/notice";
import { authErrorMessage, sendSignInCode, startSocialSignIn } from "#/lib/auth-client";
import { links } from "#/lib/config";
import { safeRedirect } from "#/lib/safe-redirect";
import { siteOrigin } from "#/lib/sign-in";
import { GITHUB_URL, routes } from "#/lib/site";

import { AuthTitle, Rise, authInput, lightPill } from "./auth-screen";
import { GoogleSignIn, OtherSignIn, type SocialProviderId } from "./social-sign-in";
import { CommandBlock, homebrewCommands } from "./command-block";
import { cx, focusRing } from "./ui";

/**
 * The sign-in column under the logo: Google, "or", the email step, the other providers
 * and the terms line. /sign-in and the signed-out /device page both render it.
 * `emailOnly` is /device?provider=email: the email step alone, focused.
 */
export function SignInPanel({
  available,
  redirect,
  initialEmail,
  initialError,
  emailOnly = false,
  onShowAll,
}: {
  available: Record<SocialProviderId, boolean>;
  redirect: string | undefined;
  initialEmail?: string;
  initialError?: string;
  emailOnly?: boolean;
  onShowAll?: () => void;
}) {
  const navigate = useNavigate();
  const placeholder = usePlaceholderAction();
  const emailId = useId();
  const hintId = useId();
  const errorId = useId();
  const [email, setEmail] = useState(initialEmail ?? "");
  const [error, setError] = useState<string | null>(
    initialError ? authErrorMessage(initialError) : null,
  );
  const [busy, setBusy] = useState(false);

  async function social(provider: SocialProviderId) {
    // Apple only renders once configured (CNV-20), which replaces this placeholder.
    if (provider === "apple") return placeholder("Signing in with Apple");
    setError(null);
    setBusy(true);
    const result = await startSocialSignIn({
      provider,
      callbackURL: safeRedirect(redirect, siteOrigin()),
      // A brand-new account goes on to install the app, unless a page asked for it back.
      ...(redirect ? {} : { newUserCallbackURL: routes.download }),
    });
    if (!result.ok) {
      setBusy(false);
      setError(result.message);
    }
  }

  return (
    <>
      <Rise index={1}>
        <AuthTitle>
          {emailOnly ? "Continue with your email" : "Sign in or create your account"}
        </AuthTitle>
      </Rise>

      <div className="mt-10 flex w-full flex-col gap-5">
        {emailOnly ? null : (
          <Rise index={2}>
            <GoogleSignIn available={available} onSelect={social} busy={busy} />
          </Rise>
        )}

        <Rise index={3}>
          <form
            className="flex flex-col gap-3"
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
                search: { email: email.trim(), ...(redirect ? { redirect } : {}) },
              });
            }}
          >
            <label htmlFor={emailId} className="sr-only">
              Email
            </label>
            <input
              id={emailId}
              type="email"
              name="email"
              required
              autoComplete="email"
              inputMode="email"
              // The desktop app sent the visitor here to type an address.
              autoFocus={emailOnly}
              placeholder="Email address"
              value={email}
              onChange={(event) => setEmail(event.target.value)}
              aria-describedby={error ? `${errorId} ${hintId}` : hintId}
              aria-invalid={error ? true : undefined}
              className={authInput}
            />
            <div id={errorId} className="empty:hidden">
              <FormError className="text-center">{error}</FormError>
            </div>
            <button type="submit" disabled={busy} className={lightPill}>
              Continue with Email
            </button>
          </form>
        </Rise>

        <Rise index={4} className="flex flex-col items-center gap-4 pt-1">
          {emailOnly ? (
            onShowAll ? (
              <button
                type="button"
                onClick={onShowAll}
                className={cx(
                  "cursor-pointer rounded-sm text-[13px]/5 text-ink-2 underline-offset-[3px] hover:text-ink hover:underline",
                  focusRing,
                )}
              >
                Other ways to sign in
              </button>
            ) : null
          ) : (
            <OtherSignIn available={available} onSelect={social} />
          )}
          <p id={hintId} className="text-center text-xs/[18px] text-pretty text-ink-2">
            By continuing, you agree to our{" "}
            <a
              href={links.terms}
              className={cx("rounded-sm text-ink underline underline-offset-2", focusRing)}
            >
              Terms
            </a>{" "}
            and{" "}
            <a
              href={links.privacy}
              className={cx("rounded-sm text-ink underline underline-offset-2", focusRing)}
            >
              Privacy Policy
            </a>
            . Bought a license already? Use the email on the receipt.
          </p>
        </Rise>

        {redirect === routes.download ? (
          <Rise index={5}>
            <WithoutAccount />
          </Rise>
        ) : null}
      </div>
    </>
  );
}

/**
 * /download asks for an account, but Homebrew and the GitHub releases cannot be gated,
 * so say so instead of pretending.
 */
function WithoutAccount() {
  return (
    <div className="mt-3 flex flex-col items-center gap-2 border-t border-line pt-5 text-center">
      <p className="text-xs/[18px] text-ink-2">
        Or install without an account from{" "}
        <a
          href={`${GITHUB_URL}/releases/latest`}
          className={cx("rounded-sm text-ink underline underline-offset-2", focusRing)}
        >
          GitHub releases
        </a>{" "}
        or with Homebrew on a Mac:
      </p>
      <CommandBlock label="Homebrew" commands={homebrewCommands} quiet className="mt-1 w-full" />
    </div>
  );
}
