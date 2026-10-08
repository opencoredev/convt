import { Cancel01Icon, CheckmarkCircle02Icon, ComputerIcon } from "@hugeicons/core-free-icons";
import { createFileRoute, redirect, useNavigate } from "@tanstack/react-router";
import { useEffect, useRef, useState } from "react";

import {
  AuthScreen,
  AuthTitle,
  GoogleMark,
  QuietButton,
  Rise,
  darkPill,
  lightPill,
} from "#/components/app/auth-screen";
import { FormError } from "#/components/app/form-error";
import { SignInPanel } from "#/components/app/sign-in-panel";
import { Icon } from "#/components/icon";
import { openAppLink } from "#/lib/activate";
import { startSocialSignIn } from "#/lib/auth-client";
import { approveDeviceSignIn } from "#/server/device-fns";
import { getPublicConfig } from "#/server/public-config";
import { getSession } from "#/server/session";

// Desktop sign-in (P8): the app opens this page with a one-time state and the hash
// of a verifier it keeps. Approving hands the app a one-time code through
// convt://auth; the app trades it for a device token. See server/device-auth.ts.
//
// `provider` says which button the visitor pressed in the app. Signed out,
// provider=google goes straight to Google and back here; provider=email shows the
// email step with the field focused, and the code brings the visitor back here.

const deviceKeys = ["state", "challenge", "name", "os", "version"] as const;
type DeviceKey = (typeof deviceKeys)[number];
type Provider = "google" | "email";
type Search = Partial<Record<DeviceKey, string>> & { provider?: Provider };

const isProvider = (value: unknown): value is Provider => value === "google" || value === "email";

/** This page's own address without `provider`, so a return trip never restarts sign-in. */
function devicePath(search: Search): string {
  const params = new URLSearchParams();
  for (const key of deviceKeys) {
    const value = search[key];
    if (value !== undefined) params.set(key, value);
  }
  return `/device?${params.toString()}`;
}

export const Route = createFileRoute("/_app/device")({
  validateSearch: (search: Record<string, unknown>): Search => {
    const out: Search = {};
    for (const key of deviceKeys) {
      const value = search[key];
      if (typeof value === "string") out[key] = value;
    }
    if (isProvider(search.provider)) out.provider = search.provider;
    return out;
  },
  beforeLoad: async ({ search }) => {
    const session = await getSession();
    if (!session) {
      // The app named a provider: this page signs in itself. Otherwise the full sign-in.
      if (search.provider) return { email: null };
      throw redirect({ to: "/sign-in", search: { redirect: devicePath(search) } });
    }
    if (!session.user.emailVerified)
      throw redirect({ to: "/sign-in/verify-email", search: { redirect: devicePath(search) } });
    return { email: session.user.email };
  },
  loader: () => getPublicConfig(),
  head: () => ({ meta: [{ title: "Sign in to the app · convt" }] }),
  component: DevicePage,
});

const valid = (s: Search) =>
  /^[A-Za-z0-9_-]{43}$/.test(s.state ?? "") && /^[A-Za-z0-9_-]{43}$/.test(s.challenge ?? "");

function DevicePage() {
  const search = Route.useSearch();
  const { email } = Route.useRouteContext();

  if (!valid(search)) {
    return (
      <AuthScreen>
        <Rise index={1}>
          <AuthTitle sub="To sign in the app, open convt, go to Settings, then License, and click Sign in with convt.app. It opens this page with everything it needs.">
            This link didn't come from convt
          </AuthTitle>
        </Rise>
      </AuthScreen>
    );
  }

  if (email === null) return <SignedOut search={search} />;
  return <Approve search={search} email={email} />;
}

function SignedOut({ search }: { search: Search }) {
  const { providers } = Route.useLoaderData();
  const navigate = useNavigate();
  const back = devicePath(search);
  const showAll = () =>
    void navigate({ to: "/device", search: { ...search, provider: undefined } });

  // Google is the only provider here that needs no typing, so it can start at once.
  if (search.provider === "google" && providers.google)
    return (
      <GoogleRedirect
        callbackURL={back}
        onEmail={() => void navigate({ to: "/device", search: { ...search, provider: "email" } })}
      />
    );

  return (
    <AuthScreen>
      <SignInPanel
        available={providers}
        redirect={back}
        emailOnly={search.provider === "email"}
        onShowAll={showAll}
      />
    </AuthScreen>
  );
}

/** Leaves for Google as soon as the page loads; the buttons are for when that fails. */
function GoogleRedirect({ callbackURL, onEmail }: { callbackURL: string; onEmail: () => void }) {
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(true);
  const started = useRef(false);

  async function start() {
    setBusy(true);
    setError(null);
    const result = await startSocialSignIn({ provider: "google", callbackURL });
    if (!result.ok) {
      setBusy(false);
      setError(result.message);
    }
  }

  useEffect(() => {
    // Once: development mode runs effects twice.
    if (started.current) return;
    started.current = true;
    void start();
  });

  return (
    <AuthScreen>
      <Rise index={1}>
        <AuthTitle sub="Sign in with Google, then come back here to finish signing in the app.">
          {busy ? "Opening Google…" : "Continue with Google"}
        </AuthTitle>
      </Rise>
      <Rise index={2} className="mt-10 flex flex-col gap-4">
        <button
          type="button"
          disabled={busy}
          onClick={() => void start()}
          className={darkPill}
          aria-busy={busy}
        >
          {busy ? <Spinner /> : <GoogleMark />}
          Continue with Google
        </button>
        <FormError className="text-center">{error}</FormError>
        <div className="flex justify-center">
          <QuietButton onClick={onEmail}>Use email instead</QuietButton>
        </div>
      </Rise>
    </AuthScreen>
  );
}

