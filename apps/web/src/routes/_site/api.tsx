// /api: the cloud API's landing page.
//
// Direction:
// - Reference: Stripe's API pages (the code is the hero, the price is a plain sentence) and
//   Resend's homepage (one terminal block carries the pitch). Built from convt's own system:
//   site column, ink tokens, the brand green and the dark code panel the home page uses.
// - Layout: the price is the headline number, beside the one request that earns it. Below,
//   an annotated HTTP response shows what a developer actually gets back.
// - Signature: a cost meter. Type a monthly volume, see the bill; it is just n cents.
// - Avoiding: three-up icon cards, testimonials, FAQ accordions, invented benchmark numbers.
//   The CloudConvert comparison uses only what cloudconvert.com/pricing states (credits per
//   conversion type and per extra minute), checked 23 November 2026, not dollar figures.

import { createFileRoute } from "@tanstack/react-router";
import { useId, useState, type ReactNode } from "react";

import { cx, focusRing } from "#/components/app/ui";
import { Code, FactList } from "#/components/site/guide";
import { TextLink, siteColumn } from "#/components/site/layout";
import capabilities from "#/generated/cloud-formats.json";
import { getPublicConfig } from "#/server/public-config";
import { SITE_NAME, SITE_ORIGIN, routes, seo } from "#/lib/site";

// The page links to the dashboard's API keys, which sends signed-out visitors to sign in.

const title = "File conversion API: $0.01 per conversion · convt";
const description =
  "Convert images, video, audio, documents and PDFs with one HTTP request. $0.01 per successful conversion, no monthly fee. Idempotent retries, signed webhooks, files up to 2 GB.";

const API = "https://api.convt.app";
const DOCS = "/docs";
const KEYS = "/dashboard/api";

const categoryNames: Record<string, string> = {
  image: "Images",
  vector: "Vector",
  video: "Video",
  audio: "Audio",
  document: "Documents",
  spreadsheet: "Spreadsheets",
  presentation: "Slides",
  pdf: "PDF",
};

const formatsByCategory = Object.entries(
  capabilities.formats.reduce<Record<string, string[]>>((groups, format) => {
    (groups[format.category] ??= []).push(format.name);
    return groups;
  }, {}),
).sort(([a], [b]) => Object.keys(categoryNames).indexOf(a) - Object.keys(categoryNames).indexOf(b));
const inputCount = capabilities.formats.length;
const pairCount = capabilities.formats.reduce((sum, f) => sum + f.targets.length, 0);

type Sample = { id: string; label: string; code: string };
const samples: Sample[] = [
  {
    id: "curl",
    label: "cURL",
    code: `curl -fsS ${API}/v1/convert \\
  -H "Authorization: Bearer $CONVT_API_KEY" \\
  -F file=@photo.heic \\
  -F to=jpg -o photo.jpg`,
  },
  {
    id: "sdk",
    label: "JavaScript",
    code: `import { Convt } from "@convt/sdk";

const convt = new Convt();
const out = await convt.convert("report.docx", { to: "pdf" });
await out.save("report.pdf");`,
  },
  {
    id: "python",
    label: "Python",
    code: `import os, requests

r = requests.post("${API}/v1/convert",
    headers={"Authorization": f"Bearer {os.environ['CONVT_API_KEY']}"},
    files={"file": open("clip.mov", "rb")}, data={"to": "mp4"})
if not r.ok or "json" in r.headers["content-type"]:
    raise SystemExit(r.text)
open("clip.mp4", "wb").write(r.content)`,
  },
];

const response = [
  ["HTTP/2 200", ""],
  ["content-type", "image/jpeg"],
  ["content-disposition", 'attachment; filename="photo.jpg"'],
  ["convt-job-id", "job_01kb6q3z9m4x7d2c8v5n0r1t6e"],
  ["x-request-id", "req_01kb6q3z8f2h5k9w3c7p1s4y0a"],
  ["x-ratelimit-remaining", "119"],
] as const;

