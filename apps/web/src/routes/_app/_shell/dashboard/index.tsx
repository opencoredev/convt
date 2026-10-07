import { Link, createFileRoute } from "@tanstack/react-router";

import { CopyKeyButton } from "#/components/app/license-actions";
import { MacList } from "#/components/app/mac-list";
import { Card, PageTitle, PrimaryLink, cx, focusRing } from "#/components/app/ui";
import { getOverview } from "#/lib/account";
import { links } from "#/lib/config";
import { formatNumber, formatShortDate, plural } from "#/lib/format";

export const Route = createFileRoute("/_app/_shell/dashboard/")({
  head: () => ({ meta: [{ title: "Overview · convt" }] }),
  loader: () => getOverview(),
  component: OverviewPage,
});

const linkAction = cx(
  "rounded-sm text-[13px]/4 font-medium text-green hover:underline hover:underline-offset-2",
  focusRing,
);

function OverviewPage() {
  const data = Route.useLoaderData();

  return (
    <div className="flex flex-col gap-6">
      <PageTitle>Overview</PageTitle>

      <section
        aria-label="Download"
        className="dither flex min-h-[220px] shrink-0 flex-col justify-end rounded-[14px] p-3 sm:p-5"
      >
        <div className="flex flex-col gap-4 rounded-[10px] bg-raised px-5 py-4.5 shadow-float sm:flex-row sm:items-center sm:justify-between">
          <div className="flex flex-col gap-1">
            <h2 className="text-[17px]/5.5 font-semibold tracking-[-0.02em]">Download convt</h2>
            <p className="text-[13px]/4 text-ink-2">
              {data.license
                ? "Your license is on this account. Sign in inside the app to unlock it."
                : "Every format works free for 7 days. No account or card to start."}
            </p>
          </div>
          <PrimaryLink href={links.download} className="shrink-0 self-start sm:self-auto">
            Download
          </PrimaryLink>
        </div>
      </section>

      <div className="grid gap-4 md:grid-cols-3">
        {data.plan ? (
          <SummaryCard
            label="Plan"
            value={
              <span className="text-[22px]/7 font-semibold tracking-[-0.02em]">
                {data.plan.name}
              </span>
            }
            detail={data.plan.priceLabel}
            meta={data.plan.meta}
            action={
              <Link to="/dashboard/billing" className={linkAction}>
                Manage
              </Link>
            }
          />
        ) : (
          <SummaryCard
            label="Plan"
            value={<span className="text-[22px]/7 font-semibold tracking-[-0.02em]">No plan</span>}
            detail="Desktop is $29 once. Pro is $12 a month."
            meta="7-day free trial"
            action={
              <span className="flex items-center gap-3">
                <a href={links.buyDesktop} className={linkAction}>
                  Get Desktop
                </a>
                <a href={links.buyPro} className={linkAction}>
                  Get Pro
                </a>
              </span>
            }
          />
        )}
        {data.license ? (
          <SummaryCard
            label="License key"
            value={
              <span className="inline-block py-[3px] font-mono text-base/5 font-medium">
                {data.license.maskedKey}
              </span>
            }
            detail={`Active on ${plural(data.license.activeMacs, "Mac")}`}
            meta={data.license.updatesLabel}
            action={<CopyKeyButton license={data.license} />}
          />
        ) : (
          <SummaryCard
            label="License key"
            value={
              <span className="text-[22px]/7 font-semibold tracking-[-0.02em]">No key yet</span>
            }
            detail="Buying Desktop or Pro puts a key here."
            meta="Works offline"
            action={
              <span className="flex items-center gap-3">
                <a href={links.buyDesktop} className={linkAction}>
                  Get Desktop
                </a>
                <a href={links.buyPro} className={linkAction}>
                  Get Pro
                </a>
              </span>
            }
          />
        )}
        {data.api ? (
          <SummaryCard
            label="API this month"
            value={
              <span className="text-[22px]/7 font-semibold tracking-[-0.02em]">
                {formatNumber(data.api.conversionsThisMonth)}
              </span>
            }
            detail={`Conversions since ${formatShortDate(data.api.since)}`}
            meta={plural(data.api.keyCount, "key")}
            action={
              <Link to="/dashboard/api" className={linkAction}>
                View usage
              </Link>
            }
          />
        ) : (
          <SummaryCard
            label="API"
            value={
              <span className="text-[22px]/7 font-semibold tracking-[-0.02em]">Not set up</span>
            }
            detail="Pay per conversion, no plan needed."
            meta="Same engines as the app"
            action={
              <a href={links.apiReference} className={linkAction}>
                API docs
              </a>
            }
          />
        )}
      </div>

      <MacList macs={data.macs} />
    </div>
  );
}

function SummaryCard({
  label,
  value,
  detail,
  meta,
  action,
}: {
  label: string;
  value: React.ReactNode;
  detail: string;
  meta: string;
  action: React.ReactNode;
}) {
  return (
    <Card className="flex flex-col gap-4 p-[22px]">
      <h2 className="text-[13px]/4 text-ink-2">{label}</h2>
      <div className="flex flex-col gap-1">
        {value}
        <p className="text-[13px]/4 text-ink-2">{detail}</p>
      </div>
      <div className="h-px shrink-0 bg-divider" />
      <div className="flex items-center justify-between gap-3">
        <span className="font-mono text-xs/4 text-ink-2">{meta}</span>
        {action}
      </div>
    </Card>
  );
}
