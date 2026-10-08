import { Link, createFileRoute } from "@tanstack/react-router";
import { useEffect, useState } from "react";

import { AuthLayout } from "#/components/app/auth-layout";
import { useNotice } from "#/components/app/notice";
import { PrimaryButton, SecondaryLink, TextButton, cx, focusRing } from "#/components/app/ui";
import { openActivationLink } from "#/lib/activate";
import { links } from "#/lib/config";
import {
  checkoutAside,
  checkoutOrigin,
  downloadAction,
  type CheckoutOrigin,
} from "#/lib/checkout-copy";
import { pollCheckout } from "#/lib/checkout-poll";
import type { Os } from "#/lib/platform";
import { fetchCheckoutResult } from "#/server/billing-fns";
import type { CheckoutView } from "#/server/views";
import { visitorOs } from "#/server/visitor-os";

// Where the provider sends the buyer back. The page renders with no order data;
// the key arrives only in the client-side server function call (private, no-store)
// and only for the browser holding the checkout's cookie or the owning account.
// No Paper artboard exists for these states; they use the sign-in layout.
export const Route = createFileRoute("/_app/checkout/success")({
  validateSearch: (
    search: Record<string, unknown>,
  ): { checkout_id?: string; error?: string; from?: "app" } => ({
    ...(typeof search.checkout_id === "string" ? { checkout_id: search.checkout_id } : {}),
    ...(typeof search.error === "string" ? { error: search.error } : {}),
    // Set when the desktop app opened the checkout (convt-billing adds it to the return URL).
    ...(checkoutOrigin(search.from) === "app" ? { from: "app" as const } : {}),
  }),
  // The visitor's OS names the download step; nothing about the order loads here.
  loader: async () => ({ os: await visitorOs() }),
  head: () => ({
    meta: [{ title: "Checkout · convt" }, { name: "referrer", content: "no-referrer" }],
  }),
  component: SuccessPage,
});

type State =
  | CheckoutView
  | { state: "loading" }
  | { state: "email" }
  | { state: "error"; reason: string };

const errors: Record<string, string> = {
  rate_limited:
    "Too many checkouts were started from here in the last hour. Wait a little and try again.",
  provider_error: "The payment provider didn't answer. Nothing was charged. Try again in a minute.",
  sign_in_required: "Sign in to start Pro.",
  needs_multiple_subscriptions: "API billing can't start yet. Nothing was charged.",
  already_enrolled: "This account already has API billing.",
};

function SuccessPage() {
  const { checkout_id: checkoutId, error, from } = Route.useSearch();
  const origin = checkoutOrigin(from);
  const { os } = Route.useLoaderData();
  const [state, setState] = useState<State>(
    error
      ? { state: "error", reason: error }
      : checkoutId
        ? { state: "loading" }
        : { state: "not_found", product: null, allowTrial: false },
  );
  useEffect(() => {
    if (!checkoutId || error) return;
    return pollCheckout({
      fetch: ({ sync, signal }) => fetchCheckoutResult({ data: { checkoutId, sync }, signal }),
      onState: setState,
    });
  }, [checkoutId, error]);

  return (
    <AuthLayout
      aside={checkoutAside({ kind: state.state === "trial" ? "trial" : "key", os, origin })}
    >
      <div aria-live="polite" className="flex flex-col gap-6">
        <Body state={state} os={os} origin={origin} />
      </div>
    </AuthLayout>
  );
}

function Heading({ children, eyebrow }: { children: React.ReactNode; eyebrow?: string }) {
  return (
    <div className="flex flex-col gap-2">
      {eyebrow ? <p className="font-mono text-[11px]/3.5 text-ink-2">{eyebrow}</p> : null}
      <h1 className="text-[28px]/9 font-semibold tracking-[-0.025em]">{children}</h1>
    </div>
  );
}

const lead = "text-sm/5 text-ink-2";

