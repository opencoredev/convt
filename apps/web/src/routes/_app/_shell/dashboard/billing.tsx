import { createFileRoute } from "@tanstack/react-router";

import { usePlaceholderAction } from "#/components/app/notice";
import {
  Badge,
  Card,
  PageTitle,
  SecondaryButton,
  SectionTitle,
  TextButton,
  table,
} from "#/components/app/ui";
import { getBilling } from "#/lib/account";
import { formatDate, formatMoney } from "#/lib/format";

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

function BillingPage() {
  const billing = Route.useLoaderData();
  // PLACEHOLDER: every billing action below needs the payment provider (plan P7).
  const placeholder = usePlaceholderAction();
  const otherInterval = billing.plan.interval === "year" ? "monthly" : "yearly";

  return (
    <div className="flex flex-col gap-7">
      <PageTitle>Billing</PageTitle>

      <Card className="flex flex-col md:flex-row">
        <section aria-labelledby="plan-title" className="flex flex-1 flex-col gap-3.5 p-6">
          <h2 className="text-[13px]/4 text-ink-2">Current plan</h2>
          <div className="flex flex-wrap items-baseline gap-2.5">
            <p id="plan-title" className="text-[22px]/7 font-semibold tracking-[-0.02em]">
              {billing.plan.name}
            </p>
            <Badge>{statusLabel[billing.plan.status]}</Badge>
          </div>
          <p className="text-[13px]/5 text-ink-2">{billing.plan.summary}</p>
          <div className="flex flex-wrap gap-2 pt-1">
            <SecondaryButton onClick={() => placeholder(`Switching to ${otherInterval}`)}>
              Switch to {otherInterval}
            </SecondaryButton>
            <button
              type="button"
              onClick={() => placeholder("Canceling your plan")}
              className="cursor-pointer rounded-lg px-3 py-[7px] text-[13px]/4 text-ink-2 outline-none hover:text-ink focus-visible:ring-2 focus-visible:ring-green"
            >
              Cancel plan
            </button>
          </div>
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
          <TextButton className="self-start" onClick={() => placeholder("Updating your card")}>
            {billing.card ? "Update card" : "Add card"}
          </TextButton>
        </section>
      </Card>

      <Card className="flex flex-wrap items-center gap-x-6 gap-y-2 px-6 py-4.5">
        <h2 className="text-[13px]/4 text-ink-2 sm:w-[200px] sm:shrink-0">Receipts go to</h2>
        <p className="min-w-0 flex-1 font-mono text-[13px]/4 break-all">{billing.receiptEmail}</p>
        <TextButton onClick={() => placeholder("Changing the receipt email")}>Change</TextButton>
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
                    <td className={table.td}>{invoice.description}</td>
                    <td className={`${table.td} text-right font-mono`}>
                      {formatMoney(invoice.amountCents)}
                    </td>
                    <td className={`${table.td} text-right`}>
                      <TextButton
                        onClick={() => placeholder("Downloading receipts")}
                        aria-label={`Receipt PDF for ${invoice.description}, ${formatDate(invoice.date)}`}
                      >
                        PDF
                      </TextButton>
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
