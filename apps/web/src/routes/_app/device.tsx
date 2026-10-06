import { createFileRoute, redirect } from "@tanstack/react-router";
import { useRef, useState } from "react";

import { AuthLayout } from "#/components/app/auth-layout";
import { FormError } from "#/components/app/form-error";
import { PrimaryButton, SecondaryButton } from "#/components/app/ui";
import { openAppLink } from "#/lib/activate";
import { approveDeviceSignIn } from "#/server/device-fns";
import { getSession } from "#/server/session";

// Desktop sign-in (P8): the app opens this page with a one-time state and the hash
// of a verifier it keeps. Approving hands the app a one-time code through
// convt://auth; the app trades it for a device token. See server/device-auth.ts.

type Search = Record<string, string>;

const deviceAside = {
  title: "WHAT SIGNING IN DOES",
  items: [
    "Keeps your Pro key current in the app",
    "One license check a day, never your files",
    "Sign the computer out any time from Settings",
  ],
};

export const Route = createFileRoute("/_app/device")({
  validateSearch: (search: Record<string, unknown>): Search => {
    const out: Search = {};
    for (const key of ["state", "challenge", "name", "os", "version"])
      if (typeof search[key] === "string") out[key] = search[key] as string;
    return out;
  },
  beforeLoad: async ({ location }) => {
    const session = await getSession();
    if (!session) throw redirect({ to: "/sign-in", search: { redirect: location.href } });
    if (!session.user.emailVerified) throw redirect({ to: "/sign-in/verify-email" });
    return { email: session.user.email };
  },
  head: () => ({ meta: [{ title: "Sign in to the app · convt" }] }),
  component: DevicePage,
});

const valid = (s: Search) =>
  /^[A-Za-z0-9_-]{43}$/.test(s.state ?? "") && /^[A-Za-z0-9_-]{43}$/.test(s.challenge ?? "");

function DevicePage() {
  const search = Route.useSearch();
  const { email } = Route.useRouteContext();
  const [phase, setPhase] = useState<"ask" | "busy" | "approved" | "cancelled">("ask");
  const [link, setLink] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  // Set by Cancel, so an approval still in flight never opens a second link.
  const cancelled = useRef(false);

  if (!valid(search)) {
    return (
      <AuthLayout aside={deviceAside}>
        <h1 className="text-[28px]/[34px] font-semibold tracking-[-0.025em]">
          This link didn't come from convt
        </h1>
        <p className="text-sm/[22px] text-ink-2">
          To sign in the app, open convt, go to Settings, then License, and click Sign in with
          convt.app. It opens this page with everything it needs.
        </p>
      </AuthLayout>
    );
  }

  const name = search.name || "This computer";
  const os = search.os || "Unknown";

  async function approve() {
    setPhase("busy");
    setError(null);
    try {
      const result = await approveDeviceSignIn({ data: search });
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
      <AuthLayout aside={deviceAside}>
        <div className="flex flex-col gap-2" data-testid="device-approved">
          <h1 className="text-[28px]/[34px] font-semibold tracking-[-0.025em]">Return to convt</h1>
          <p className="text-sm/[22px] text-ink-2">
            Sign-in approved for {name} as {email}. Switch to convt to finish; it shows when it is
            signed in and fetches your Pro key. You can close this tab.
          </p>
        </div>
        <div>
          <SecondaryButton onClick={() => link && openAppLink(link)}>
            Open convt again
          </SecondaryButton>
        </div>
      </AuthLayout>
    );
  }

  if (phase === "cancelled") {
    return (
      <AuthLayout aside={deviceAside}>
        <h1 className="text-[28px]/[34px] font-semibold tracking-[-0.025em]">Sign-in cancelled</h1>
        <p className="text-sm/[22px] text-ink-2">
          {name} was not signed in. You can close this tab.
        </p>
      </AuthLayout>
    );
  }

  return (
    <AuthLayout aside={deviceAside}>
      <div className="flex flex-col gap-2">
        <h1 className="text-[28px]/[34px] font-semibold tracking-[-0.025em]">
          Sign in to convt on this computer?
        </h1>
        <p className="text-sm/[22px] text-ink-2">
          Only approve if you just clicked Sign in inside the convt app.
        </p>
      </div>
      <dl className="grid grid-cols-[96px_1fr] gap-x-4 gap-y-2.5 border-y border-line py-4 text-sm/4.5">
        <dt className="text-ink-2">Computer</dt>
        <dd data-testid="device-name" className="break-words">
          {name}
        </dd>
        <dt className="text-ink-2">System</dt>
        <dd className="font-mono text-[13px]/4.5">
          {os}
          {search.version ? ` · convt ${search.version}` : ""}
        </dd>
        <dt className="text-ink-2">Account</dt>
        <dd className="break-all">{email}</dd>
      </dl>
      <p className="text-[13px]/5 text-ink-2">
        While signed in, the app asks convt.app for your current Pro key once a day at launch and
        when you click Refresh license. It sends this computer's sign-in token and the app version,
        never your files. Sign it out any time under Settings on this site.
      </p>
      <FormError>{error}</FormError>
      <div className="flex gap-3">
        <PrimaryButton onClick={approve} disabled={phase === "busy"} data-testid="device-approve">
          {phase === "busy" ? "Approving…" : "Approve"}
        </PrimaryButton>
        <SecondaryButton
          onClick={cancel}
          disabled={phase === "busy"}
          data-testid="device-cancel"
          className="disabled:cursor-not-allowed disabled:opacity-60"
        >
          Cancel
        </SecondaryButton>
      </div>
    </AuthLayout>
  );
}