function Body({ state, os, origin }: { state: State; os: Os | null; origin: CheckoutOrigin }) {
  switch (state.state) {
    case "loading":
    case "pending": {
      // A trial charges nothing today, so don't call it a payment once we know.
      const trial = state.state === "pending" && state.allowTrial;
      return (
        <>
          <Heading eyebrow="CHECKOUT">
            {trial ? "Starting your trial" : "Confirming your payment"}
          </Heading>
          <p className={lead}>This usually takes a few seconds. Keep this page open.</p>
          <Progress />
        </>
      );
    }
    case "ready":
      return <Ready state={state} os={os} origin={origin} />;
    case "trial":
      return origin === "app" ? (
        <>
          <Heading eyebrow="CONVT PRO">Your trial is on</Heading>
          <p className={lead}>
            Go back to convt. It shows your trial within a few seconds, and you can close this tab.
          </p>
          <Actions secondary={{ href: "/dashboard/billing", label: "Manage billing" }} />
        </>
      ) : (
        <>
          <Heading eyebrow="CONVT PRO">Your trial has started</Heading>
          <p className={lead}>Download convt and sign in with this account to use Pro.</p>
          <Actions primary={downloadAction(os)} />
        </>
      );
    case "api_enrolled":
      return (
        <>
          <Heading eyebrow="CONVT API">API billing is on</Heading>
          <p className={lead}>
            Your card is saved and your spend cap is set. Conversions are billed at the end of each
            month.
          </p>
          <Actions primary={{ href: "/dashboard/api", label: "Go to API" }} />
        </>
      );
    case "email":
      return (
        <>
          <Heading eyebrow="CHECKOUT">Your key is on its way by email</Heading>
          <p className={lead}>
            The payment is taking longer than usual to confirm. As soon as it does, we email your
            license key. It also appears under Licenses after you sign in with the email you used at
            checkout.
          </p>
          <Actions primary={{ href: "/dashboard/licenses", label: "Go to Licenses" }} />
        </>
      );
    case "shown":
      return (
        <>
          <Heading eyebrow="CHECKOUT">This key was already shown</Heading>
          <p className={lead}>
            For your security a license key appears here once. It's in the email we sent, and under
            Licenses after you sign in with the email you used at checkout.
          </p>
          <Actions primary={{ href: "/dashboard/licenses", label: "Go to Licenses" }} />
        </>
      );
    case "failed":
      return (
        <>
          <Heading eyebrow="CHECKOUT">This checkout didn't complete</Heading>
          <p className={lead}>Nothing was charged. You can start again whenever you're ready.</p>
          <Actions primary={{ href: links.pricing, label: "See pricing" }} />
        </>
      );
    case "error":
      return (
        <>
          <Heading eyebrow="CHECKOUT">Checkout didn't start</Heading>
          <p className={lead}>
            {errors[state.reason] ?? "Something went wrong. Nothing was charged."}
          </p>
          <Actions primary={{ href: links.pricing, label: "Back to pricing" }} />
        </>
      );
    default:
      return (
        <>
          <Heading eyebrow="CHECKOUT">Nothing to show here</Heading>
          <p className={lead}>
            This page shows a license key only in the browser that paid for it. If you bought convt,
            your key is in your email, and under Licenses after you sign in with the email you used
            at checkout.
          </p>
          <Actions primary={{ href: "/dashboard/licenses", label: "Go to Licenses" }} />
        </>
      );
  }
}

function Ready({
  state,
  os,
  origin,
}: {
  state: Extract<CheckoutView, { state: "ready" }>;
  os: Os | null;
  origin: CheckoutOrigin;
}) {
  const notice = useNotice();
  const product = state.product === "pro" ? "convt Pro" : "convt Desktop";
  return (
    <>
      <Heading eyebrow={state.product === "pro" ? "CONVT PRO" : "CONVT DESKTOP"}>
        Thanks. Here's your license key.
      </Heading>
      <div className="flex flex-col gap-3 rounded-xl bg-raised p-4 ring-1 ring-line">
        <p className="text-xs/4 text-ink-2">{product} license key</p>
        <p data-testid="license-key" className="font-mono text-xs/[18px] break-all select-all">
          {state.token}
        </p>
        <div className="flex flex-wrap items-center gap-x-4 gap-y-2 pt-1">
          <PrimaryButton onClick={() => openActivationLink(state.token)}>
            Open in convt
          </PrimaryButton>
          <TextButton
            onClick={async () => {
              try {
                await navigator.clipboard.writeText(state.token);
                notice("License key copied.");
              } catch {
                notice("Couldn't copy the key. Select it and copy it instead.");
              }
            }}
          >
            Copy key
          </TextButton>
        </div>
      </div>
      <ul className="flex flex-col gap-1.5 text-[13px]/5 text-ink-2">
        <li>Includes lifetime desktop updates.</li>
        <li>We also emailed it to {state.maskedEmail}.</li>
        <li>Open in convt asks the app to confirm before it adds the key.</li>
      </ul>
      <Actions
        primary={origin === "app" ? undefined : downloadAction(os)}
        secondary={{ href: "/dashboard/licenses", label: "Go to Licenses" }}
      />
    </>
  );
}

function Actions({
  primary,
  secondary,
}: {
  primary?: { href: string; label: string };
  secondary?: { href: string; label: string };
}) {
  return (
    <div className="flex flex-wrap items-center gap-3">
      {primary ? (
        <SecondaryLink href={primary.href} className="px-3.5 py-2">
          {primary.label}
        </SecondaryLink>
      ) : null}
      {secondary ? (
        <a
          href={secondary.href}
          className={cx(
            "rounded-sm text-[13px]/4 font-medium text-green hover:underline hover:underline-offset-2",
            focusRing,
          )}
        >
          {secondary.label}
        </a>
      ) : null}
      <Link to="/" className={cx("rounded-sm text-[13px]/4 text-ink-2 hover:text-ink", focusRing)}>
        convt.app
      </Link>
    </div>
  );
}

function Progress() {
  return (
    <div
      role="progressbar"
      aria-label="Confirming"
      className="h-1 w-full overflow-hidden rounded-full bg-hover"
    >
      <div className="h-full w-1/3 animate-[convt-progress_1.4s_ease-in-out_infinite] rounded-full bg-green motion-reduce:animate-none" />
    </div>
  );
}
