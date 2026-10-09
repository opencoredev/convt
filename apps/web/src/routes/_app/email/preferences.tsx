import { Link, createFileRoute } from "@tanstack/react-router";
import { useState } from "react";

import { AuthLayout } from "#/components/app/auth-layout";
import { FormError } from "#/components/app/form-error";
import { SecondaryButton, cx, focusRing } from "#/components/app/ui";
import {
  fetchLinkPreference,
  saveLinkPreference,
  type LinkPreference,
} from "#/server/marketing-fns";

// The "Email preferences" link in every campaign email. The token in the link is
// the only credential, so this works signed out. Opening the page changes
// nothing: link scanners fetch URLs from email, so only the button (a POST)
// unsubscribes.
export const Route = createFileRoute("/_app/email/preferences")({
  validateSearch: (search: Record<string, unknown>): { t?: string } =>
    typeof search.t === "string" ? { t: search.t } : {},
  loaderDeps: ({ search }) => ({ t: search.t }),
  loader: ({ deps }) => fetchLinkPreference({ data: { token: deps.t ?? "" } }),
  head: () => ({
    meta: [{ title: "Email preferences · convt" }, { name: "referrer", content: "no-referrer" }],
  }),
  // Keyed by the token, so following another link never shows the previous one's state.
  component: () => <PreferencesPage key={Route.useSearch().t ?? ""} />,
});

const aside = {
  title: "EMAIL FROM CONVT",
  items: [
    "Product news and release notes",
    "Tips for getting more out of convt",
    "License and account email always arrives",
  ],
};

function PreferencesPage() {
  const loaded = Route.useLoaderData();
  const { t } = Route.useSearch();
  const [pref, setPref] = useState<LinkPreference>(loaded);
  const [busy, setBusy] = useState(false);
  const [status, setStatus] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  async function choose(subscribed: boolean) {
    setBusy(true);
    setError(null);
    setStatus(null);
    try {
      const next = await saveLinkPreference({ data: { token: t ?? "", subscribed } });
      setPref(next);
      if (next.state === "ok")
        setStatus(
          subscribed
            ? "You're subscribed again."
            : "You're unsubscribed. It can take a few minutes to reach every list.",
        );
      else if (next.state === "unavailable")
        setError("Unable to save your choice right now. Try again in a minute.");
    } catch {
      setError("Unable to save your choice. Check your connection and try again.");
    } finally {
      setBusy(false);
    }
  }

  return (
    <AuthLayout aside={aside}>
      {pref.state === "ok" ? (
        <>
          <div className="flex flex-col gap-2">
            <h1 className="text-[28px]/[34px] font-semibold tracking-[-0.025em]">
              Email preferences
            </h1>
            <p className="text-sm/[22px] break-words text-ink-2">
              For the convt account <span className="font-medium text-ink">{pref.maskedEmail}</span>
              .
            </p>
          </div>
          <div className="flex flex-col gap-4 rounded-xl bg-raised p-5 shadow-[inset_0_0_0_1px_var(--line)]">
            <div className="flex flex-col gap-1">
              <h2 className="text-[15px]/5 font-semibold">Product news and tips</h2>
              <p className="text-[13px]/[19px] text-ink-2">
                {pref.subscribed
                  ? "You get occasional emails about new features, releases and offers."
                  : "You don't get product news or tips. License and account emails still arrive."}
              </p>
            </div>
            <div className="flex flex-wrap items-center gap-3">
              {/* One button whose label changes, so keyboard focus stays on it. */}
              {/* aria-disabled, not disabled: a disabled button drops keyboard focus. */}
              <SecondaryButton
                aria-disabled={busy}
                onClick={() => {
                  if (!busy) void choose(!pref.subscribed);
                }}
              >
                {pref.subscribed ? "Unsubscribe" : "Subscribe again"}
              </SecondaryButton>
              <span
                className={cx(
                  "text-[13px]/4 font-medium",
                  pref.subscribed ? "text-green" : "text-ink-3",
                )}
              >
                {pref.subscribed ? "Subscribed" : "Unsubscribed"}
              </span>
            </div>
          </div>
          {/* Always mounted, so screen readers announce the text when it changes. */}
          <p role="status" className="min-h-[18px] text-[13px]/[18px] text-ink-2">
            {status}
          </p>
          <FormError>{error}</FormError>
          <p className="text-[13px]/[19px] text-ink-2">
            Signed in? You can also change this in{" "}
            <Link
              to="/account"
              className={cx("rounded-sm font-medium text-green hover:underline", focusRing)}
            >
              Settings
            </Link>
            .
          </p>
        </>
      ) : (
        <div className="flex flex-col gap-2">
          <h1 className="text-[28px]/[34px] font-semibold tracking-[-0.025em]">
            {pref.state === "invalid"
              ? "This link doesn't work"
              : "Unable to load your preferences"}
          </h1>
          <p className="text-sm/[22px] text-ink-2">
            {pref.state === "invalid"
              ? "The link may be incomplete. Use the Unsubscribe link in the email instead, or sign in and change your email preferences in Settings."
              : "Try again in a minute. You can also use the Unsubscribe link in the email."}
          </p>
          <Link
            to="/account"
            className={cx(
              "mt-2 self-start rounded-sm text-sm font-medium text-green hover:underline",
              focusRing,
            )}
          >
            Go to Settings
          </Link>
        </div>
      )}
    </AuthLayout>
  );
}