const production: [ReactNode, ReactNode][] = [
  [
    "One request",
    <>
      <Code>POST /v1/convert</Code> takes a file up to 25 MB and answers with the converted file. No
      upload step, no polling.
    </>,
  ],
  [
    "Files up to 2 GB",
    "Larger files upload straight to storage through a signed URL, then run on the same queue.",
  ],
  [
    "Files already online",
    <>
      Send a <Code>source_url</Code>, such as a presigned S3 or R2 link, and convt downloads it.
      Private and metadata addresses are refused.
    </>,
  ],
  [
    "Safe retries",
    <>
      An <Code>Idempotency-Key</Code> makes a retried request return the original job instead of
      billing a second one.
    </>,
  ],
  [
    "Signed webhooks",
    "A job with a webhook_url POSTs an HMAC-signed event when it finishes, retried for nearly three hours.",
  ],
  [
    "Debuggable",
    "Every response has an X-Request-Id and every error a stable code. Authenticated calls carry rate-limit headers.",
  ],
  [
    "Isolated",
    "Each conversion runs in a fresh sandboxed process with no network. A job's files are deleted 24 hours after it is created.",
  ],
];

// Credits from cloudconvert.com/pricing (23 November 2026): 1 for most conversions, 2 for Office
// to PDF, plus 1 for every minute of conversion time past the first. Only pairs the convt API
// itself supports (cloud-formats.json) are listed.
const comparison: [string, string, string][] = [
  ["Photo, HEIC to JPG", "1 credit", "$0.01"],
  ["Word to PDF", "2 credits", "$0.01"],
  ["Excel to PDF", "2 credits", "$0.01"],
  ["Video, 6 minutes to convert", "6 credits", "$0.01"],
  ["Failed conversion", "Free", "Free"],
];

export const Route = createFileRoute("/_site/api")({
  head: () => {
    const tags = seo({ title, description, path: "/api" });
    const structuredData = {
      "@context": "https://schema.org",
      "@type": "WebAPI",
      name: `${SITE_NAME} API`,
      url: `${SITE_ORIGIN}/api`,
      description,
      documentation: `${SITE_ORIGIN}/docs`,
      provider: { "@id": `${SITE_ORIGIN}/#organization` },
      offers: {
        "@type": "Offer",
        price: "0.01",
        priceCurrency: "USD",
        description: "Per successful conversion",
      },
    };
    return {
      ...tags,
      scripts: [{ type: "application/ld+json", children: JSON.stringify(structuredData) }],
    };
  },
  loader: async () => ({ sales: (await getPublicConfig()).sales }),
  component: ApiPage,
});

