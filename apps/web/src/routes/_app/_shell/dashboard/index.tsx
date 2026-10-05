import { Link, createFileRoute } from "@tanstack/react-router";

import { MacList } from "#/components/app/mac-list";
import { usePlaceholderAction } from "#/components/app/notice";
import { Card, PageTitle, PrimaryLink, TextButton, cx, focusRing } from "#/components/app/ui";
import { getOverview } from "#/lib/account";
import { links } from "#/lib/config";
import { formatDate, formatNumber, formatShortDate, plural } from "#/lib/format";

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
  const placeholder = usePlaceholderAction();

  return (
    <div className="flex flex-col gap-6">
      <PageTitle>Overview</PageTitle>

      <section
        aria-label="Download"
        className="dither flex min-h-[220px] shrink-0 flex-col justify-end rounded-[14px] p-3 sm:p-5"
      >
        <div className="flex flex-col gap-4 rounded-[10px] bg-raised px-5 py-4.5 shadow-float sm:flex-row sm:items-center sm:justify-between">
          <div className="flex flex-col gap-1">
            <h2 className="text-[17px]/5.5 font-semibold tracking-[-0.02em]">convt for Mac</h2>
            <p className="text-[13px]/4 text-ink-2">
              Your license is on this account. Sign in inside the app to unlock it.
            </p>
          </div>
          <PrimaryLink href={links.downloadMac} className="shrink-0 self-start sm:self-auto">
            Download for macOS
          </PrimaryLink>
        </div>
      </section>

      <div className="grid gap-4 md:grid-cols-3">
        <SummaryCard
          label="Plan"
          value={
            <span className="text-[22px]/7 font-semibold tracking-[-0.02em]">{data.plan.name}</span>
          }
          detail={data.plan.priceLabel}
          meta={`Renews ${formatDate(data.plan.renewsOn)}`}
          action={
            <Link to="/dashboard/billing" className={linkAction}>
              Manage
            </Link>
          }
        />
        <SummaryCard
          label="License key"
          value={
            <span className="inline-block py-[3px] font-mono text-base/5 font-medium">
              {data.license.maskedKey}
            </span>
          }
          detail={`Active on ${plural(data.license.activeMacs, "Mac")}`}
          meta={data.license.updatesLabel}
          action={
            // PLACEHOLDER: the full key will come from the account API.
            <TextButton onClick={() => placeholder("Copying your license key")}>
              Copy key
            </TextButton>
          }
        />
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
