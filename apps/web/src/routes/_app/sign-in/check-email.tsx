import { Link, createFileRoute } from "@tanstack/react-router";
import { useId, useState } from "react";

import { AuthLayout } from "#/components/app/auth-layout";
import { CodeInput } from "#/components/app/code-input";
import { FormError, FormStatus } from "#/components/app/form-error";
import { TextButton, cx, focusRing } from "#/components/app/ui";
import { sendSignInCode, signInWithCode } from "#/lib/auth-client";
import { magicLinkMinutes, signInCodeLength } from "#/lib/config";
import { safeRedirect } from "#/lib/safe-redirect";
import { authSearch, siteOrigin } from "#/lib/sign-in";

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
      window.location.assign(safeRedirect(redirectTo, siteOrigin()));
      return;
    }
    setBusy(false);
    setError(result.message);
    setAttempt((n) => n + 1);
  }

  return (
    <AuthLayout>
      <span
        aria-hidden="true"
        className="flex size-11 shrink-0 items-center justify-center rounded-[10px] bg-green-tint shadow-[inset_0_0_0_1px_var(--green-line)]"
      >
        <svg width="22" height="22" viewBox="0 0 24 24" className="text-green">
          <rect
            x="3"
            y="5"
            width="18"
            height="14"
            rx="2"
            fill="none"
            stroke="currentColor"
            strokeWidth="1.8"
          />
          <path
            d="M 3.5 6.5 L 12 13 L 20.5 6.5"
            fill="none"
            stroke="currentColor"
            strokeWidth="1.8"
            strokeLinecap="round"
            strokeLinejoin="round"
          />
        </svg>
      </span>

      <div className="flex flex-col gap-2">
        <h1 className="text-[28px]/[34px] font-semibold tracking-[-0.025em]">Check your email</h1>
        <p className="text-sm/[22px] break-words text-ink-2">
          We sent a sign-in link to {email ?? "your email"}. It works once and expires in{" "}
          {magicLinkMinutes} minutes.
        </p>
      </div>

      <div className="flex flex-col gap-2">
        <p id={codeLabelId} className="text-[13px]/4 font-medium">
          Or enter the code from the email
        </p>
        <CodeInput
          key={attempt}
          length={signInCodeLength}
          labelId={codeLabelId}
          onComplete={submit}
        />
        <FormError>{error}</FormError>
        <FormStatus>{status}</FormStatus>
      </div>

      <div className="flex flex-wrap gap-4">
        <TextButton
          disabled={!email}
          onClick={async () => {
            if (!email) return;
            setError(null);
            setStatus(null);
            const result = await sendSignInCode(email);
            if (result.ok) setStatus("We sent a new link. The old one no longer works.");
            else setError(result.message);
          }}
        >
          Resend link
        </TextButton>
        <Link
          to="/sign-in"
          search={{ ...(email ? { email } : {}), ...(redirectTo ? { redirect: redirectTo } : {}) }}
          className={cx("rounded-sm text-[13px]/4 text-ink-2 hover:text-ink", focusRing)}
        >
          Use a different email
        </Link>
      </div>
    </AuthLayout>
  );
}
