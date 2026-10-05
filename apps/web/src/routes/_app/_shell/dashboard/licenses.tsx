import { createFileRoute } from "@tanstack/react-router";

import { MacList } from "#/components/app/mac-list";
import { usePlaceholderAction } from "#/components/app/notice";
import { Badge, Card, PageTitle, TextButton } from "#/components/app/ui";
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
  const placeholder = usePlaceholderAction();

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
                  <Badge size="sm" tone={license.product === "Pro" ? "green" : "neutral"}>
                    {license.product.toUpperCase()}
                  </Badge>
                </div>
                <div className="flex flex-col gap-1">
                  <span className="inline-block py-[3px] font-mono text-base/5 font-medium">
                    {license.maskedKey}
                  </span>
                  <p className="text-[13px]/4 text-ink-2">
                    {license.activeMacs > 0
                      ? `Active on ${plural(license.activeMacs, "Mac")}`
                      : "Not active on any Mac"}
                  </p>
                </div>
                <div className="mt-auto h-px shrink-0 bg-divider" />
                <div className="flex items-center justify-between gap-3">
                  <span className="font-mono text-xs/4 text-ink-2">{license.updatesLabel}</span>
                  {/* PLACEHOLDER: the full key will come from the account API. */}
                  <TextButton
                    onClick={() => placeholder("Copying your license key")}
                    aria-label={`Copy ${license.product} license key`}
                  >
                    Copy key
                  </TextButton>
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
