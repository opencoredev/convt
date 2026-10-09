import { useState, type ReactNode } from "react";
import { addApiKey, removeApiKey, fetchApiSpend } from "#/server/cloud-fns";
import { createFileRoute, useRouter } from "@tanstack/react-router";

import { ApiEnrollmentCard } from "#/components/app/api-enrollment";
import { CodePanel, CopyCode } from "#/components/app/code-panel";
import { CopyButton } from "#/components/app/copy-button";
import { UsageChart } from "#/components/app/usage-chart";
import {
  Card,
  ChevronIcon,
  ExternalIcon,
  Meter,
  PageTitle,
  PrimaryButton,
  SecondaryLink,
  SectionTitle,
  TextButton,
  cx,
  focusRing,
} from "#/components/app/ui";
import { getApiOverview } from "#/lib/account";
import { quickStart } from "#/lib/api-samples";
import { apiBaseUrl, links } from "#/lib/config";
import { formatDate, formatMoney, formatNumber, formatShortDate } from "#/lib/format";

export const Route = createFileRoute("/_app/_shell/dashboard/api")({
  head: () => ({ meta: [{ title: "API · convt" }] }),
  loader: async () => ({ ...(await getApiOverview()), spend: await fetchApiSpend() }),
  component: ApiPage,
});

const docLinks = [
  {
    href: links.docsQuickStart,
    title: "Quick start",
    body: "Your first conversion in four requests",
  },
  {
    href: links.docsErrors,
    title: "Errors",
    body: "Every error code and when to retry",
  },
  {
    href: links.docsLimits,
    title: "Limits and billing",
    body: "File size, rate limit, retention",
  },
  {
    href: links.docsFormats,
    title: "Supported conversions",
    body: "Every pair the cloud converts",
  },
];

