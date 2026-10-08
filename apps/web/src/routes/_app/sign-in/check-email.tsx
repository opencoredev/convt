import { Mail01Icon } from "@hugeicons/core-free-icons";
import { Link, createFileRoute } from "@tanstack/react-router";
import { useId, useState } from "react";

import { AuthScreen, AuthTitle, QuietButton, Rise } from "#/components/app/auth-screen";
import { CodeInput } from "#/components/app/code-input";
import { FormError, FormStatus } from "#/components/app/form-error";
import { cx, focusRing } from "#/components/app/ui";
import { Icon } from "#/components/icon";
import { sendSignInCode, signInWithCode } from "#/lib/auth-client";
import { magicLinkMinutes, signInCodeLength } from "#/lib/config";
import { authSearch, isNewAccount, landingAfterSignIn, siteOrigin } from "#/lib/sign-in";

// Shown after the sign-in form sent a code. The email holds the same code and a
// link to /sign-in/verify; either one signs in, once.
export const Route = createFileRoute("/_app/sign-in/check-email")({
  validateSearch: authSearch,
  head: () => ({ meta: [{ title: "Check your email · convt" }] }),
  component: CheckEmailPage,
});

function CheckEmailPage() {
  const { email, redirect: redirectTo } = Route.useSearch();
  const codeLabelId = useId();
  const [error, setError] = useState<string | null>(null);
  const [status, setStatus] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  // A new key clears the boxes after a wrong code.
  const [attempt, setAttempt] = useState(0);

  async function submit(code: string) {
    if (!email || busy) return;
    setBusy(true);
    setError(null);
    setStatus(null);
    const result = await signInWithCode(email, code);
    if (result.ok) {
      window.location.assign(
        landingAfterSignIn({
          redirect: redirectTo,
          newAccount: isNewAccount(result.data?.user?.createdAt),
          origin: siteOrigin(),
        }),
      );
      return;
    }
    setBusy(false);
    setError(result.message);
    setAttempt((n) => n + 1);
  }

  return (
    <AuthScreen>
      <Rise index={1} className="mt-8 flex justify-center">
        <span
          aria-hidden="true"
          className="flex size-12 items-center justify-center rounded-full bg-green-tint text-green shadow-[inset_0_0_0_1px_var(--green-line)]"
        >
          <Icon icon={Mail01Icon} size={22} strokeWidth={1.7} />
        </span>
      </Rise>
      <Rise index={2}>
        <AuthTitle
          strong
          sub={
            <>
              We sent a code and a sign-in link to{" "}
              <span className="font-medium text-ink">{email ?? "your email"}</span>. Both work once
              and expire in {magicLinkMinutes} minutes.
            </>
          }
        >
          Check your email
        </AuthTitle>
      </Rise>

      <Rise index={3} className="mt-9 flex flex-col items-center gap-3">
        <p id={codeLabelId} className="text-[13px]/4 font-medium text-ink-2">
          Enter the code
        </p>
        <CodeInput
          key={attempt}
          length={signInCodeLength}
          labelId={codeLabelId}
          onComplete={submit}
        />
        <div className="min-h-[18px] text-center">
          <FormError>{error}</FormError>
          <FormStatus>{status}</FormStatus>
        </div>
      </Rise>

      <Rise index={4} className="mt-4 flex flex-wrap items-center justify-center gap-x-5 gap-y-2">
        <QuietButton
          disabled={!email}
          onClick={async () => {
            if (!email) return;
            setError(null);
            setStatus(null);
            const result = await sendSignInCode(email);
            if (result.ok) setStatus("We sent a new code. The old one no longer works.");
            else setError(result.message);
          }}
        >
          Resend code
        </QuietButton>
        <Link
          to="/sign-in"
          search={{ ...(email ? { email } : {}), ...(redirectTo ? { redirect: redirectTo } : {}) }}
          className={cx(
            "rounded-sm text-[13px]/4 text-ink-2 underline-offset-[3px] hover:text-ink hover:underline",
            focusRing,
          )}
        >
          Use a different email
        </Link>
      </Rise>
    </AuthScreen>
  );
}
