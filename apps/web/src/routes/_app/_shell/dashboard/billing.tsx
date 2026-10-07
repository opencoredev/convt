import { createFileRoute, useRouter } from "@tanstack/react-router";
import { useState } from "react";

import { FormError } from "#/components/app/form-error";
import { useNotice } from "#/components/app/notice";
import {
  Badge,
  Card,
  PageTitle,
  PrimaryButton,
  SecondaryButton,
  SecondaryLink,
  SectionTitle,
  TextButton,
  table,
} from "#/components/app/ui";
import { getBilling } from "#/lib/account";
import { links } from "#/lib/config";
import { formatDate, formatMoney } from "#/lib/format";
import { openPortal, openReceipt, setPlanCancel, switchPlanInterval } from "#/server/billing-fns";

export const Route = createFileRoute("/_app/_shell/dashboard/billing")({
  head: () => ({ meta: [{ title: "Billing · convt" }] }),
  loader: () => getBilling(),
  component: BillingPage,
});

const statusLabel = {
  active: "ACTIVE",
  trialing: "TRIAL",
  past_due: "PAST DUE",
  canceled: "CANCELED",
} as const;

const apiLabel = {
  none: null,
  pending: "PENDING",
  enrolled: "ACTIVE",
  credit: "CREDIT",
  payment_failed: "PAYMENT FAILED",
  ended: "ENDED",
} as const;

/** Opens the provider's customer portal in this tab. */
function usePortal() {
  const notice = useNotice();
  const [busy, setBusy] = useState(false);
  const open = async () => {
    setBusy(true);
    try {
      const { url } = await openPortal();
      if (!url) throw new Error("no portal");
      window.location.assign(url);
    } catch {
      notice("Couldn't open billing management. Try again in a minute.");
      setBusy(false);
    }
  };
  return { open, busy };
}

type Confirm = "switch" | "cancel" | null;

