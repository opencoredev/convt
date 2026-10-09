import { Link, createFileRoute, useRouter } from "@tanstack/react-router";
import { useId, useState } from "react";

import { CodeInput } from "#/components/app/code-input";
import { FormError } from "#/components/app/form-error";
import { useNotice, usePlaceholderAction } from "#/components/app/notice";
import {
  Badge,
  Card,
  PageTitle,
  SecondaryButton,
  TextButton,
  cx,
  focusRing,
  table,
} from "#/components/app/ui";
import { getAccountSettings } from "#/lib/account";
import {
  authErrorMessage,
  changeEmail,
  linkSocial,
  requestEmailChange,
  signOut,
  unlinkAccount,
  type AuthResult,
} from "#/lib/auth-client";
import { signInCodeLength } from "#/lib/config";
import type { SignInMethod } from "#/lib/types";
import { endOtherSessions, endSession, saveName } from "#/server/account-fns";
import { saveMarketingPreference } from "#/server/marketing-fns";
import { deleteAccount } from "#/server/billing-fns";

export const Route = createFileRoute("/_app/_shell/account")({
  validateSearch: (search: Record<string, unknown>): { error?: string } =>
    typeof search.error === "string" ? { error: search.error } : {},
  head: () => ({ meta: [{ title: "Settings · convt" }] }),
  loader: () => getAccountSettings(),
  component: SettingsPage,
});

// The shared tables need 720px. Below md each row becomes a two-line grid instead:
// name and action on top, the account or client and its status underneath, so emails
// and device names stay readable without sideways scrolling. The roles keep the table
// semantics that `display: block` and `grid` would otherwise drop in some browsers.
const stacked = {
  table: cx(table.table, "max-md:block max-md:min-w-0"),
  body: "max-md:block",
  row: cx(
    table.row,
    "max-md:grid max-md:grid-cols-[minmax(0,1fr)_auto] max-md:items-baseline max-md:gap-x-4 max-md:gap-y-1 max-md:px-6 max-md:py-3.5",
  ),
  cell: cx(table.td, "max-md:w-auto! max-md:p-0!"),
  /** Top line, leading: the method or device name. */
  name: "max-md:col-start-1 max-md:row-start-1",
  /** Second line, leading: the account or client. */
  detail: "max-md:col-start-1 max-md:row-start-2",
  /** Second line, trailing: status or last active. */
  meta: "max-md:col-start-2 max-md:row-start-2 max-md:text-right",
  /** Top line, trailing: the row's action. */
  action: "max-md:col-start-2 max-md:row-start-1",
};

function CardHeader({
  id,
  title,
  body,
  children,
}: {
  id: string;
  title: string;
  body: string;
  children?: React.ReactNode;
}) {
  return (
    <div className="flex flex-wrap items-center justify-between gap-4 border-b border-line px-6 py-5">
      <div className="flex flex-col gap-1">
        <h2 id={id} className="text-[15px]/4.5 font-semibold">
          {title}
        </h2>
        <p className="text-[13px]/4 text-ink-2">{body}</p>
      </div>
      {children}
    </div>
  );
}