function Spinner() {
  return (
    <span
      aria-hidden="true"
      className="size-4 shrink-0 animate-spin rounded-full border-2 border-current border-r-transparent motion-reduce:animate-none"
    />
  );
}

function Approve({ search, email }: { search: Search; email: string }) {
  const [phase, setPhase] = useState<"ask" | "busy" | "approved" | "cancelled">("ask");
  const [link, setLink] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  // Set by Cancel, so an approval still in flight never opens a second link.
  const cancelled = useRef(false);

  const name = search.name || "this computer";
  const os = search.os || "Unknown system";

  async function approve() {
    setPhase("busy");
    setError(null);
    try {
      const { provider: _provider, ...request } = search;
      const result = await approveDeviceSignIn({ data: request });
      if (cancelled.current) return;
      if (!result.ok) {
        setPhase("ask");
        setError("Too many sign-ins in the last hour. Wait a bit, then try again from convt.");
        return;
      }
      setLink(result.link);
      setPhase("approved");
      openAppLink(result.link);
    } catch {
      if (cancelled.current) return;
      setPhase("ask");
      setError("That didn't work. Try again.");
    }
  }

  function cancel() {
    if (phase === "busy") return;
    cancelled.current = true;
    setPhase("cancelled");
    openAppLink(`convt://auth?state=${search.state}&error=access_denied`);
  }

  if (phase === "approved") {
    return (
      <AuthScreen>
        <div data-testid="device-approved" className="flex w-full flex-col items-center">
          <Rise index={1} className="mt-10 flex justify-center">
            <span className="device-done flex size-16 items-center justify-center rounded-full bg-green-tint text-green shadow-[inset_0_0_0_1px_var(--green-line)]">
              <Icon icon={CheckmarkCircle02Icon} size={30} strokeWidth={1.6} />
            </span>
          </Rise>
          <Rise index={2}>
            <AuthTitle
              strong
              sub={
                <>
                  {search.name ? (
                    <span className="font-medium text-ink">{search.name}</span>
                  ) : (
                    "This computer"
                  )}{" "}
                  is signed in as {email}. convt fetches your Pro key on its own; you can close this
                  tab.
                </>
              }
            >
              You can go back to convt
            </AuthTitle>
          </Rise>
          <Rise index={3} className="mt-8 flex justify-center">
            <QuietButton onClick={() => link && openAppLink(link)}>Open convt again</QuietButton>
          </Rise>
        </div>
      </AuthScreen>
    );
  }

  if (phase === "cancelled") {
    return (
      <AuthScreen>
        <Rise index={1} className="mt-10 flex justify-center">
          <span className="flex size-14 items-center justify-center rounded-full bg-sunken text-ink-2 shadow-[inset_0_0_0_1px_var(--line)]">
            <Icon icon={Cancel01Icon} size={24} />
          </span>
        </Rise>
        <Rise index={2}>
          <AuthTitle
            strong
            sub={`${search.name || "This computer"} was not signed in. You can close this tab.`}
          >
            Sign-in cancelled
          </AuthTitle>
        </Rise>
      </AuthScreen>
    );
  }

  return (
    <AuthScreen>
      <Rise index={1}>
        <AuthTitle strong sub="Only approve if you just clicked Sign in inside the convt app.">
          Sign in to convt on <span data-testid="device-name">{name}</span>?
        </AuthTitle>
      </Rise>

      <Rise index={2} className="mt-8">
        <div className="flex items-center gap-3.5 rounded-2xl bg-raised/85 p-4 shadow-[inset_0_0_0_1px_var(--line),0_8px_24px_rgb(10_30_20/6%)] backdrop-blur-sm">
          <span className="flex size-10 shrink-0 items-center justify-center rounded-xl bg-green-tint text-green">
            <Icon icon={ComputerIcon} size={20} />
          </span>
          <div className="flex min-w-0 flex-col gap-0.5">
            <span className="truncate text-[15px]/5 font-medium">
              {search.name || "This computer"}
            </span>
            <span className="truncate font-mono text-[12px]/4 text-ink-2">
              {os}
              {search.version ? ` · convt ${search.version}` : ""}
            </span>
          </div>
        </div>
        <p className="mt-3 text-center text-[13px]/5 break-all text-ink-2">
          Signing in as <span className="text-ink">{email}</span>
        </p>
      </Rise>

      <Rise index={3} className="mt-8 flex flex-col gap-3">
        <FormError className="text-center">{error}</FormError>
        <button
          type="button"
          onClick={approve}
          disabled={phase === "busy"}
          data-testid="device-approve"
          className={darkPill}
        >
          {phase === "busy" ? <Spinner /> : null}
          {phase === "busy" ? "Approving…" : "Approve"}
        </button>
        <button
          type="button"
          onClick={cancel}
          disabled={phase === "busy"}
          data-testid="device-cancel"
          className={lightPill}
        >
          Cancel
        </button>
      </Rise>

      <Rise index={4} className="mt-6">
        <p className="text-center text-xs/[18px] text-pretty text-ink-2">
          While signed in, the app asks convt.app for your current Pro key once a day and when you
          click Refresh license. It sends this computer's sign-in token and the app version, never
          your files. Sign it out any time from your account.
        </p>
      </Rise>
    </AuthScreen>
  );
}