function BillingPage() {
  const billing = Route.useLoaderData();
  const router = useRouter();
  const notice = useNotice();
  const portal = usePortal();
  const [confirm, setConfirm] = useState<Confirm>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const plan = billing.plan;
  const paidApi = billing.api.state !== "none" && billing.api.state !== "credit";
  const otherInterval = plan?.interval === "year" ? "month" : "year";
  const otherLabel = otherInterval === "year" ? "yearly" : "monthly";

  async function run(
    action: () => Promise<{ ok: true } | { ok: false; message: string }>,
    done: string,
  ) {
    setBusy(true);
    setError(null);
    try {
      const r = await action();
      if (r.ok) {
        setConfirm(null);
        notice(done);
        await router.invalidate();
      } else {
        setError(r.message);
      }
    } catch {
      setError("Something went wrong. Try again.");
    } finally {
      setBusy(false);
    }
  }

  const switchCopy =
    otherInterval === "year"
      ? "You're charged $96 today for a year of Pro, minus credit for the unused part of this month. A new key covering the year appears under Licenses."
      : "You switch to $12 a month today. The unused part of your year becomes credit that pays the next months.";
  const cancelCopy =
    plan?.status === "trialing"
      ? `Your trial continues until it ends, and then Pro stops. You won't be charged.`
      : `Pro stays active until ${plan?.cancelsOn ? formatDate(plan.cancelsOn) : "the end of this period"}. Your last key keeps working for every build released before then.`;

  return (
    <div className="flex flex-col gap-7">
      <PageTitle>Billing</PageTitle>

      <Card className="flex flex-col md:flex-row">
        <section aria-labelledby="plan-title" className="flex flex-1 flex-col gap-3.5 p-6">
          <h2 className="text-[13px]/4 text-ink-2">Current plan</h2>
          {plan ? (
            <>
              <div className="flex flex-wrap items-baseline gap-2.5">
                <p id="plan-title" className="text-[22px]/7 font-semibold tracking-[-0.02em]">
                  {plan.name}
                </p>
                <Badge
                  tone={
                    plan.status === "canceled" || plan.status === "past_due" || plan.cancelsOn
                      ? "neutral"
                      : "green"
                  }
                >
                  {plan.cancelsOn
                    ? `ENDS ${formatDate(plan.cancelsOn).toUpperCase()}`
                    : statusLabel[plan.status]}
                </Badge>
              </div>
              <p className="text-[13px]/5 text-ink-2">{plan.summary}</p>
              {confirm ? (
                <div
                  className="flex flex-col gap-3 rounded-lg bg-sunken p-4 ring-1 ring-line"
                  role="group"
                  aria-labelledby="confirm-title"
                >
                  <p id="confirm-title" className="text-[13px]/4 font-semibold">
                    {confirm === "switch"
                      ? `Switch to ${otherLabel} now?`
                      : plan.status === "trialing"
                        ? "Cancel your trial?"
                        : "Cancel Pro?"}
                  </p>
                  <p className="text-[13px]/5 text-ink-2">
                    {confirm === "switch" ? switchCopy : cancelCopy}
                  </p>
                  {error ? <FormError>{error}</FormError> : null}
                  <div className="flex flex-wrap gap-2">
                    {confirm === "switch" ? (
                      <PrimaryButton
                        disabled={busy}
                        onClick={() =>
                          run(
                            () => switchPlanInterval({ data: { to: otherInterval } }),
                            `Switched to ${otherLabel}.`,
                          )
                        }
                      >
                        {busy ? "Switching…" : `Switch to ${otherLabel}`}
                      </PrimaryButton>
                    ) : (
                      <SecondaryButton
                        disabled={busy}
                        className="text-error"
                        onClick={() =>
                          run(
                            () => setPlanCancel({ data: { kind: "pro", cancel: true } }),
                            plan.status === "trialing"
                              ? "Your trial won't convert."
                              : "Pro won't renew.",
                          )
                        }
                      >
                        {busy
                          ? "Canceling…"
                          : plan.status === "trialing"
                            ? "Cancel trial"
                            : "Cancel Pro"}
                      </SecondaryButton>
                    )}
                    <TextButton
                      tone="muted"
                      disabled={busy}
                      onClick={() => {
                        setConfirm(null);
                        setError(null);
                      }}
                    >
                      Keep my plan
                    </TextButton>
                  </div>
                </div>
              ) : (
                <div className="flex flex-wrap items-center gap-2 pt-1">
                  {plan.status === "canceled" ? (
                    billing.sales === "all" ? (
                      <SecondaryLink href={`/checkout/pro?interval=${plan.interval}`}>
                        Restart Pro
                      </SecondaryLink>
                    ) : (
                      <span className="text-[13px]/4 text-ink-2">Pro is coming soon.</span>
                    )
                  ) : plan.status === "past_due" ? (
                    <PrimaryButton disabled={portal.busy} onClick={portal.open}>
                      Update card
                    </PrimaryButton>
                  ) : plan.cancelsOn ? (
                    <SecondaryButton
                      disabled={busy}
                      onClick={() =>
                        run(
                          () => setPlanCancel({ data: { kind: "pro", cancel: false } }),
                          "Pro will renew.",
                        )
                      }
                    >
                      Resume Pro
                    </SecondaryButton>
                  ) : (
                    <>
                      <SecondaryButton onClick={() => setConfirm("switch")}>
                        Switch to {otherLabel}
                      </SecondaryButton>
                      <button
                        type="button"
                        onClick={() => setConfirm("cancel")}
                        className="cursor-pointer rounded-lg px-3 py-[7px] text-[13px]/4 text-ink-2 outline-none hover:text-ink focus-visible:ring-2 focus-visible:ring-green"
                      >
                        {plan.status === "trialing" ? "Cancel trial" : "Cancel plan"}
                      </button>
                    </>
                  )}
                  {!confirm && error ? <FormError>{error}</FormError> : null}
                </div>
              )}
            </>
          ) : (
            <>
              <p id="plan-title" className="text-[22px]/7 font-semibold tracking-[-0.02em]">
                No plan
              </p>
              <p className="text-[13px]/5 text-ink-2">
                {billing.hadPro
                  ? "Pro is $12 a month or $96 a year. Desktop is $29 once and shows up under invoices."
                  : "Pro is $12 a month or $96 a year, with a 7-day free trial. Desktop is $29 once and shows up under invoices."}
              </p>
              <div className="flex flex-wrap gap-2 pt-1">
                {billing.sales === "all" ? (
                  <SecondaryLink href="/checkout/pro?interval=month">
                    {billing.hadPro ? "Start Pro" : "Start free trial"}
                  </SecondaryLink>
                ) : (
                  <span className="text-[13px]/4 text-ink-2">Pro is coming soon.</span>
                )}
                <TextButton tone="muted" onClick={() => window.location.assign(links.pricing)}>
                  See pricing
                </TextButton>
              </div>
            </>
          )}
          {apiLabel[billing.api.state] ? (
            <div className="mt-1 flex flex-wrap items-center gap-x-2.5 gap-y-1 border-t border-line pt-3.5">
              <span className="text-[13px]/4 font-medium">API, pay per conversion</span>
              <Badge
                size="sm"
                tone={
                  billing.api.state === "enrolled" || billing.api.state === "credit"
                    ? "green"
                    : "neutral"
                }
              >
                {apiLabel[billing.api.state]}
              </Badge>
              <span className="text-[13px]/4 text-ink-2">
                {billing.api.state === "credit"
                  ? `${formatMoney(billing.api.spendCapCents ?? 0)} of credit, no card needed`
                  : billing.api.spendCapCents !== null
                    ? `Spend cap ${formatMoney(billing.api.spendCapCents)} a month`
                    : "No spend cap"}
                {billing.api.endsOn ? `. Ends ${formatDate(billing.api.endsOn)}` : ""}
              </span>
              <a
                href="/dashboard/api"
                className="text-[13px]/4 font-medium text-green hover:underline hover:underline-offset-2"
              >
                Manage
              </a>
            </div>
          ) : null}
        </section>
        <section
          aria-labelledby="card-title"
          className="flex flex-col gap-3.5 border-t border-line p-6 md:w-[340px] md:shrink-0 md:border-t-0 md:border-l"
        >
          <h2 id="card-title" className="text-[13px]/4 text-ink-2">
            Payment method
          </h2>
          {billing.card ? (
            <div className="flex items-center gap-2.5">
              <span className="flex h-6 w-9 shrink-0 items-center justify-center rounded bg-chip font-mono text-[9px]/3 font-semibold shadow-[inset_0_0_0_1px_var(--chip-line)]">
                {billing.card.brand}
              </span>
              <span className="font-mono text-[13px]/4">
                <span className="sr-only">Card ending in </span>
                <span aria-hidden="true">•••• </span>
                {billing.card.last4}
              </span>
              <span className="font-mono text-xs/4 text-ink-2">
                <span className="sr-only">expires </span>
                {billing.card.expires}
              </span>
            </div>
          ) : (
            <p className="text-[13px]/4 text-ink-2">No card on file.</p>
          )}
          <div className="flex flex-wrap gap-x-4 gap-y-2">
            {billing.card || plan || paidApi ? (
              <TextButton disabled={portal.busy} onClick={portal.open}>
                {billing.card ? "Update card" : "Add card"}
              </TextButton>
            ) : null}
            {plan || paidApi ? (
              <TextButton tone="muted" disabled={portal.busy} onClick={portal.open}>
                Manage billing
              </TextButton>
            ) : null}
          </div>
        </section>
      </Card>

      <Card className="flex flex-wrap items-center gap-x-6 gap-y-2 px-6 py-4.5">
        <h2 className="text-[13px]/4 text-ink-2 sm:w-[200px] sm:shrink-0">Receipts go to</h2>
        <p className="min-w-0 flex-1 font-mono text-[13px]/4 break-all">{billing.receiptEmail}</p>
        {plan || paidApi ? (
          <TextButton disabled={portal.busy} onClick={portal.open}>
            Change
          </TextButton>
        ) : null}
      </Card>

      <section aria-labelledby="invoices-title" className="flex flex-col gap-3">
        <SectionTitle id="invoices-title">Invoices</SectionTitle>
        {billing.invoices.length === 0 ? (
          <Card className="px-6 py-4 text-[13px]/4 text-ink-2">No invoices yet.</Card>
        ) : (
          <div className={table.wrap}>
            <table className={table.table}>
              <thead>
                <tr className={table.headRow}>
                  <th scope="col" className={`${table.th} w-[200px]`}>
                    Date
                  </th>
                  <th scope="col" className={table.th}>
                    Description
                  </th>
                  <th scope="col" className={`${table.th} w-[120px] text-right`}>
                    Amount
                  </th>
                  <th scope="col" className={`${table.th} w-[120px] text-right`}>
                    Receipt
                  </th>
                </tr>
              </thead>
              <tbody>
                {billing.invoices.map((invoice) => (
                  <tr key={invoice.id} className={table.row}>
                    <td className={table.td}>{formatDate(invoice.date)}</td>
                    <td className={table.td}>
                      {invoice.description}
                      {invoice.statusLabel ? (
                        <span className="ml-2 align-middle">
                          <Badge size="sm" tone="neutral">
                            {invoice.statusLabel.toUpperCase()}
                          </Badge>
                        </span>
                      ) : null}
                    </td>
                    <td className={`${table.td} text-right font-mono`}>
                      {formatMoney(invoice.amountCents)}
                    </td>
                    <td className={`${table.td} text-right`}>
                      <ReceiptButton
                        id={invoice.id}
                        label={`Receipt PDF for ${invoice.description}, ${formatDate(invoice.date)}`}
                      />
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </section>
    </div>
  );
}

/** The receipt URL is fetched on click; Polar issues receipts as merchant of record. */
function ReceiptButton({ id, label }: { id: string; label: string }) {
  const notice = useNotice();
  const [busy, setBusy] = useState(false);
  return (
    <TextButton
      disabled={busy}
      aria-label={label}
      onClick={async () => {
        setBusy(true);
        try {
          const { url } = await openReceipt({ data: { id } });
          if (!url) throw new Error("none");
          window.open(url, "_blank", "noopener,noreferrer");
        } catch {
          notice("That receipt isn't available yet.");
        } finally {
          setBusy(false);
        }
      }}
    >
      PDF
    </TextButton>
  );
}
