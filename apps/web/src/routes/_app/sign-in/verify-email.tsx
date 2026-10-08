import { createFileRoute, redirect } from "@tanstack/react-router";
import { useId, useState } from "react";

import { AuthScreen, AuthTitle, QuietButton, Rise, darkPill } from "#/components/app/auth-screen";
import { CodeInput } from "#/components/app/code-input";
import { FormError, FormStatus } from "#/components/app/form-error";
import { sendVerificationCode, signOut, verifyEmail } from "#/lib/auth-client";
import { magicLinkMinutes, signInCodeLength } from "#/lib/config";
import { safeRedirect } from "#/lib/safe-redirect";
import { authSearch, siteOrigin } from "#/lib/sign-in";
import { getSession } from "#/server/session";

// After a GitHub sign-up, or a Google one for an address Google does not own,
// convt has no proof the address is the user's. The dashboard, purchases and
// account data stay closed until a code sent to it comes back.
export const Route = createFileRoute("/_app/sign-in/verify-email")({
  // `redirect` (or `next`): where to go once the address is confirmed, such as /download.
  validateSearch: (search: Record<string, unknown>): { redirect?: string } => {
    const { redirect: to } = authSearch(search);
    return to ? { redirect: to } : {};
  },
  beforeLoad: async () => {
    const session = await getSession();
    if (!session) throw redirect({ to: "/sign-in" });
    if (session.user.emailVerified) throw redirect({ to: "/dashboard" });
    return { email: session.user.email };
  },
  loader: ({ context }) => ({ email: context.email }),
  head: () => ({ meta: [{ title: "Confirm your email · convt" }] }),
  component: VerifyEmailPage,
});

function VerifyEmailPage() {
  const { email } = Route.useLoaderData();
  const { redirect: redirectTo } = Route.useSearch();
  const codeLabelId = useId();
  const [sent, setSent] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [status, setStatus] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [attempt, setAttempt] = useState(0);

  async function send() {
    setBusy(true);
    setError(null);
    const result = await sendVerificationCode(email);
    setBusy(false);
    if (!result.ok) return setError(result.message);
    if (sent) setStatus("We sent a new code. The old one no longer works.");
    setSent(true);
  }

  return (
    <AuthScreen>
      <Rise index={1}>
        <AuthTitle
          sub={
            sent
              ? `We sent a code to ${email}. It works once and expires in ${magicLinkMinutes} minutes.`
              : `Your account uses ${email}. Confirm it with a code before you see licenses and receipts.`
          }
        >
          Confirm your email
        </AuthTitle>
      </Rise>
      <Rise index={2} className="mt-9 flex flex-col items-center gap-3">
        {sent ? (
          <>
            <p id={codeLabelId} className="text-[13px]/4 font-medium text-ink-2">
              Enter the code from the email
            </p>
            <CodeInput
              key={attempt}
              length={signInCodeLength}
              labelId={codeLabelId}
              onComplete={async (code) => {
                setError(null);
                setStatus(null);
                const result = await verifyEmail(email, code);
                if (result.ok)
                  return window.location.assign(safeRedirect(redirectTo, siteOrigin()));
                setError(result.message);
                setAttempt((n) => n + 1);
              }}
            />
          </>
        ) : (
          <button type="button" disabled={busy} className={darkPill} onClick={send}>
            Email me a code
          </button>
        )}
        <div className="text-center">
          <FormError>{error}</FormError>
          <FormStatus>{status}</FormStatus>
        </div>
      </Rise>
      <Rise index={3} className="mt-4 flex flex-wrap justify-center gap-x-5 gap-y-2">
        {sent ? <QuietButton onClick={send}>Resend code</QuietButton> : null}
        <QuietButton
          onClick={async () => {
            await signOut();
            window.location.assign("/sign-in");
          }}
        >
          Use a different account
        </QuietButton>
      </Rise>
    </AuthScreen>
  );
}
