import { ComingSoon, Container, cx, focusRing } from "#/components/landing/ui";
import { PageShell } from "#/components/landing/page-shell";
import { faqFor, stepsFor } from "#/lib/conversion-copy";
import {
  type Conversion,
  commandFor,
  engines,
  formats,
  relatedTo,
  slugOf,
  titleOf,
} from "#/lib/conversions";

import { FileChip, Prose, SectionTitle, TextLink } from "./ui";

/** One /convert/<from>-to-<to> page. */
export function ConversionPage({ conversion: c }: { conversion: Conversion }) {
  const from = formats[c.from];
  const to = formats[c.to];
  const sample = c.category === "documents" ? "report" : c.category === "images" ? "photo" : "clip";
  return (
    <PageShell>
      <Container className="flex flex-col gap-16 pt-10 md:gap-20 md:pt-16">
        <header className="flex flex-col items-start gap-6">
          <nav aria-label="Breadcrumb" className="font-mono text-[12px]/[16px] text-land-muted">
            <ol className="flex flex-wrap items-center gap-2">
              <li>
                <TextLink href="/convert" className="text-land-muted no-underline hover:text-ink">
                  Conversions
                </TextLink>
              </li>
              <li aria-hidden="true">/</li>
              <li aria-current="page" className="text-ink-2">
                {titleOf(c)}
              </li>
            </ol>
          </nav>
          <h1 className="max-w-[820px] text-[38px]/[42px] font-medium tracking-[-0.04em] text-balance text-ink md:text-[56px]/[60px]">
            Convert {from.label} to {to.label} on your computer
          </h1>
          <p className="max-w-[640px] text-[17px]/[27px] text-ink-2 md:text-[18px]/[28px]">
            {c.why} convt does it with a right-click, on your own machine. Nothing gets uploaded.
          </p>
          <div className="flex flex-wrap items-center gap-3">
            <ComingSoon className="rounded-[10px] px-[18px] py-[11px] text-[15px]/[18px]">
              Coming soon to macOS, Windows and Linux
            </ComingSoon>
          </div>
          <div aria-hidden="true" className="flex items-center gap-3 pt-2">
            <FileChip name={sample} ext={from.ext} />
            <span className="text-land-accent">→</span>
            <FileChip name={sample} ext={to.ext} done />
          </div>
        </header>

        <div className="grid gap-16 lg:grid-cols-[minmax(0,1fr)_340px] lg:gap-20">
          <div className="flex min-w-0 flex-col gap-16">
            <section aria-labelledby="how" className="flex flex-col gap-6">
              <SectionTitle id="how">How to convert {titleOf(c)} with convt</SectionTitle>
              <ol className="flex flex-col gap-3">
                {stepsFor(c).map((step, i) => (
                  <li key={step} className="flex gap-3.5 text-[16px]/[25px] text-ink-2">
                    <span className="flex size-6 shrink-0 items-center justify-center rounded-full bg-sunken font-mono text-[12px]/[16px] text-ink">
                      {i + 1}
                    </span>
                    {step}
                  </li>
                ))}
              </ol>
              <div className="flex flex-col gap-3">
                <p className="text-[15px]/[23px] text-ink-2">Or from a terminal:</p>
                <pre className="overflow-x-auto rounded-[10px] bg-land-code px-4 py-3.5 font-mono text-[13px]/[22px] text-land-mono shadow-[inset_0_0_0_1px_#232726]">
                  <code>
                    <span className="text-land-muted">$ </span>
                    {commandFor(c, sample)}
                  </code>
                </pre>
                {c.options && (
                  <dl className="flex flex-col gap-2">
                    {c.options.map((o) => (
                      <div key={o.flag} className="flex flex-wrap items-baseline gap-x-3 gap-y-1">
                        <dt>
                          <Prose text={`\`${o.flag}\``} />
                        </dt>
                        <dd className="text-[14px]/[21px] text-ink-2">{o.does}</dd>
                      </div>
                    ))}
                  </dl>
                )}
              </div>
            </section>

            <section aria-labelledby="notes" className="flex flex-col gap-5">
              <SectionTitle id="notes">Good to know</SectionTitle>
              <ul className="flex flex-col gap-3">
                {c.notes.map((note) => (
                  <li key={note} className="flex gap-3 text-[16px]/[25px] text-ink-2">
                    <span aria-hidden="true" className="text-land-accent">
                      ✓
                    </span>
                    <span>
                      <Prose text={note} />
                    </span>
                  </li>
                ))}
              </ul>
            </section>

            <section aria-labelledby="the-formats" className="flex flex-col gap-5">
              <SectionTitle id="the-formats">The formats</SectionTitle>
              <div className="grid gap-4 sm:grid-cols-2">
                {[from, to].map((f) => (
                  <div
                    key={f.label}
                    className="flex flex-col gap-2 rounded-[14px] bg-raised p-5 shadow-land-card"
                  >
                    <h3 className="font-mono text-[13px]/[16px] text-land-mono">.{f.ext}</h3>
                    <p className="text-[17px]/[22px] font-medium text-ink">{f.label}</p>
                    <p className="text-[14px]/[21px] text-ink-2">{f.about}</p>
                  </div>
                ))}
              </div>
            </section>

            <section aria-labelledby="faq" className="flex flex-col gap-5">
              <SectionTitle id="faq">Questions</SectionTitle>
              <dl className="flex flex-col divide-y divide-line">
                {faqFor(c).map(({ q, a }) => (
                  <div key={q} className="flex flex-col gap-2 py-5 first:pt-0">
                    <dt className="text-[16px]/[22px] font-medium text-ink">{q}</dt>
                    <dd className="text-[15px]/[24px] text-ink-2">
                      <Prose text={a} />
                    </dd>
                  </div>
                ))}
              </dl>
            </section>
          </div>

          <aside className="flex flex-col gap-6 lg:sticky lg:top-8 lg:self-start">
            <div className="flex flex-col gap-3 rounded-[14px] bg-raised p-5 shadow-land-card">
              <p className="font-mono text-[11px]/[14px] tracking-[0.06em] text-land-label uppercase">
                Engine
              </p>
              <p className="text-[17px]/[22px] font-medium text-ink">{engines[c.engine].name}</p>
              <p className="text-[14px]/[21px] text-ink-2">
                <Prose text={engines[c.engine].about} />
              </p>
            </div>
            <nav aria-labelledby="related" className="flex flex-col gap-3">
              <h2
                id="related"
                className="font-mono text-[11px]/[14px] tracking-[0.06em] text-land-label uppercase"
              >
                Related conversions
              </h2>
              <ul className="flex flex-wrap gap-2">
                {relatedTo(c).map((r) => (
                  <li key={slugOf(r)}>
                    <a
                      href={`/convert/${slugOf(r)}`}
                      className={cx(
                        "inline-flex rounded-lg bg-sunken px-3 py-1.5 text-[13px]/[18px] text-ink-2 shadow-land-secondary transition-colors hover:text-ink",
                        focusRing,
                      )}
                    >
                      {titleOf(r)}
                    </a>
                  </li>
                ))}
              </ul>
              <TextLink href="/convert" className="self-start text-[14px]/[20px]">
                All conversions
              </TextLink>
            </nav>
          </aside>
        </div>
      </Container>
    </PageShell>
  );
}
