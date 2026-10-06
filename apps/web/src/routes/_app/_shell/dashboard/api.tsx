import { useState } from "react";
import { addApiKey, removeApiKey, fetchApiSpend } from "#/server/cloud-fns";
import { createFileRoute, useRouter } from "@tanstack/react-router";

import { ApiEnrollmentCard } from "#/components/app/api-enrollment";
import { CodeSample } from "#/components/app/code-sample";
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
  loader: async () => ({ ...(await getApiOverview()), spend: await fetchApiSpend() }),
  component: ApiPage,
});

const docLinks = [
  { href: links.apiReference, title: "API reference", body: "Endpoints, options and errors" },
  { href: links.formats, title: "Supported formats", body: "Every input and target the API takes" },
  { href: links.apiReference, title: "Jobs and limits", body: "Upload, start, poll and download" },
];

function ApiPage() {
  const api = Route.useLoaderData();
  const router = useRouter();
  const [creating, setCreating] = useState(false);
  const [name, setName] = useState("");
  const [shownKey, setShownKey] = useState<string | null>(null);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  async function create() {
    setBusy(true);
    setError("");
    try {
      const result = await addApiKey({ data: { name } });
      setShownKey(result.key);
      setCreating(false);
      setName("");
      await router.invalidate();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Key creation failed. Try again.");
    } finally {
      setBusy(false);
    }
  }
  async function revoke(id: string) {
    setBusy(true);
    setError("");
    try {
      await removeApiKey({ data: { id } });
      await router.invalidate();
    } catch {
      setError("Revoking the key failed. Try again.");
    } finally {
      setBusy(false);
    }
  }

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
          <PrimaryButton
            disabled={busy || api.sales !== "all" || api.enrollment.state !== "enrolled"}
            title={
              api.enrollment.state !== "enrolled" ? "Add a card under API billing first" : undefined
            }
            onClick={() => {
              setCreating(true);
              setShownKey(null);
            }}
          >
            {api.sales === "all" ? "Create key" : "Coming soon"}
          </PrimaryButton>
        </div>
      </div>

      {api.sales === "all" ? (
        <SecondaryLink href="/dashboard/api/convert" className="self-start">
          Convert in your browser
        </SecondaryLink>
      ) : (
        <p className="text-sm text-ink-2">Cloud conversions are coming soon.</p>
      )}
      {error && (
        <p role="alert" className="text-sm text-error">
          {error}
        </p>
      )}
      {creating && (
        <Card className="flex flex-col gap-3 p-5">
          <label htmlFor="key-name" className="text-sm font-medium">
            Key name
          </label>
          <input
            id="key-name"
            value={name}
            maxLength={80}
            onChange={(e) => setName(e.target.value)}
            placeholder="Production server"
            className={`rounded-lg border border-line bg-page px-3 py-2 text-sm ${focusRing}`}
          />
          <div className="flex gap-3">
            <PrimaryButton disabled={busy || !name.trim()} onClick={create}>
              {busy ? "Creating…" : "Create API key"}
            </PrimaryButton>
            <TextButton onClick={() => setCreating(false)}>Cancel</TextButton>
          </div>
        </Card>
      )}
      {shownKey && (
        <Card className="flex flex-col gap-3 p-5">
          <SectionTitle>Your new API key</SectionTitle>
          <p className="text-sm text-ink-2">Save this key now. It will not be shown again.</p>
          <input
            aria-label="New API key"
            readOnly
            value={shownKey}
            onFocus={(e) => e.target.select()}
            className={`w-full rounded-lg border border-line bg-page px-3 py-2 font-mono text-xs ${focusRing}`}
          />
          <TextButton className="self-start" onClick={() => setShownKey(null)}>
            I saved the key
          </TextButton>
        </Card>
      )}
      <Card className="flex flex-wrap gap-6 px-6 py-4 text-sm">
        <div>
          <span className="text-ink-2">Spent </span>
          <span className="font-mono">${(api.spend.used / 100).toFixed(2)}</span>
        </div>
        <div>
          <span className="text-ink-2">Reserved </span>
          <span className="font-mono">${(api.spend.reserved / 100).toFixed(2)}</span>
        </div>
        <div>
          <span className="text-ink-2">Spend cap </span>
          <span className="font-mono">${(api.spend.limit / 100).toFixed(2)}</span>
        </div>
        {api.spend.allowed && api.spend.used + api.spend.reserved >= api.spend.limit && (
          <p role="status" className="text-error">
            Spend cap reached. Raise it under API billing to create more jobs.
          </p>
        )}
      </Card>
      <ApiEnrollmentCard
        enrollment={api.enrollment}
        blocked={api.enrollBlocked}
        available={api.sales === "all"}
      />

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
                  <th scope="col" className={`${table.th} relative w-20`}>
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
                        disabled={busy}
                        onClick={() => revoke(key.id)}
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
              <span className="text-xs/4 text-ink-2">
                {links.docs.startsWith("/") ? "convt.app" : new URL(links.docs).host}
              </span>
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
