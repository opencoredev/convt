import { PageShell } from "#/components/landing/page-shell";
import { Container, cx, focusRing } from "#/components/landing/ui";
import { formatGroups } from "#/lib/conversion-copy";
import { categories, conversions, formats, slugOf, titleOf } from "#/lib/conversions";

import { SectionTitle } from "./ui";

/** /convert: every conversion page, then every format convt reads. */
export function HubPage() {
  const total = formatGroups.reduce((n, g) => n + g.formats.length, 0);
  return (
    <PageShell>
      <Container className="flex flex-col gap-16 pt-10 md:gap-20 md:pt-16">
        <header className="flex max-w-[760px] flex-col gap-5">
          <h1 className="text-[38px]/[42px] font-medium tracking-[-0.04em] text-balance text-ink md:text-[56px]/[60px]">
            Convert files on your computer
          </h1>
          <p className="text-[17px]/[27px] text-ink-2 md:text-[18px]/[28px]">
            Every conversion here runs on your own machine with convt: right-click a file and pick a
            format, or use the <code className="font-mono text-[0.9em] text-land-mono">convt</code>{" "}
            command. Nothing gets uploaded, and it works offline.
          </p>
        </header>

        {categories.map((category) => (
          <section
            key={category.id}
            aria-labelledby={`cat-${category.id}`}
            className="flex flex-col gap-6"
          >
            <SectionTitle id={`cat-${category.id}`}>{category.title}</SectionTitle>
            <ul className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
              {conversions
                .filter((c) => c.category === category.id)
                .map((c) => (
                  <li key={slugOf(c)} className="flex">
                    <a
                      href={`/convert/${slugOf(c)}`}
                      className={cx(
                        "group flex w-full flex-col gap-2 rounded-[14px] bg-raised p-5 shadow-land-card transition-colors hover:bg-hover",
                        focusRing,
                      )}
                    >
                      <span className="flex items-center gap-2 font-mono text-[12px]/[16px] text-land-mono">
                        .{formats[c.from].ext}
                        <span aria-hidden="true" className="text-land-accent">
                          →
                        </span>
                        .{formats[c.to].ext}
                      </span>
                      <span className="text-[17px]/[22px] font-medium text-ink">{titleOf(c)}</span>
                      <span className="text-[14px]/[21px] text-ink-2">{c.why}</span>
                    </a>
                  </li>
                ))}
            </ul>
          </section>
        ))}

        <section aria-labelledby="all-formats" className="flex flex-col gap-6">
          <div className="flex flex-col gap-2">
            <SectionTitle id="all-formats">All {total} formats</SectionTitle>
            <p className="max-w-[640px] text-[15px]/[24px] text-ink-2">
              convt reads and writes these, and chains up to three steps when no single tool does
              the job directly.
            </p>
          </div>
          <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
            {formatGroups.map((group) => (
              <div
                key={group.title}
                className="flex flex-col gap-3.5 rounded-[14px] bg-raised p-5 shadow-land-card"
              >
                <div className="flex items-baseline justify-between">
                  <h3 className="text-[17px]/[22px] font-medium text-ink">{group.title}</h3>
                  <span className="font-mono text-[12px]/[16px] text-land-muted">
                    {group.formats.length}
                  </span>
                </div>
                <ul className="flex flex-wrap gap-1.5">
                  {group.formats.map((f) => (
                    <li
                      key={f.id}
                      className="rounded-md bg-sunken px-[7px] py-[3px] font-mono text-[11.5px]/[14px] text-ink-2"
                    >
                      {f.name}
                    </li>
                  ))}
                </ul>
              </div>
            ))}
          </div>
        </section>
      </Container>
    </PageShell>
  );
}
