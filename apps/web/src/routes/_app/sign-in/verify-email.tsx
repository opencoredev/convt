import { createFileRoute, redirect } from "@tanstack/react-router";
import { useId, useState } from "react";

import { AuthLayout } from "#/components/app/auth-layout";
import { CodeInput } from "#/components/app/code-input";
import { FormError, FormStatus } from "#/components/app/form-error";
import { PrimaryButton, TextButton } from "#/components/app/ui";
import { sendVerificationCode, signOut, verifyEmail } from "#/lib/auth-client";
import { magicLinkMinutes, signInCodeLength } from "#/lib/config";
import { getSession } from "#/server/session";

// After a GitHub sign-up, or a Google one for an address Google does not own,
// convt has no proof the address is the user's. The dashboard, purchases and
// account data stay closed until a code sent to it comes back.
export const Route = createFileRoute("/_app/sign-in/verify-email")({
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
    <AuthLayout>
      <div className="flex flex-col gap-2">
        <h1 className="text-[28px]/[34px] font-semibold tracking-[-0.025em]">Confirm your email</h1>
        <p className="text-sm/[22px] break-words text-ink-2">
          {sent
            ? `We sent a code to ${email}. It works once and expires in ${magicLinkMinutes} minutes.`
            : `Your account uses ${email}. Confirm it with a code before you see licenses and receipts.`}
        </p>
      </div>
      {sent ? (
        <div className="flex flex-col gap-2">
          <p id={codeLabelId} className="text-[13px]/4 font-medium">
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
              if (result.ok) return window.location.assign("/dashboard");
              setError(result.message);
              setAttempt((n) => n + 1);
            }}
          />
        </div>
      ) : (
        <PrimaryButton disabled={busy} className="h-10 text-sm/4.5" onClick={send}>
          Email me a code
        </PrimaryButton>
      )}
      <FormError>{error}</FormError>
      <FormStatus>{status}</FormStatus>
      <div className="flex flex-wrap gap-4">
        {sent ? <TextButton onClick={send}>Resend code</TextButton> : null}
        <TextButton
          tone="muted"
          onClick={async () => {
            await signOut();
            window.location.assign("/sign-in");
          }}
        >
          Use a different account
        </TextButton>
      </div>
    </AuthLayout>
  );
}