function ApiPage() {
  const { sales } = Route.useLoaderData();
  const onSale = sales === "all";
  return (
    <article className={cx(siteColumn, "pt-10 pb-20 md:pt-16")}>
      <section className="grid grid-cols-[minmax(0,1fr)] items-start gap-10 lg:grid-cols-[minmax(0,1fr)_minmax(0,560px)] lg:gap-14">
        <div className="flex min-w-0 flex-col gap-5">
          <p className="font-mono text-xs/4 text-ink-2 uppercase">convt API</p>
          <h1 className="text-[36px]/[42px] font-semibold tracking-[-0.035em] text-balance md:text-[52px]/[56px]">
            Convert any file with one request.
          </h1>
          <p className="flex flex-col gap-1 sm:flex-row sm:items-baseline sm:gap-2.5">
            <span className="text-[56px]/[60px] font-semibold tracking-[-0.045em] text-[#157f4a] tabular-nums dark:text-green">
              $0.01
            </span>
            <span className="text-[17px]/6 text-ink-2">per conversion. Failures are free.</span>
          </p>
          <p className="max-w-[520px] text-[17px]/[27px] text-pretty text-ink-2">
            Images, video, audio, Word, Excel, PowerPoint and PDF, on the same engines as the convt
            desktop app. No monthly fee and no credits to buy: you set a spend cap and pay for what
            succeeded, once a month.
          </p>
          <div className="flex flex-wrap items-center gap-3 pt-1">
            {onSale ? (
              <a
                href={KEYS}
                className={cx(
                  "btn-primary inline-flex items-center justify-center rounded-lg px-4 py-2.5 text-sm/4.5 font-medium",
                  focusRing,
                )}
              >
                Get an API key
              </a>
            ) : (
              <span className="inline-flex items-center rounded-lg bg-sunken px-4 py-2.5 text-sm/4.5 text-ink-2">
                API keys open soon
              </span>
            )}
            <a
              href={`${DOCS}/quick-start`}
              className={cx(
                "inline-flex items-center justify-center rounded-lg bg-raised px-4 py-2.5 text-sm/4.5 font-medium shadow-[inset_0_0_0_1px_var(--line)] hover:bg-hover",
                focusRing,
              )}
            >
              Quick start
            </a>
          </div>
        </div>
        <div className="lg:pt-10">
          <CodePanel />
        </div>
      </section>

      <section
        aria-labelledby="price"
        className="mt-20 grid grid-cols-[minmax(0,1fr)] gap-10 md:mt-28 lg:grid-cols-2 lg:gap-14"
      >
        <div className="flex flex-col gap-4">
          <h2 id="price" className="text-[28px]/[34px] font-semibold tracking-[-0.03em]">
            The price is the number of files.
          </h2>
          <div className="flex flex-col gap-3 text-[16px]/[26px] text-pretty text-ink-2">
            <p>
              Each conversion that succeeds costs one cent, whatever the format, the size (up to 2
              GB) or how long it takes. Failed and cancelled conversions cost nothing. Creating a
              job reserves one cent against your spend cap, and new jobs stop once billed plus
              reserved spend reaches it.
            </p>
            <p>
              Billing runs through Polar once a month. Add a card and set a cap on the{" "}
              <TextLink href={KEYS}>dashboard</TextLink>, then create a key.
            </p>
          </div>
        </div>
        <CostMeter />
      </section>

      <section
        aria-labelledby="response"
        className="mt-20 grid grid-cols-[minmax(0,1fr)] gap-10 md:mt-28 lg:grid-cols-2 lg:gap-14"
      >
        <div className="flex flex-col gap-4 lg:order-2">
          <h2 id="response" className="text-[28px]/[34px] font-semibold tracking-[-0.03em]">
            The answer is the file.
          </h2>
          <div className="flex flex-col gap-3 text-[16px]/[26px] text-pretty text-ink-2">
            <p>
              The response to <Code>POST /v1/convert</Code> is the converted file, named after
              yours. The headers name the job, the request and your remaining rate limit, so a
              failure in production is one id away from an answer.
            </p>
            <p>
              Long video? Create a job with a <Code>webhook_url</Code> instead, and convt tells your
              server when it is done.
            </p>
          </div>
        </div>
        <figure className="dark flex min-w-0 flex-col overflow-hidden rounded-xl bg-land-code text-ink shadow-[inset_0_0_0_1px_#232726] lg:order-1">
          <figcaption className="flex h-[34px] items-center px-4 font-mono text-[11px]/4 text-land-muted shadow-[inset_0_-1px_0_#ffffff0f]">
            Response headers
          </figcaption>
          <pre
            tabIndex={0}
            aria-label="Example response headers"
            className="overflow-x-auto px-4 py-4 font-mono text-[12.5px]/[22px] [scrollbar-width:thin]"
          >
            <code>
              {response.map(([name, value]) => (
                <span key={name} className="block whitespace-pre">
                  <span className={value ? "text-[#9cc9ff]" : "text-[#7fd3a6]"}>{name}</span>
                  {value && <span className="text-land-muted">: </span>}
                  <span className="text-land-mono">{value}</span>
                </span>
              ))}
            </code>
          </pre>
        </figure>
      </section>

      <section aria-labelledby="formats" className="mt-20 flex flex-col gap-6 md:mt-28">
        <div className="flex max-w-[680px] flex-col gap-3">
          <h2 id="formats" className="text-[28px]/[34px] font-semibold tracking-[-0.03em]">
            {inputCount} input formats, {pairCount} conversions.
          </h2>
          <p className="text-[16px]/[26px] text-pretty text-ink-2">
            The same engines as the app: FFmpeg, LibreOffice, PDFium, libheif and resvg. When there
            is no direct path, convt chains up to three steps for you.{" "}
            <TextLink href={`${DOCS}/reference/formats`}>Every pair</TextLink>
          </p>
        </div>
        <dl className="grid gap-x-8 gap-y-5 sm:grid-cols-2 lg:grid-cols-4">
          {formatsByCategory.map(([category, names]) => (
            <div key={category} className="flex flex-col gap-1.5 border-t border-line pt-3">
              <dt className="text-[14px]/5 font-medium">{categoryNames[category] ?? category}</dt>
              <dd className="text-[13px]/5 text-pretty text-ink-2">{names.join(", ")}</dd>
            </div>
          ))}
        </dl>
      </section>

      <section aria-labelledby="compare" className="mt-20 flex flex-col gap-6 md:mt-28">
        <div className="flex max-w-[680px] flex-col gap-3">
          <h2 id="compare" className="text-[28px]/[34px] font-semibold tracking-[-0.03em]">
            Compared with credits
          </h2>
          <p className="text-[16px]/[26px] text-pretty text-ink-2">
            CloudConvert charges in credits: 1 for most conversions, 2 for Office to PDF, and 1 more
            for every minute of conversion time past the first. Its credit price depends on the
            package or subscription you buy. convt charges the same cent for all of them.
            CloudConvert does have a free tier of 10 credits a day; convt does not.
          </p>
        </div>
        <div className="overflow-x-auto rounded-2xl shadow-[inset_0_0_0_1px_var(--line)]">
          <table className="w-full text-left text-[13px]/5 sm:text-[14px]/5">
            <thead>
              <tr className="text-ink-2">
                <th scope="col" className="px-3 py-3 sm:px-4 font-medium">
                  Conversion
                </th>
                <th scope="col" className="px-3 py-3 sm:px-4 font-medium">
                  CloudConvert
                </th>
                <th scope="col" className="px-3 py-3 sm:px-4 font-medium">
                  convt
                </th>
              </tr>
            </thead>
            <tbody className="divide-y divide-divider border-t border-line">
              {comparison.map(([what, credits, cents]) => (
                <tr key={what}>
                  <th scope="row" className="px-3 py-3 font-normal sm:px-4">
                    {what}
                  </th>
                  <td className="px-3 py-3 whitespace-nowrap text-ink-2 tabular-nums sm:px-4">
                    {credits}
                  </td>
                  <td className="px-3 py-3 font-medium whitespace-nowrap tabular-nums sm:px-4">
                    {cents}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
        <p className="text-[13px]/5 text-ink-3">
          CloudConvert credits as listed on{" "}
          <TextLink href="https://cloudconvert.com/pricing">cloudconvert.com/pricing</TextLink> on
          23 November 2026.
        </p>
      </section>

      <section aria-labelledby="production" className="mt-20 flex flex-col gap-6 md:mt-28">
        <h2 id="production" className="text-[28px]/[34px] font-semibold tracking-[-0.03em]">
          Built for production
        </h2>
        <FactList facts={production} />
      </section>

      <section className="mt-20 flex flex-col items-start gap-4 border-t border-line pt-10 md:mt-28">
        <h2 className="text-[28px]/[34px] font-semibold tracking-[-0.03em] text-balance">
          Your first conversion is one line away.
        </h2>
        <div className="flex flex-wrap gap-3">
          {onSale && (
            <a
              href={KEYS}
              className={cx(
                "btn-primary inline-flex items-center justify-center rounded-lg px-4 py-2.5 text-sm/4.5 font-medium",
                focusRing,
              )}
            >
              Get an API key
            </a>
          )}
          <a
            href={`${DOCS}/api`}
            className={cx(
              "inline-flex items-center justify-center rounded-lg bg-raised px-4 py-2.5 text-sm/4.5 font-medium shadow-[inset_0_0_0_1px_var(--line)] hover:bg-hover",
              focusRing,
            )}
          >
            API reference
          </a>
        </div>
        <p className="text-[14px]/5 text-ink-2">
          Converting on your own machine instead? The{" "}
          <TextLink href={routes.download}>desktop app and CLI</TextLink> never upload a file.
        </p>
      </section>
    </article>
  );
}

function CodePanel() {
  const [active, setActive] = useState(samples[0].id);
  const [copyState, setCopyState] = useState<"idle" | "copied" | "failed">("idle");
  const base = useId();
  const sample = samples.find((s) => s.id === active) ?? samples[0];
  async function copy() {
    try {
      await navigator.clipboard.writeText(sample.code);
      setCopyState("copied");
    } catch {
      setCopyState("failed");
    }
    setTimeout(() => setCopyState("idle"), 2000);
  }
  return (
    // The code stays dark in both themes, like a terminal.
    <div className="dark flex min-w-0 flex-col overflow-hidden rounded-xl bg-land-code text-ink shadow-[inset_0_0_0_1px_#232726,0_8px_30px_#0a1e1426]">
      <div className="flex flex-wrap items-center justify-between gap-x-2 px-2 shadow-[inset_0_-1px_0_#ffffff0f]">
        <div role="tablist" aria-label="Language" className="flex">
          {samples.map((s) => (
            <button
              key={s.id}
              id={`${base}-${s.id}`}
              type="button"
              role="tab"
              aria-selected={s.id === active}
              aria-controls={`${base}-panel`}
              tabIndex={s.id === active ? 0 : -1}
              onClick={() => setActive(s.id)}
              onKeyDown={(event) => {
                const index = samples.findIndex((x) => x.id === s.id);
                const step = event.key === "ArrowRight" ? 1 : event.key === "ArrowLeft" ? -1 : 0;
                if (!step) return;
                event.preventDefault();
                const next = samples[(index + step + samples.length) % samples.length];
                setActive(next.id);
                document.getElementById(`${base}-${next.id}`)?.focus();
              }}
              className={cx(
                "relative h-10 rounded-sm px-3 font-mono text-[12px]/4 transition-colors",
                s.id === active ? "text-ink" : "text-land-muted hover:text-ink",
                focusRing,
              )}
            >
              {s.label}
              {s.id === active && (
                <span aria-hidden="true" className="absolute inset-x-3 -bottom-px h-px bg-green" />
              )}
            </button>
          ))}
        </div>
        <button
          type="button"
          onClick={copy}
          className={cx(
            "h-7 rounded-md px-2.5 font-mono text-[11px]/4 text-land-muted hover:bg-[#ffffff0f] hover:text-ink",
            focusRing,
          )}
        >
          {copyState === "copied" ? "Copied" : copyState === "failed" ? "Copy failed" : "Copy"}
        </button>
      </div>
      <span role="status" className="sr-only">
        {copyState === "copied"
          ? "Code copied"
          : copyState === "failed"
            ? "Copying failed. Select the code and copy it by hand."
            : ""}
      </span>
      <pre
        id={`${base}-panel`}
        role="tabpanel"
        tabIndex={0}
        aria-labelledby={`${base}-${active}`}
        className="min-h-[188px] overflow-x-auto px-4 py-4 font-mono text-[11.5px]/[21px] text-land-mono [scrollbar-width:thin] md:px-5 md:py-5 md:text-[13.5px]/[24px]"
      >
        <code>{sample.code}</code>
      </pre>
    </div>
  );
}

const presets = [1_000, 3_000, 10_000];

function CostMeter() {
  const [count, setCount] = useState(3_000);
  const inputId = useId();
  const dollars = (count / 100).toLocaleString("en-US", {
    style: "currency",
    currency: "USD",
    minimumFractionDigits: count % 100 === 0 ? 0 : 2,
    maximumFractionDigits: count % 100 === 0 ? 0 : 2,
  });
  return (
    <div className="flex flex-col gap-5 rounded-2xl bg-raised p-6 shadow-[0_0_0_1px_var(--line),0_8px_24px_#0a1e140f] dark:bg-panel dark:shadow-[0_0_0_1px_#232726]">
      <label htmlFor={inputId} className="text-[14px]/5 font-medium">
        Successful conversions a month
      </label>
      <input
        id={inputId}
        type="number"
        inputMode="numeric"
        min={0}
        max={1_000_000}
        step={1}
        value={count}
        onChange={(e) => {
          const next = Math.floor(Number(e.target.value));
          setCount(Number.isFinite(next) ? Math.min(Math.max(next, 0), 1_000_000) : 0);
        }}
        className={cx(
          "w-full rounded-lg border border-line bg-page px-3 py-2 font-mono text-[15px]/6 tabular-nums",
          focusRing,
        )}
      />
      <div className="flex flex-wrap gap-2">
        {presets.map((p) => (
          <button
            key={p}
            type="button"
            onClick={() => setCount(p)}
            aria-pressed={count === p}
            className={cx(
              "rounded-md px-2.5 py-1 font-mono text-[12px]/4 shadow-[inset_0_0_0_1px_var(--line)]",
              count === p ? "bg-sunken text-ink" : "text-ink-2 hover:bg-hover",
              focusRing,
            )}
          >
            {p.toLocaleString("en-US")}
          </button>
        ))}
      </div>
      <div className="flex items-baseline justify-between gap-3 border-t border-line pt-4">
        <span className="text-[14px]/5 text-ink-2">Your bill</span>
        <output
          htmlFor={inputId}
          aria-live="polite"
          className="text-[32px]/[38px] font-semibold tracking-[-0.03em] tabular-nums"
        >
          {dollars}
        </output>
      </div>
      <p className="text-[13px]/5 text-ink-2">
        No monthly fee and no minimum. Billed once a month. Accounts start at 100 conversions per
        rolling 24 hours, about 3,000 a month; we raise that on request.
      </p>
    </div>
  );
}
