import type { ReactNode } from "react";

import { cx, focusRing } from "#/components/app/ui";
import { legal } from "#/lib/site";

import { siteColumn } from "./layout";

export type LegalSection = { id: string; title: string; body: ReactNode };

/**
 * Template for the privacy policy and terms: the title, the effective date, a contents
 * list (sticky beside the text on wide screens) and numbered sections.
 */
export function LegalPage({
  title,
  summary,
  sections,
}: {
  title: string;
  summary: ReactNode;
  sections: LegalSection[];
}) {
  return (
    <div className={cx(siteColumn, "flex flex-col gap-10 pt-12 pb-20 md:pt-16")}>
      <div className="flex max-w-[680px] flex-col gap-3">
        <p className="font-mono text-xs/4 text-ink-2 uppercase">Legal</p>
        <h1 className="text-[34px]/10 font-semibold tracking-[-0.03em] md:text-[44px]/12">
          {title}
        </h1>
        <p className="text-sm/5 text-ink-2">Effective {legal.effectiveDate}</p>
        <div className="pt-2 text-[17px]/[26px] text-ink-2">{summary}</div>
      </div>
      <div className="grid gap-10 lg:grid-cols-[220px_1fr] lg:gap-16">
        <nav aria-label="Contents" className="lg:sticky lg:top-6 lg:self-start">
          <h2 className="pb-3 font-mono text-xs/4 text-ink-2 uppercase">Contents</h2>
          <ol className="flex flex-col gap-2 text-sm/5">
            {sections.map((section, i) => (
              <li key={section.id}>
                <a
                  href={`#${section.id}`}
                  className={cx("flex gap-2 rounded-sm text-ink-2 hover:text-ink", focusRing)}
                >
                  <span className="w-5 shrink-0 font-mono text-xs/5 text-ink-2">{i + 1}.</span>
                  {section.title}
                </a>
              </li>
            ))}
          </ol>
        </nav>
        <div className="flex max-w-[680px] min-w-0 flex-col gap-10">
          {sections.map((section, i) => (
            <section
              key={section.id}
              id={section.id}
              aria-labelledby={`${section.id}-title`}
              className="flex scroll-mt-6 flex-col gap-3"
            >
              <h2
                id={`${section.id}-title`}
                className="text-xl/7 font-semibold tracking-[-0.015em]"
              >
                <span className="pr-2 font-mono text-sm text-ink-2">{i + 1}.</span>
                {section.title}
              </h2>
              <div className="flex flex-col gap-3 text-[15px]/6 text-ink-2">{section.body}</div>
            </section>
          ))}
        </div>
      </div>
    </div>
  );
}

/** Plain bullet list for legal text. */
export function List({ items }: { items: ReactNode[] }) {
  return (
    <ul className="flex flex-col gap-2 pl-1">
      {items.map((item, i) => (
        <li key={i} className="flex gap-3">
          <span
            aria-hidden="true"
            className="mt-[9px] size-1.5 shrink-0 rounded-[3px] bg-line-strong"
          />
          <span>{item}</span>
        </li>
      ))}
    </ul>
  );
}
