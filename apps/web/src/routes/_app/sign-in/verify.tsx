import { Link, createFileRoute, useNavigate } from "@tanstack/react-router";
import { useEffect, useRef, useState } from "react";

import { AuthScreen, AuthTitle, Rise, darkPill } from "#/components/app/auth-screen";
import { FormError } from "#/components/app/form-error";
import { cx, focusRing } from "#/components/app/ui";
import { signInWithCode } from "#/lib/auth-client";
import { isNewAccount, landingAfterSignIn, siteOrigin } from "#/lib/sign-in";

// The link in the sign-in email: /sign-in/verify#email=...&code=... The fragment
// never reaches the server, logs or referrers. The page reads it, removes it from
// the address bar at once, and signs in only when the button is pressed, so a
// mail scanner that opens the link cannot use the code.
export const Route = createFileRoute("/_app/sign-in/verify")({
  head: () => ({
    meta: [{ title: "Sign in · convt" }, { name: "referrer", content: "no-referrer" }],
  }),
  headers: () => ({ "Cache-Control": "no-store", "Referrer-Policy": "no-referrer" }),
  component: VerifyPage,
});

type Pending = { email: string; code: string } | null;

function VerifyPage() {
  // undefined: not read yet (server render); null: no usable link.
  const [pending, setPending] = useState<Pending | undefined>(undefined);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const navigate = useNavigate();
  const read = useRef(false);

  useEffect(() => {
    // Once: development mode runs effects twice, and the hash is gone after the first.
    if (read.current) return;
    read.current = true;
    const params = new URLSearchParams(window.location.hash.slice(1));
    const email = params.get("email");
    const code = params.get("code");
    setPending(email && code && /^\d{6}$/.test(code) ? { email, code } : null);
    // Through the router, which would otherwise put its own copy of the hash back.
    void navigate({ to: "/sign-in/verify", hash: "", replace: true });
  }, [navigate]);

  return (
    <AuthScreen>
      <Rise index={1}>
        <AuthTitle
          sub={
            pending === undefined
              ? "Reading your sign-in link."
              : pending
                ? `Continue as ${pending.email}.`
                : "This sign-in link is incomplete. Open it again from the email, or enter the code on the sign-in page."
          }
        >
          Sign in to convt
        </AuthTitle>
      </Rise>
      {pending ? (
        <Rise index={2} className="mt-9 flex flex-col gap-3">
          <button
            type="button"
            disabled={busy}
            className={darkPill}
            onClick={async () => {
              setBusy(true);
              setError(null);
              const result = await signInWithCode(pending.email, pending.code);
              if (result.ok)
                return window.location.assign(
                  landingAfterSignIn({
                    redirect: undefined,
                    newAccount: isNewAccount(result.data?.user?.createdAt),
                    origin: siteOrigin(),
                  }),
                );
              setBusy(false);
              setError(result.message);
            }}
          >
            Continue
          </button>
          <FormError className="text-center">{error}</FormError>
        </Rise>
      ) : null}
      <Rise index={3} className="mt-6 flex justify-center">
        <Link
          to="/sign-in"
          search={pending?.email ? { email: pending.email } : {}}
          className={cx(
            "rounded-sm text-[13px]/4 text-ink-2 underline-offset-[3px] hover:text-ink hover:underline",
            focusRing,
          )}
        >
          Back to sign in
        </Link>
      </Rise>
    </AuthScreen>
  );
}
