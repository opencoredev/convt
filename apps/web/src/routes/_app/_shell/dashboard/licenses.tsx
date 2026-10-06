import { createFileRoute } from "@tanstack/react-router";

import { ActivateButton, CopyKeyButton } from "#/components/app/license-actions";
import { MacList } from "#/components/app/mac-list";
import { Badge, Card, PageTitle, cx } from "#/components/app/ui";
import { getLicenses } from "#/lib/account";
import { plural } from "#/lib/format";

// No artboard exists for this page. It reuses the license card and the Macs list
// from the Overview design.
export const Route = createFileRoute("/_app/_shell/dashboard/licenses")({
  head: () => ({ meta: [{ title: "Licenses · convt" }] }),
  loader: () => getLicenses(),
  component: LicensesPage,
});

function LicensesPage() {
  const { licenses, macs } = Route.useLoaderData();

  return (
    <div className="flex flex-col gap-6">
      <PageTitle>Licenses</PageTitle>

      {licenses.length === 0 ? (
        <Card className="p-[22px] text-[13px]/4 text-ink-2">No licenses on this account yet.</Card>
      ) : (
        <ul className="grid gap-4 md:grid-cols-2">
          {licenses.map((license) => (
            <li key={license.id}>
              <Card className="flex h-full flex-col gap-4 p-[22px]">
                <div className="flex items-center justify-between gap-3">
                  <h2 className="text-[13px]/4 text-ink-2">{license.detail}</h2>
                  <span className="flex items-center gap-1.5">
                    {license.revokedBadge ? (
                      <Badge size="sm" tone="neutral">
                        {license.revokedBadge}
                      </Badge>
                    ) : null}
                    <Badge
                      size="sm"
                      tone={license.product === "Pro" && !license.revoked ? "green" : "neutral"}
                    >
                      {license.product.toUpperCase()}
                    </Badge>
                  </span>
                </div>
                <div className="flex flex-col gap-1">
                  <span
                    className={cx(
                      "inline-block py-[3px] font-mono text-base/5 font-medium",
                      license.revoked && "text-ink-2 line-through decoration-ink-3",
                    )}
                  >
                    {license.maskedKey}
                  </span>
                  {license.revoked ? null : (
                    <p className="text-[13px]/4 text-ink-2">
                      {license.activeMacs > 0
                        ? `Active on ${plural(license.activeMacs, "Mac")}`
                        : "Not active on any Mac"}
                    </p>
                  )}
                </div>
                <div className="mt-auto h-px shrink-0 bg-divider" />
                <div className="flex flex-wrap items-center justify-between gap-x-3 gap-y-2">
                  <span
                    className={
                      license.revoked
                        ? "text-[13px]/5 text-ink-2"
                        : "font-mono text-xs/4 text-ink-2"
                    }
                  >
                    {license.updatesLabel}
                  </span>
                  <span className="flex items-center gap-4">
                    <ActivateButton license={license} />
                    <CopyKeyButton license={license} />
                  </span>
                </div>
              </Card>
            </li>
          ))}
        </ul>
      )}

      <MacList macs={macs} />
    </div>
  );
}
