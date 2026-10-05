import { createFileRoute } from "@tanstack/react-router";
import { useId } from "react";

import { usePlaceholderAction } from "#/components/app/notice";
import {
  Badge,
  Card,
  PageTitle,
  SecondaryButton,
  TextButton,
  cx,
  table,
} from "#/components/app/ui";
import { getAccountSettings } from "#/lib/account";

export const Route = createFileRoute("/_app/_shell/account")({
  head: () => ({ meta: [{ title: "Settings · convt" }] }),
  loader: () => getAccountSettings(),
  component: SettingsPage,
});

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
  // PLACEHOLDER: every change on this page needs the account service (plan P6).
  const placeholder = usePlaceholderAction();
  const nameId = useId();
  const connectedCount = settings.methods.filter((m) => m.identity).length;

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
              placeholder("Saving your name");
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
              onBlur={(event) => {
                if (event.currentTarget.value !== settings.account.name)
                  placeholder("Saving your name");
              }}
              className="h-9 w-full rounded-lg bg-raised px-3 text-[13px]/4 text-ink shadow-input outline-none focus-visible:ring-2 focus-visible:ring-green sm:w-[360px]"
            />
          </form>
          <div className="flex flex-wrap items-center gap-x-6 gap-y-2 px-6 py-4">
            <span className="text-[13px]/4 text-ink-2 sm:w-[200px] sm:shrink-0">Email</span>
            <span className="flex min-w-0 flex-1 flex-wrap items-center gap-2.5">
              <span className="font-mono text-[13px]/4 break-all">{settings.account.email}</span>
              {settings.account.emailVerified ? <Badge size="sm">VERIFIED</Badge> : null}
            </span>
            <TextButton onClick={() => placeholder("Changing your email")}>Change email</TextButton>
          </div>
        </section>
      </Card>

      <section aria-labelledby="methods-title" className={table.wrap}>
        <CardHeader
          id="methods-title"
          title="Sign-in methods"
          body="Any of these gets you into the same account. Keep at least one."
        />
        <table className={table.table}>
          <thead className="sr-only">
            <tr>
              <th scope="col">Method</th>
              <th scope="col">Account</th>
              <th scope="col">Status</th>
              <th scope="col">Actions</th>
            </tr>
          </thead>
          <tbody>
            {settings.methods.map((method) => (
              <tr key={method.id} className={table.row}>
                <th scope="row" className={`${table.td} w-[320px] text-left font-medium`}>
                  {method.label}
                </th>
                <td
                  className={cx(
                    table.td,
                    method.identity ? "font-mono text-xs/4 text-ink-2" : "text-ink-3",
                  )}
                >
                  {method.identity ?? "Not connected"}
                </td>
                <td className={`${table.td} w-[160px] text-green`}>
                  {method.identity ? "Connected" : null}
                </td>
                <td className={`${table.td} w-20 text-right`}>
                  {method.identity ? (
                    method.removable && connectedCount > 1 ? (
                      <TextButton
                        tone="muted"
                        onClick={() => placeholder(`Removing ${method.label}`)}
                        aria-label={`Remove ${method.label}`}
                      >
                        Remove
                      </TextButton>
                    ) : null
                  ) : (
                    // PLACEHOLDER: Google and Apple sign-in are not set up.
                    <TextButton
                      onClick={() => placeholder(`Connecting ${method.label}`)}
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
          body="Browsers and Macs using this account. Signing out a Mac frees its license seat."
        >
          <SecondaryButton onClick={() => placeholder("Signing out other sessions")}>
            Sign out everywhere else
          </SecondaryButton>
        </CardHeader>
        <table className={table.table}>
          <thead className="sr-only">
            <tr>
              <th scope="col">Device</th>
              <th scope="col">Client</th>
              <th scope="col">Last active</th>
              <th scope="col">Actions</th>
            </tr>
          </thead>
          <tbody>
            {settings.sessions.map((session) => (
              <tr key={session.id} className={table.row}>
                <th scope="row" className={`${table.td} w-[320px] text-left font-medium`}>
                  <span className="flex items-center gap-2.5">
                    {session.name}
                    {session.current ? (
                      <Badge size="sm" tone="neutral">
                        THIS BROWSER
                      </Badge>
                    ) : null}
                  </span>
                </th>
                <td className={`${table.td} text-ink-2`}>{session.kind}</td>
                <td className={`${table.td} w-[160px] text-ink-2`}>{session.lastSeen}</td>
                <td className={`${table.td} w-20 text-right`}>
                  {session.current ? null : (
                    <TextButton
                      tone="muted"
                      onClick={() => placeholder(`Signing out ${session.name}`)}
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

      <section
        aria-labelledby="delete-title"
        className="flex flex-wrap items-center justify-between gap-4 rounded-xl bg-raised px-6 py-5 ring-1 ring-error-line"
      >
        <div className="flex flex-col gap-1">
          <h2 id="delete-title" className="text-[15px]/4.5 font-semibold">
            Delete account
          </h2>
          <p className="text-[13px]/4 text-ink-2">
            Cancels Pro, revokes API keys and signs out every Mac. Receipts stay with our payment
            provider.
          </p>
        </div>
        <button
          type="button"
          onClick={() => placeholder("Deleting your account")}
          className="cursor-pointer rounded-lg bg-raised px-3 py-[7px] text-[13px]/4 font-medium text-error shadow-[rgb(0_0_0/4%)_0_-1px_0_inset,var(--error-ring)_0_0_0_1px,rgb(0_0_0/6%)_0_1px_2px] outline-none hover:bg-hover focus-visible:ring-2 focus-visible:ring-error dark:bg-sunken"
        >
          Delete account
        </button>
      </section>
    </div>
  );
}
