import type { ReactNode } from "react";

import { cx, focusRing } from "#/components/app/ui";
import { legal } from "#/lib/site";

import { siteColumn } from "./layout";

export type LegalSection = { id: string; title: string; body: ReactNode };

/**
 * Template for the privacy policy and terms: a draft notice, the title, a contents
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
      <DraftNotice />
      <div className="flex max-w-[680px] flex-col gap-3">
        <p className="font-mono text-xs/4 text-ink-2 uppercase">Legal</p>
        <h1 className="text-[34px]/10 font-semibold tracking-[-0.03em] md:text-[44px]/12">
          {title}
        </h1>
        <p className="text-sm/5 text-ink-2">
          Effective <Placeholder>{legal.effectiveDate}</Placeholder>
        </p>
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

function DraftNotice() {
  return (
    <div
      role="note"
      className="flex flex-col gap-1 rounded-xl bg-[#fff8e6] px-4 py-3 text-sm/5 text-[#5c4300] shadow-[inset_0_0_0_1px_#f0dca6] dark:bg-[#2a2210] dark:text-[#f0d48a] dark:shadow-[inset_0_0_0_1px_#4a3c17]"
    >
      <p className="font-semibold">Draft, pending legal review</p>
      <p>
        This text has not been reviewed by a lawyer yet and may change before convt launches.
        Highlighted parts are placeholders.
      </p>
    </div>
  );
}

/** A value Leo still has to fill in, highlighted until then. */
export function Placeholder({ children }: { children: ReactNode }) {
  return (
    <mark className="rounded-[4px] bg-[#fff1c2] px-1 text-[#5c4300] dark:bg-[#3a2f12] dark:text-[#f0d48a]">
      {children}
    </mark>
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