function ApiPage() {
  const api = Route.useLoaderData();
  const router = useRouter();
  const [creating, setCreating] = useState(false);
  const [name, setName] = useState("");
  const [shownKey, setShownKey] = useState<string | null>(null);
  const [confirming, setConfirming] = useState<string | null>(null);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const onSale = api.sales === "all";
  const enrolled = api.enrollment.state === "enrolled";
  const canCreate = onSale && enrolled;

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
      setConfirming(null);
      await router.invalidate();
    } catch {
      setError("Revoking the key failed. Try again.");
    } finally {
      setBusy(false);
    }
  }

  const { used, reserved, limit } = api.spend;
  const capReached = api.spend.allowed && used + reserved >= limit;

  return (
    <div className="flex flex-col gap-8">
      <header className="flex flex-col gap-4 sm:flex-row sm:items-end sm:justify-between">
        <div className="flex flex-col gap-2">
          <PageTitle>API</PageTitle>
          <p className="max-w-[560px] text-[14px]/[21px] text-ink-2">
            Convert files from your own code with the same engines as the app. Billed per successful
            conversion at the end of each month.
          </p>
        </div>
        <div className="flex flex-wrap items-center gap-2">
          <SecondaryLink href={links.apiReference} className="gap-1.5 px-3.5 py-2">
            API reference
            <span className="text-ink-2">
              <ExternalIcon />
            </span>
          </SecondaryLink>
          {onSale ? (
            <SecondaryLink href="/dashboard/api/convert" className="px-3.5 py-2">
              Cloud converter
            </SecondaryLink>
          ) : null}
        </div>
      </header>

      {!onSale && (
        <Card className="flex items-center gap-3 px-5 py-4 text-[13px]/5 text-ink-2">
          <span aria-hidden="true" className="size-2 shrink-0 rounded-full bg-ink-3" />
          Cloud conversions are coming soon. Keys and billing open when the API goes on sale.
        </Card>
      )}

      <dl className="grid gap-3 sm:grid-cols-3">
        <Stat label="Spend this month">
          <dd className="font-mono text-[26px]/8 font-medium tracking-[-0.02em]">
            {formatMoney(used)}
          </dd>
          {limit > 0 ? (
            <>
              <dd className="pt-1">
                <Meter
                  used={used}
                  reserved={reserved}
                  limit={limit}
                  label={`${formatMoney(used + reserved)} of the ${formatMoney(limit)} spend cap used or reserved`}
                />
              </dd>
              <dd className={cx("text-xs/4", capReached ? "text-error" : "text-ink-3")}>
                {capReached
                  ? "Spend cap reached. Raise it under API billing to create more jobs."
                  : `of ${formatMoney(limit)} cap${reserved > 0 ? ` · ${formatMoney(reserved)} reserved` : ""}`}
              </dd>
            </>
          ) : (
            <dd className="text-xs/4 text-ink-3">No spend cap yet</dd>
          )}
        </Stat>
        <Stat label="Conversions this month">
          <dd className="font-mono text-[26px]/8 font-medium tracking-[-0.02em]">
            {formatNumber(api.thisMonth)}
          </dd>
          <dd className="text-xs/4 text-ink-3">Since {formatShortDate(api.since)}</dd>
        </Stat>
        <Stat label="Last 30 days">
          <dd className="font-mono text-[26px]/8 font-medium tracking-[-0.02em]">
            {formatNumber(api.last30Days)}
          </dd>
          <dd className={cx("text-xs/4", api.failed > 0 ? "text-error" : "text-ink-3")}>
            {formatNumber(api.failed)} failed, not charged
          </dd>
        </Stat>
      </dl>

      {error && (
        <p role="alert" className="text-sm text-error">
          {error}
        </p>
      )}

      <div className="grid items-start gap-6 lg:grid-cols-[minmax(0,1fr)_340px]">
        <div className="flex min-w-0 flex-col gap-6">
          <section aria-labelledby="keys-title">
            <Card className="flex flex-col overflow-clip">
              <div className="flex flex-wrap items-center justify-between gap-3 border-b border-line px-5 py-4">
                <div className="flex flex-col gap-1">
                  <SectionTitle id="keys-title">Keys</SectionTitle>
                  <p className="text-xs/4 text-ink-2">
                    Send a key as <code className="font-mono text-ink">Authorization: Bearer</code>.
                    Keep keys on your server.
                  </p>
                </div>
                {!creating && (
                  <PrimaryButton
                    disabled={busy || !canCreate}
                    title={onSale && !enrolled ? "Add a card under API billing first" : undefined}
                    onClick={() => {
                      setCreating(true);
                      setShownKey(null);
                    }}
                  >
                    {onSale ? "Create key" : "Coming soon"}
                  </PrimaryButton>
                )}
              </div>

              {creating && (
                <form
                  className="flex flex-col gap-3 border-b border-line bg-sunken px-5 py-4 sm:flex-row sm:items-end"
                  onSubmit={(e) => {
                    e.preventDefault();
                    if (name.trim()) void create();
                  }}
                >
                  <div className="flex min-w-0 flex-1 flex-col gap-1.5">
                    <label htmlFor="key-name" className="text-[13px]/4 font-medium">
                      Key name
                    </label>
                    <input
                      id="key-name"
                      autoFocus
                      value={name}
                      maxLength={80}
                      onChange={(e) => setName(e.target.value)}
                      placeholder="Production server"
                      className={cx(
                        "h-9 rounded-lg bg-page px-3 text-sm shadow-input dark:bg-raised",
                        focusRing,
                      )}
                    />
                  </div>
                  <div className="flex items-center gap-3">
                    <PrimaryButton type="submit" disabled={busy || !name.trim()} className="h-9">
                      {busy ? "Creating…" : "Create API key"}
                    </PrimaryButton>
                    <TextButton tone="muted" onClick={() => setCreating(false)}>
                      Cancel
                    </TextButton>
                  </div>
                </form>
              )}

              {shownKey && (
                <div
                  role="status"
                  className="flex flex-col gap-3 border-b border-green-line bg-green-tint px-5 py-4"
                >
                  <div className="flex flex-col gap-1">
                    <h3 className="text-[13px]/4 font-semibold">Your new API key</h3>
                    <p className="text-xs/4 text-ink-2">
                      Copy it now. convt stores only a hash, so it will not be shown again.
                    </p>
                  </div>
                  <div className="flex flex-col gap-2 sm:flex-row sm:items-center">
                    <input
                      aria-label="New API key"
                      readOnly
                      value={shownKey}
                      onFocus={(e) => e.target.select()}
                      className={cx(
                        "h-9 w-full min-w-0 rounded-lg bg-page px-3 font-mono text-xs shadow-input dark:bg-raised",
                        focusRing,
                      )}
                    />
                    <CopyButton value={shownKey} aria-label="Copy API key" />
                  </div>
                  <TextButton className="self-start" onClick={() => setShownKey(null)}>
                    I saved the key
                  </TextButton>
                </div>
              )}

              {api.keys.length === 0 ? (
                <div className="flex flex-col items-start gap-1 px-5 py-8">
                  <p className="text-[13px]/5 font-medium">No keys yet.</p>
                  <p className="text-[13px]/5 text-ink-2">
                    {!onSale
                      ? "Keys open when the API goes on sale."
                      : enrolled
                        ? "Create one to start converting."
                        : "Add a card under API billing, then create a key here."}
                  </p>
                </div>
              ) : (
                <ul aria-label="API keys" className="flex flex-col">
                  <li
                    aria-hidden="true"
                    className="hidden grid-cols-[minmax(0,1fr)_minmax(0,1.3fr)_110px_110px_88px] gap-4 bg-sunken px-5 py-2 text-xs/4 text-ink-2 md:grid"
                  >
                    <span>Name</span>
                    <span>Key</span>
                    <span>Created</span>
                    <span>Last used</span>
                    <span />
                  </li>
                  {api.keys.map((key) => (
                    <li
                      key={key.id}
                      className="grid grid-cols-[minmax(0,1fr)_auto] items-center gap-x-4 gap-y-1.5 border-t border-line px-5 py-3.5 text-[13px]/4 md:grid-cols-[minmax(0,1fr)_minmax(0,1.3fr)_110px_110px_88px]"
                    >
                      <span className="truncate font-medium">{key.name}</span>
                      <span className="col-start-1 row-start-2 min-w-0 md:col-start-auto md:row-start-auto">
                        <span className="inline-block max-w-full truncate rounded-md bg-chip px-2 py-1 font-mono text-[11.5px]/4 text-ink-2 ring-1 ring-chip-line">
                          {key.maskedKey}
                        </span>
                      </span>
                      <span className="col-start-1 text-xs/4 text-ink-2 md:col-start-auto md:text-[13px]/4">
                        <span className="md:hidden">Created </span>
                        {formatDate(key.created)}
                      </span>
                      <span className="col-start-1 text-xs/4 text-ink-2 md:col-start-auto md:text-[13px]/4">
                        <span className="md:hidden">Last used </span>
                        {key.lastUsed}
                      </span>
                      <span className="col-start-2 row-span-4 row-start-1 flex justify-end md:col-start-auto md:row-span-1 md:row-start-auto">
                        {confirming === key.id ? (
                          <span className="flex flex-col items-end gap-1.5">
                            <TextButton
                              tone="danger"
                              disabled={busy}
                              onClick={() => revoke(key.id)}
                              aria-label={`Revoke now: ${key.name}`}
                              className="font-medium"
                            >
                              Revoke now
                            </TextButton>
                            <TextButton
                              tone="muted"
                              onClick={() => setConfirming(null)}
                              aria-label={`Keep ${key.name}`}
                            >
                              Keep
                            </TextButton>
                          </span>
                        ) : (
                          <TextButton
                            tone="danger"
                            disabled={busy}
                            onClick={() => setConfirming(key.id)}
                            aria-label={`Revoke ${key.name}`}
                          >
                            Revoke
                          </TextButton>
                        )}
                      </span>
                    </li>
                  ))}
                </ul>
              )}
              {confirming && (
                <p
                  role="status"
                  className="border-t border-line bg-sunken px-5 py-3 text-xs/4 text-ink-2"
                >
                  Requests with a revoked key fail at once with 403. This can't be undone.
                </p>
              )}
            </Card>
          </section>

          <Card className="p-5 sm:p-6">
            <UsageChart days={api.perDay} />
          </Card>

          <section aria-labelledby="quick-start-title" className="flex flex-col gap-3">
            <div className="flex flex-wrap items-baseline justify-between gap-2">
              <SectionTitle id="quick-start-title">Quick start</SectionTitle>
              <a
                href={links.docsQuickStart}
                className={cx(
                  "rounded-sm text-[13px]/4 font-medium text-green hover:underline",
                  focusRing,
                )}
              >
                Walk through it in the docs
              </a>
            </div>
            <CodePanel samples={quickStart(apiBaseUrl)} maxHeight="max-h-[420px]" />
          </section>
        </div>

        <aside className="flex min-w-0 flex-col gap-6">
          <ApiEnrollmentCard
            enrollment={api.enrollment}
            blocked={api.enrollBlocked}
            available={onSale}
          />

          <Card className="flex flex-col gap-3 p-5">
            <h2 className="text-[13px]/4 font-semibold">Base URL</h2>
            <div className="flex items-center justify-between gap-2 rounded-lg bg-code py-1 pr-1 pl-3">
              <code className="min-w-0 truncate font-mono text-[12px]/5 text-[#e6e8e7]">
                {apiBaseUrl}
              </code>
              <CopyCode value={apiBaseUrl} />
            </div>
            <p className="text-xs/[18px] text-ink-3">
              An interim host. api.convt.app replaces it once its DNS is live.
            </p>
          </Card>

          <Card className="flex flex-col overflow-clip">
            <a
              href={links.apiReference}
              className={cx(
                "flex items-center justify-between gap-3 border-b border-line px-5 py-4 hover:bg-hover",
                focusRing,
                "focus-visible:ring-inset focus-visible:ring-offset-0",
              )}
            >
              <span className="flex flex-col gap-0.5">
                <span className="text-[13px]/4 font-semibold">API reference</span>
                <span className="text-xs/4 text-ink-2">Endpoints, objects and samples</span>
              </span>
              <span className="text-ink-3">
                <ExternalIcon />
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
        </aside>
      </div>
    </div>
  );
}

function Stat({ label, children }: { label: string; children: ReactNode }) {
  return (
    <Card className="flex flex-col gap-1.5 px-5 py-4">
      <dt className="text-[13px]/4 text-ink-2">{label}</dt>
      {children}
    </Card>
  );
}
