import { Link, createFileRoute, useNavigate } from "@tanstack/react-router";
import { useEffect, useRef, useState } from "react";

import { AuthLayout } from "#/components/app/auth-layout";
import { FormError } from "#/components/app/form-error";
import { PrimaryButton, cx, focusRing } from "#/components/app/ui";
import { signInWithCode } from "#/lib/auth-client";

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
    <AuthLayout>
      <div className="flex flex-col gap-2">
        <h1 className="text-[28px]/[34px] font-semibold tracking-[-0.025em]">Sign in to convt</h1>
        <p className="text-sm/[22px] break-words text-ink-2">
          {pending === undefined
            ? "Reading your sign-in link."
            : pending
              ? `Continue as ${pending.email}.`
              : "This sign-in link is incomplete. Open it again from the email, or enter the code on the sign-in page."}
        </p>
      </div>
      {pending ? (
        <div className="flex flex-col gap-3">
          <PrimaryButton
            disabled={busy}
            className="h-10 text-sm/4.5"
            onClick={async () => {
              setBusy(true);
              setError(null);
              const result = await signInWithCode(pending.email, pending.code);
              if (result.ok) return window.location.assign("/dashboard");
              setBusy(false);
              setError(result.message);
            }}
          >
            Continue
          </PrimaryButton>
          <FormError>{error}</FormError>
        </div>
      ) : null}
      <Link
        to="/sign-in"
        search={pending?.email ? { email: pending.email } : {}}
        className={cx("self-start rounded-sm text-[13px]/4 text-ink-2 hover:text-ink", focusRing)}
      >
        Back to sign in
      </Link>
    </AuthLayout>
  );
}
