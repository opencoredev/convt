import { Link, createFileRoute, useNavigate } from "@tanstack/react-router";
import { useEffect, useRef, useState } from "react";

import { AuthLayout } from "#/components/app/auth-layout";
import { FormError } from "#/components/app/form-error";
import { PrimaryButton, cx, focusRing } from "#/components/app/ui";
import { PRIVACY_EMAIL } from "#/lib/site";
import { unsubscribeLaunchList } from "#/server/mobile-link-fns";

// The unsubscribe link in launch-list email: /unsubscribe#t=<token>. Like the
// sign-in link, the fragment never reaches the server, logs or referrers; the page
// reads it, clears it from the address bar, and deletes the address only when the
// button is pressed, so a mail scanner that opens the link changes nothing.
export const Route = createFileRoute("/_app/unsubscribe")({
  head: () => ({
    meta: [{ title: "Unsubscribe · convt" }, { name: "referrer", content: "no-referrer" }],
  }),
  headers: () => ({ "Cache-Control": "no-store", "Referrer-Policy": "no-referrer" }),
  component: UnsubscribePage,
});

const tokenShape = /^[A-Za-z0-9_-]{43}$/;

type State =
  | { step: "reading" }
  | { step: "invalid" }
  | { step: "ready"; token: string }
  | { step: "done"; removed: boolean };

function UnsubscribePage() {
  const [state, setState] = useState<State>({ step: "reading" });
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const navigate = useNavigate();
  const read = useRef(false);

  useEffect(() => {
    // Once: development mode runs effects twice, and the hash is gone after the first.
    if (read.current) return;
    read.current = true;
    const token = new URLSearchParams(window.location.hash.slice(1)).get("t");
    setState(token && tokenShape.test(token) ? { step: "ready", token } : { step: "invalid" });
    void navigate({ to: "/unsubscribe", hash: "", replace: true });
  }, [navigate]);

  const message =
    state.step === "reading"
      ? "Reading your unsubscribe link."
      : state.step === "invalid"
        ? `This unsubscribe link is incomplete. Open it again from the email, or write to ${PRIVACY_EMAIL} and we'll remove your address.`
        : state.step === "ready"
          ? "Leave the convt launch list. We delete your email address and send you nothing more."
          : state.removed
            ? "You're unsubscribed. We deleted your email address from the launch list."
            : `This link doesn't match an address on the list, so it's already been removed. If you still get launch email, write to ${PRIVACY_EMAIL}.`;

  return (
    <AuthLayout>
      <div className="flex flex-col gap-2">
        <h1 className="text-[28px]/[34px] font-semibold tracking-[-0.025em]">
          {state.step === "done" && state.removed ? "Unsubscribed" : "Unsubscribe"}
        </h1>
        <p aria-live="polite" className="text-sm/[22px] break-words text-ink-2">
          {message}
        </p>
      </div>
      {state.step === "ready" ? (
        <div className="flex flex-col gap-3">
          <PrimaryButton
            disabled={busy}
            className="h-10 text-sm/4.5"
            onClick={async () => {
              setBusy(true);
              setError(null);
              try {
                const result = await unsubscribeLaunchList({ data: { token: state.token } });
                setState({ step: "done", removed: result.removed });
              } catch {
                setError("That didn't go through. Try again in a minute.");
              } finally {
                setBusy(false);
              }
            }}
          >
            Unsubscribe and delete my address
          </PrimaryButton>
          <FormError>{error}</FormError>
        </div>
      ) : null}
      <Link
        to="/"
        className={cx("self-start rounded-sm text-[13px]/4 text-ink-2 hover:text-ink", focusRing)}
      >
        convt.app
      </Link>
    </AuthLayout>
  );
}
