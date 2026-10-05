import { createFileRoute } from "@tanstack/react-router";

import { CodeSample } from "#/components/app/code-sample";
import { usePlaceholderAction } from "#/components/app/notice";
import { UsageChart } from "#/components/app/usage-chart";
import {
  Card,
  ChevronIcon,
  ExternalIcon,
  PageTitle,
  PrimaryButton,
  SecondaryLink,
  SectionTitle,
  TextButton,
  cx,
  focusRing,
  table,
} from "#/components/app/ui";
import { getApiOverview } from "#/lib/account";
import { links } from "#/lib/config";
import { formatDate, formatNumber, formatShortDate } from "#/lib/format";

export const Route = createFileRoute("/_app/_shell/dashboard/api")({
  head: () => ({ meta: [{ title: "API · convt" }] }),
  loader: () => getApiOverview(),
  component: ApiPage,
});

// PLACEHOLDER: docs.convt.app does not exist yet; see `links` in src/lib/config.ts.
const docLinks = [
  { href: links.apiReference, title: "API reference", body: "Endpoints, options and errors" },
  { href: links.formats, title: "Supported formats", body: "Every input and target the API takes" },
  { href: links.webhooks, title: "Webhooks", body: "Get notified when long jobs finish" },
];

function ApiPage() {
  const api = Route.useLoaderData();
  // PLACEHOLDER: creating and revoking keys needs the API service (plan P9).
  const placeholder = usePlaceholderAction();

  return (
    <div className="flex flex-col gap-7">
      <div className="flex flex-wrap items-end justify-between gap-4">
        <div className="flex flex-col gap-2">
          <PageTitle>API</PageTitle>
          <p className="text-[13px]/4 text-ink-2">
            Convert files from your own code. Billed per conversion at the end of each month.
          </p>
        </div>
        <div className="flex items-center gap-2">
          <SecondaryLink href={links.docs} className="gap-1.5 px-3.5 py-2">
            API docs
            <span className="text-ink-2">
              <ExternalIcon />
            </span>
          </SecondaryLink>
          <PrimaryButton onClick={() => placeholder("Creating an API key")}>
            Create key
          </PrimaryButton>
        </div>
      </div>

      <Card className="flex flex-col md:flex-row">
        <dl className="grid grid-cols-3 gap-5.5 border-b border-line p-6 md:flex md:w-[260px] md:shrink-0 md:flex-col md:border-r md:border-b-0">
          <div className="flex flex-col gap-1.5">
            <dt className="text-[13px]/4 text-ink-2">This month</dt>
            <dd className="font-mono text-[28px]/[34px] font-medium tracking-[-0.02em]">
              {formatNumber(api.thisMonth)}
            </dd>
            <dd className="text-xs/4 text-ink-3">Conversions since {formatShortDate(api.since)}</dd>
          </div>
          <div className="flex flex-col gap-1.5">
            <dt className="text-[13px]/4 text-ink-2">Last 30 days</dt>
            <dd className="font-mono text-xl/6 font-medium">{formatNumber(api.last30Days)}</dd>
          </div>
          <div className="flex flex-col gap-1.5">
            <dt className="text-[13px]/4 text-ink-2">Failed</dt>
            <dd className="font-mono text-xl/6 font-medium">{formatNumber(api.failed)}</dd>
          </div>
        </dl>
        <div className="min-w-0 flex-1 p-6">
          <UsageChart days={api.perDay} />
        </div>
      </Card>

      <section aria-labelledby="keys-title" className="flex flex-col gap-3">
        <SectionTitle id="keys-title">Keys</SectionTitle>
        {api.keys.length === 0 ? (
          <Card className="px-6 py-4 text-[13px]/4 text-ink-2">
            No keys yet. Create one to start converting.
          </Card>
        ) : (
          <div className={table.wrap}>
            <table className={table.table}>
              <thead>
                <tr className={table.headRow}>
                  <th scope="col" className={`${table.th} w-[240px]`}>
                    Name
                  </th>
                  <th scope="col" className={table.th}>
                    Key
                  </th>
                  <th scope="col" className={`${table.th} w-[160px]`}>
                    Created
                  </th>
                  <th scope="col" className={`${table.th} w-[160px]`}>
                    Last used
                  </th>
                  <th scope="col" className={`${table.th} w-20`}>
                    <span className="sr-only">Actions</span>
                  </th>
                </tr>
              </thead>
              <tbody>
                {api.keys.map((key) => (
                  <tr key={key.id} className={table.row}>
                    <td className={`${table.td} font-medium`}>{key.name}</td>
                    <td className={`${table.td} font-mono text-xs/4`}>{key.maskedKey}</td>
                    <td className={`${table.td} text-ink-2`}>{formatDate(key.created)}</td>
                    <td className={`${table.td} text-ink-2`}>{key.lastUsed}</td>
                    <td className={`${table.td} text-right`}>
                      <TextButton
                        tone="danger"
                        onClick={() => placeholder(`Revoking "${key.name}"`)}
                        aria-label={`Revoke ${key.name}`}
                      >
                        Revoke
                      </TextButton>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </section>

      <section aria-labelledby="quick-start-title" className="flex flex-col gap-3">
        <SectionTitle id="quick-start-title">Quick start</SectionTitle>
        <div className="flex flex-col gap-4 lg:flex-row">
          <CodeSample />
          <Card className="flex flex-col overflow-clip lg:w-[340px] lg:shrink-0">
            <a
              href={links.docs}
              className={cx(
                "flex flex-col gap-1 border-b border-line px-5 pt-4 pb-3 hover:bg-hover",
                focusRing,
              )}
            >
              <span className="text-[13px]/4 font-semibold">Documentation</span>
              <span className="text-xs/4 text-ink-2">{new URL(links.docs).host}</span>
            </a>
            <ul>
              {docLinks.map((doc) => (
                <li key={doc.title} className="border-b border-line last:border-b-0">
                  <a
                    href={doc.href}
                    className={cx(
                      "flex items-center justify-between gap-3 px-5 py-3 hover:bg-hover",
                      focusRing,
                      "focus-visible:ring-inset focus-visible:ring-offset-0",
                    )}
                  >
                    <span className="flex flex-col gap-0.5">
                      <span className="text-[13px]/4 font-medium">{doc.title}</span>
                      <span className="text-xs/4 text-ink-3">{doc.body}</span>
                    </span>
                    <span className="text-ink-3">
                      <ChevronIcon />
                    </span>
                  </a>
                </li>
              ))}
            </ul>
          </Card>
        </div>
      </section>
    </div>
  );
}