function SettingsPage() {
  const settings = Route.useLoaderData();
  const { error: linkError } = Route.useSearch();
  // Apple is hidden until it is configured (CNV-20); its branch below waits for that.
  const placeholder = usePlaceholderAction();
  const notice = useNotice();
  const router = useRouter();
  const nameId = useId();
  const [methodsError, setMethodsError] = useState<string | null>(
    linkError ? authErrorMessage(linkError) : null,
  );
  const [stale, setStale] = useState(false);

  /** Runs an action that needs a recent sign-in; a stale session shows a sign-in link. */
  function fresh(result: AuthResult, onError: (message: string) => void) {
    if (result.ok) return true;
    if (result.code === "SESSION_NOT_FRESH") setStale(true);
    onError(result.message);
    return false;
  }

  async function saveNameValue(value: string) {
    if (value.trim() === settings.account.name) return;
    try {
      await saveName({ data: { name: value } });
      notice("Name saved.");
      await router.invalidate();
    } catch (e) {
      notice(e instanceof Error ? e.message : "Couldn't save your name.");
    }
  }

  async function removeMethod(method: SignInMethod) {
    if (!method.accountId) return;
    setMethodsError(null);
    if (fresh(await unlinkAccount(method.accountId), setMethodsError)) {
      notice(`${method.label} removed.`);
      await router.invalidate();
    }
  }

  async function connectMethod(method: SignInMethod) {
    if (method.id === "apple") return placeholder("Connecting Apple");
    if (method.id !== "github" && method.id !== "google") return;
    setMethodsError(null);
    fresh(await linkSocial(method.id), setMethodsError);
  }

  return (
    <div className="flex flex-col gap-7">
      <PageTitle>Settings</PageTitle>

      <Card>
        <section aria-labelledby="profile-title">
          <CardHeader id="profile-title" title="Profile" body="Shown on receipts and in the app." />
          <form
            className="flex flex-col gap-2 border-b border-line px-6 py-4 sm:flex-row sm:items-center sm:gap-6"
            onSubmit={(event) => {
              event.preventDefault();
              const input = event.currentTarget.elements.namedItem("name") as HTMLInputElement;
              void saveNameValue(input.value);
            }}
          >
            <label htmlFor={nameId} className="text-[13px]/4 text-ink-2 sm:w-[200px] sm:shrink-0">
              Name
            </label>
            <input
              id={nameId}
              name="name"
              autoComplete="name"
              defaultValue={settings.account.name}
              onBlur={(event) => void saveNameValue(event.currentTarget.value)}
              className="h-9 w-full rounded-lg bg-raised px-3 text-[13px]/4 text-ink shadow-input outline-none focus-visible:ring-2 focus-visible:ring-green sm:w-[360px]"
            />
          </form>
          <EmailRow
            email={settings.account.email}
            verified={settings.account.emailVerified}
            onStale={() => setStale(true)}
          />
        </section>
      </Card>

      <MarketingEmail subscribed={settings.marketing?.subscribed ?? null} />

      <section aria-labelledby="methods-title" className={table.wrap}>
        <CardHeader
          id="methods-title"
          title="Sign-in methods"
          body="Any of these gets you into the same account. Keep at least one."
        />
        {methodsError ? (
          <div className="flex flex-wrap items-center gap-x-3 gap-y-1 border-b border-line px-6 py-3">
            <FormError>{methodsError}</FormError>
            {stale ? <SignInAgain /> : null}
          </div>
        ) : null}
        <table role="table" className={stacked.table}>
          <thead className="sr-only">
            <tr>
              <th scope="col">Method</th>
              <th scope="col">Account</th>
              <th scope="col">Status</th>
              <th scope="col">Actions</th>
            </tr>
          </thead>
          <tbody role="rowgroup" className={stacked.body}>
            {settings.methods.map((method) => (
              <tr key={method.id} role="row" className={stacked.row}>
                <th
                  scope="row"
                  role="rowheader"
                  className={cx(stacked.cell, stacked.name, "w-[320px] text-left font-medium")}
                >
                  {method.label}
                </th>
                <td
                  role="cell"
                  className={cx(
                    stacked.cell,
                    stacked.detail,
                    "[overflow-wrap:anywhere]",
                    method.identity ? "font-mono text-xs/4 text-ink-2" : "text-ink-3",
                  )}
                >
                  {method.identity ?? "Not connected"}
                </td>
                <td role="cell" className={cx(stacked.cell, stacked.meta, "w-[160px] text-green")}>
                  {method.identity ? "Connected" : null}
                </td>
                <td role="cell" className={cx(stacked.cell, stacked.action, "w-20 text-right")}>
                  {method.identity ? (
                    method.removable ? (
                      <TextButton
                        tone="muted"
                        onClick={() => void removeMethod(method)}
                        aria-label={`Remove ${method.label}`}
                      >
                        Remove
                      </TextButton>
                    ) : null
                  ) : (
                    <TextButton
                      onClick={() => void connectMethod(method)}
                      aria-label={`Connect ${method.label}`}
                    >
                      Connect
                    </TextButton>
                  )}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </section>

      <section aria-labelledby="sessions-title" className={table.wrap}>
        <CardHeader
          id="sessions-title"
          title="Where you're signed in"
          body="Browsers and computers using this account. Signing out a computer frees its license seat."
        >
          <SecondaryButton
            onClick={async () => {
              try {
                const result = await endOtherSessions();
                const total = result.sessions + result.devices;
                notice(
                  total
                    ? `Signed out ${total} other ${total === 1 ? "session" : "sessions"}.`
                    : "No other sessions.",
                );
                await router.invalidate();
              } catch {
                notice("Couldn't sign out the other sessions. Try again.");
              }
            }}
          >
            Sign out everywhere else
          </SecondaryButton>
        </CardHeader>
        <table role="table" className={stacked.table}>
          <thead className="sr-only">
            <tr>
              <th scope="col">Device</th>
              <th scope="col">Client</th>
              <th scope="col">Last active</th>
              <th scope="col">Actions</th>
            </tr>
          </thead>
          <tbody role="rowgroup" className={stacked.body}>
            {settings.sessions.map((session) => (
              <tr key={session.id} role="row" className={stacked.row}>
                <th
                  scope="row"
                  role="rowheader"
                  className={cx(stacked.cell, stacked.name, "w-[320px] text-left font-medium")}
                >
                  <span className="flex flex-wrap items-center gap-x-2.5 gap-y-1 [overflow-wrap:anywhere]">
                    {session.name}
                    {session.current ? (
                      <Badge size="sm" tone="neutral">
                        THIS BROWSER
                      </Badge>
                    ) : null}
                  </span>
                </th>
                <td role="cell" className={cx(stacked.cell, stacked.detail, "text-ink-2")}>
                  {session.kind}
                </td>
                <td role="cell" className={cx(stacked.cell, stacked.meta, "w-[160px] text-ink-2")}>
                  {session.lastSeen}
                </td>
                <td role="cell" className={cx(stacked.cell, stacked.action, "w-20 text-right")}>
                  {session.current ? (
                    <TextButton
                      tone="muted"
                      onClick={async () => {
                        await signOut();
                        window.location.assign("/sign-in");
                      }}
                      aria-label="Sign out of this browser"
                    >
                      Sign out
                    </TextButton>
                  ) : (
                    <TextButton
                      tone="muted"
                      onClick={async () => {
                        try {
                          await endSession({ data: { id: session.id, type: session.type } });
                          notice(`Signed out ${session.name}.`);
                          await router.invalidate();
                        } catch {
                          notice(`Couldn't sign out ${session.name}. Try again.`);
                        }
                      }}
                      aria-label={`Sign out ${session.name}`}
                    >
                      Sign out
                    </TextButton>
                  )}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </section>

      <DeleteAccount email={settings.account.email} deletion={settings.deletion} />
    </div>
  );
}

/** Campaign email. Account and license email is not affected and has no switch. */
function MarketingEmail({ subscribed }: { subscribed: boolean | null }) {
  const notice = useNotice();
  const router = useRouter();
  const [busy, setBusy] = useState(false);

  async function choose(next: boolean) {
    setBusy(true);
    try {
      await saveMarketingPreference({ data: { subscribed: next } });
      notice(next ? "Subscribed to product news." : "Unsubscribed from product news.");
      await router.invalidate();
    } catch {
      notice("Unable to save your email preference. Try again.");
    } finally {
      setBusy(false);
    }
  }

  return (
    <Card>
      <section aria-labelledby="email-title">
        <CardHeader
          id="email-title"
          title="Email"
          body="Sign-in codes, license keys and billing notices always arrive."
        />
        <div className="flex flex-wrap items-center justify-between gap-x-6 gap-y-3 px-6 py-4">
          <div className="flex flex-col gap-1">
            <p className="text-[13px]/4 font-medium">Product news and tips</p>
            <p className="text-[13px]/[18px] text-ink-2">
              {subscribed === null
                ? "Unable to load this setting right now."
                : subscribed
                  ? "Occasional emails about new features, releases and offers."
                  : "You don't get product news or tips."}
            </p>
          </div>
          {subscribed === null ? null : (
            <div className="flex items-center gap-4">
              <span className={cx("text-[13px]/4", subscribed ? "text-green" : "text-ink-3")}>
                {subscribed ? "Subscribed" : "Unsubscribed"}
              </span>
              {/* aria-disabled, not disabled: a disabled button drops keyboard focus. */}
              <SecondaryButton
                aria-disabled={busy}
                onClick={() => {
                  if (!busy) void choose(!subscribed);
                }}
              >
                {subscribed ? "Unsubscribe" : "Subscribe"}
              </SecondaryButton>
            </div>
          )}
        </div>
      </section>
    </Card>
  );
}

/**
 * Deletion ends Pro and API billing first (an immediate end, no refund), then
 * removes the account. It needs a sign-in within the last hour and the email typed.
 */
function DeleteAccount({
  email,
  deletion,
}: {
  email: string;
  deletion: { status: string; started: string } | null;
}) {
  const inputId = useId();
  const [open, setOpen] = useState(false);
  const [typed, setTyped] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [stale, setStale] = useState(false);
  const [started, setStarted] = useState(deletion !== null);

  async function confirmDelete() {
    setBusy(true);
    setError(null);
    try {
      const r = await deleteAccount({ data: { email: typed } });
      if (!r.ok) {
        setStale(r.code === "SESSION_NOT_FRESH");
        setError(r.message);
        return;
      }
      setStarted(true);
      // The account is gone within seconds; this browser's session goes with it.
      setTimeout(() => window.location.assign("/"), 6000);
    } catch {
      setError("Something went wrong. Try again.");
    } finally {
      setBusy(false);
    }
  }

  if (started) {
    return (
      <section
        aria-labelledby="delete-title"
        className="flex flex-col gap-1 rounded-xl bg-raised px-6 py-5 ring-1 ring-error-line"
        role="status"
      >
        <h2 id="delete-title" className="text-[15px]/4.5 font-semibold">
          Deleting your account
        </h2>
        <p className="text-[13px]/5 text-ink-2">
          We're ending your subscriptions, then removing the account. Every other browser and
          computer is already signed out. If this takes more than a few minutes, it continues in the
          background and finishes on its own.
        </p>
      </section>
    );
  }

  return (
    <section
      aria-labelledby="delete-title"
      className="flex flex-col gap-4 rounded-xl bg-raised px-6 py-5 ring-1 ring-error-line"
    >
      <div className="flex flex-wrap items-center justify-between gap-4">
        <div className="flex flex-col gap-1">
          <h2 id="delete-title" className="text-[15px]/4.5 font-semibold">
            Delete account
          </h2>
          <p className="text-[13px]/4 text-ink-2">
            Cancels Pro, revokes API keys and signs out every computer. Receipts stay with our
            payment provider.
          </p>
        </div>
        {open ? null : (
          <button
            type="button"
            onClick={() => setOpen(true)}
            className="cursor-pointer rounded-lg bg-raised px-3 py-[7px] text-[13px]/4 font-medium text-error shadow-[rgb(0_0_0/4%)_0_-1px_0_inset,var(--error-ring)_0_0_0_1px,rgb(0_0_0/6%)_0_1px_2px] outline-none hover:bg-hover focus-visible:ring-2 focus-visible:ring-error dark:bg-sunken"
          >
            Delete account
          </button>
        )}
      </div>
      {open ? (
        <form
          className="flex flex-col gap-3 border-t border-line pt-4"
          onSubmit={(event) => {
            event.preventDefault();
            void confirmDelete();
          }}
        >
          <p className="text-[13px]/5 text-ink-2">
            Pro and API billing end now, without a refund for the rest of the period. Your license
            keys keep working offline in the builds they cover, but they leave your dashboard. This
            can't be undone.
          </p>
          <label htmlFor={inputId} className="text-[13px]/4 font-medium">
            Type <span className="font-mono">{email}</span> to confirm
          </label>
          <input
            id={inputId}
            autoComplete="off"
            spellCheck={false}
            value={typed}
            onChange={(e) => setTyped(e.target.value)}
            className="h-9 w-full rounded-lg bg-raised px-3 font-mono text-[13px]/4 text-ink shadow-input outline-none focus-visible:ring-2 focus-visible:ring-error sm:w-[360px]"
          />
          <div className="flex flex-wrap items-center gap-x-3 gap-y-1">
            <FormError>{error}</FormError>
            {stale ? <SignInAgain /> : null}
          </div>
          <div className="flex flex-wrap items-center gap-3">
            <button
              type="submit"
              disabled={busy || typed.trim().toLowerCase() !== email.toLowerCase()}
              className="cursor-pointer rounded-lg bg-error px-3 py-[7px] text-[13px]/4 font-medium text-white outline-none hover:opacity-90 focus-visible:ring-2 focus-visible:ring-error focus-visible:ring-offset-2 focus-visible:ring-offset-page disabled:cursor-not-allowed disabled:opacity-50"
            >
              {busy ? "Deleting…" : "Delete my account"}
            </button>
            <TextButton tone="muted" onClick={() => setOpen(false)}>
              Keep my account
            </TextButton>
          </div>
        </form>
      ) : null}
    </section>
  );
}

function SignInAgain() {
  return (
    <Link
      to="/sign-in"
      search={{ redirect: "/account" }}
      onClick={async (event) => {
        // A fresh session needs a new sign-in, so end this one first.
        event.preventDefault();
        await signOut();
        window.location.assign("/sign-in?redirect=%2Faccount");
      }}
      className={cx(
        "rounded-sm text-[13px]/4 font-medium text-green hover:underline hover:underline-offset-2",
        focusRing,
      )}
    >
      Sign in again
    </Link>
  );
}

/** The email row: shows the address, and changes it with a code sent to the new one. */
function EmailRow({
  email,
  verified,
  onStale,
}: {
  email: string;
  verified: boolean;
  onStale: () => void;
}) {
  const router = useRouter();
  const notice = useNotice();
  const inputId = useId();
  const codeLabelId = useId();
  const [step, setStep] = useState<"view" | "enter" | "code">("view");
  const [newEmail, setNewEmail] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [stale, setStale] = useState(false);
  const [busy, setBusy] = useState(false);
  const [attempt, setAttempt] = useState(0);

  function fail(result: Exclude<AuthResult, { ok: true }>) {
    if (result.code === "SESSION_NOT_FRESH") {
      setStale(true);
      onStale();
    }
    setError(result.message);
  }

  return (
    <div className="flex flex-col gap-3 px-6 py-4">
      <div className="flex flex-wrap items-center gap-x-6 gap-y-2">
        <span className="text-[13px]/4 text-ink-2 sm:w-[200px] sm:shrink-0">Email</span>
        <span className="flex min-w-0 flex-1 flex-wrap items-center gap-2.5">
          <span className="font-mono text-[13px]/4 break-all">{email}</span>
          {verified ? <Badge size="sm">VERIFIED</Badge> : null}
        </span>
        {step === "view" ? (
          <TextButton onClick={() => setStep("enter")}>Change email</TextButton>
        ) : (
          <TextButton
            tone="muted"
            onClick={() => {
              setStep("view");
              setError(null);
              setStale(false);
            }}
          >
            Cancel
          </TextButton>
        )}
      </div>
      {step === "enter" ? (
        <form
          className="flex flex-col gap-2 sm:flex-row sm:items-center sm:gap-6"
          onSubmit={async (event) => {
            event.preventDefault();
            setBusy(true);
            setError(null);
            const result = await requestEmailChange(newEmail);
            setBusy(false);
            if (result.ok) setStep("code");
            else fail(result);
          }}
        >
          <label htmlFor={inputId} className="text-[13px]/4 text-ink-2 sm:w-[200px] sm:shrink-0">
            New email
          </label>
          <span className="flex flex-wrap items-center gap-3">
            <input
              id={inputId}
              type="email"
              required
              autoComplete="email"
              value={newEmail}
              onChange={(event) => setNewEmail(event.target.value)}
              className="h-9 w-full rounded-lg bg-raised px-3 text-[13px]/4 text-ink shadow-input outline-none focus-visible:ring-2 focus-visible:ring-green sm:w-[360px]"
            />
            <SecondaryButton type="submit" disabled={busy}>
              Send code
            </SecondaryButton>
          </span>
        </form>
      ) : null}
      {step === "code" ? (
        <div className="flex flex-col gap-2 sm:flex-row sm:items-start sm:gap-6">
          <p id={codeLabelId} className="text-[13px]/4 text-ink-2 sm:w-[200px] sm:shrink-0 sm:pt-3">
            Code sent to {newEmail}
          </p>
          <CodeInput
            key={attempt}
            length={signInCodeLength}
            labelId={codeLabelId}
            onComplete={async (code) => {
              setError(null);
              const result = await changeEmail(newEmail, code);
              if (result.ok) {
                setStep("view");
                notice("Email changed. Other sessions were signed out.");
                await router.invalidate();
              } else {
                fail(result);
                setAttempt((n) => n + 1);
              }
            }}
          />
        </div>
      ) : null}
      {error ? (
        <div className="flex flex-wrap items-center gap-x-3 gap-y-1 sm:pl-[224px]">
          <FormError>{error}</FormError>
          {stale ? <SignInAgain /> : null}
        </div>
      ) : null}
    </div>
  );
}
