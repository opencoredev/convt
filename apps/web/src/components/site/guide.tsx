import type { ReactNode } from "react";

import { CommandBlock } from "#/components/app/command-block";
import { cx, focusRing } from "#/components/app/ui";
import { guideDownloadUrl, type Guide, type GuideSlug } from "#/lib/guides";
import { routes } from "#/lib/site";

import { siteColumn } from "./layout";

// Building blocks for the pages under /guides: the article frame with its contents
// list, sections, numbered steps, and the label that says how each method was checked.

export type GuideSectionLink = { id: string; title: string };

const dateFormat = new Intl.DateTimeFormat("en-US", {
  year: "numeric",
  month: "long",
  day: "numeric",
  timeZone: "UTC",
});

/** Article frame: breadcrumb, title, lead, the contents rail on wide screens, then the body. */
export function GuideArticle({
  guide,
  lead,
  sections,
  children,
}: {
  guide: Guide;
  lead: ReactNode;
  sections: GuideSectionLink[];
  children: ReactNode;
}) {
  return (
    <article className={cx(siteColumn, "pt-10 pb-20 md:pt-14")}>
      <nav aria-label="Breadcrumb" className="font-mono text-xs/4 text-ink-2 uppercase">
        <a href={routes.guides} className={cx("rounded-sm hover:text-ink", focusRing)}>
          Guides
        </a>
      </nav>
      <header className="mt-3 flex max-w-[720px] flex-col gap-4">
        <h1 className="text-[34px]/10 font-semibold tracking-[-0.03em] text-balance md:text-[44px]/12">
          {guide.heading}
        </h1>
        <div className="text-[17px]/[27px] text-pretty text-ink-2">{lead}</div>
        <p className="text-[13px]/5 text-ink-2">
          Updated <time dateTime={guide.updated}>{dateFormat.format(new Date(guide.updated))}</time>
        </p>
      </header>
      <div className="mt-12 grid gap-12 lg:grid-cols-[minmax(0,720px)_220px] lg:justify-between">
        <div className="flex min-w-0 flex-col gap-16">{children}</div>
        <nav aria-label="On this page" className="max-lg:hidden">
          <div className="sticky top-8 flex flex-col gap-3">
            <p className="font-mono text-[11px]/4 text-ink-2 uppercase">On this page</p>
            <ol className="flex flex-col gap-2.5 border-l border-line">
              {sections.map((section) => (
                <li key={section.id}>
                  <a
                    href={`#${section.id}`}
                    className={cx(
                      "-ml-px block border-l border-transparent pl-3 text-[13px]/[18px] text-ink-2 hover:border-ink-2 hover:text-ink",
                      focusRing,
                    )}
                  >
                    {section.title}
                  </a>
                </li>
              ))}
            </ol>
          </div>
        </nav>
      </div>
    </article>
  );
}

/** A top-level section of a guide, linked from the contents rail by its id. */
export function GuideSection({
  id,
  title,
  source,
  children,
}: {
  id: string;
  title: string;
  /** How the steps were checked, shown under the heading. */
  source?: ReactNode;
  children: ReactNode;
}) {
  return (
    <section aria-labelledby={`${id}-title`} id={id} className="flex scroll-mt-8 flex-col gap-5">
      <div className="flex flex-col gap-3">
        <h2 id={`${id}-title`} className="text-[26px]/8 font-semibold tracking-[-0.02em]">
          {title}
        </h2>
        {source}
      </div>
      {children}
    </section>
  );
}

/** Says where a method's steps come from: run by us, or read in someone's documentation. */
export function Checked({ how, children }: { how: "tested" | "docs"; children: ReactNode }) {
  return (
    <p
      className={cx(
        "flex w-fit max-w-full items-start gap-2 rounded-lg px-3 py-2 text-[13px]/5",
        how === "tested"
          ? "bg-green-tint text-ink shadow-[inset_0_0_0_1px_var(--green-line)]"
          : "bg-sunken text-ink-2 shadow-[inset_0_0_0_1px_var(--line)]",
      )}
    >
      <span
        aria-hidden="true"
        className={cx(
          "mt-[7px] size-1.5 shrink-0 rounded-full",
          how === "tested" ? "bg-green" : "bg-ink-3",
        )}
      />
      <span>{children}</span>
    </p>
  );
}

/** Running text inside a guide. */
export function Prose({ children }: { children: ReactNode }) {
  return <div className="flex flex-col gap-4 text-[16px]/[26px] text-pretty">{children}</div>;
}

/** Numbered steps; each child is one step. */
export function Steps({ children }: { children: ReactNode[] }) {
  return (
    <ol className="flex flex-col gap-3">
      {children.map((step, i) => (
        <li key={i} className="flex gap-3.5 text-[16px]/[26px]">
          <span
            aria-hidden="true"
            className="mt-0.5 flex size-[22px] shrink-0 items-center justify-center rounded-full bg-chip font-mono text-[11.5px]/4 text-ink-2 shadow-[inset_0_0_0_1px_var(--chip-line)]"
          >
            {i + 1}
          </span>
          <span className="min-w-0">{step}</span>
        </li>
      ))}
    </ol>
  );
}

/** Terminal commands with a copy button. */
export function Commands({ label, commands }: { label: string; commands: [string, ...string[]] }) {
  return <CommandBlock label={label} commands={commands} />;
}

/** What a command printed, copied from a real run. */
export function Output({ label, children }: { label: string; children: string }) {
  return (
    <figure className="flex flex-col gap-2">
      <figcaption className="font-mono text-[11px]/4 text-ink-2 uppercase">{label}</figcaption>
      <pre className="overflow-x-auto rounded-xl bg-code px-4 py-3.5 font-mono text-[12.5px]/[21px] whitespace-pre text-code-ink shadow-[inset_0_0_0_1px_var(--code-ring)] [scrollbar-width:thin]">
        <code>{children}</code>
      </pre>
    </figure>
  );
}

/** Pairs of a term and what it means or does, one row each; rows stack on narrow screens. */
export function FactList({ facts }: { facts: [ReactNode, ReactNode][] }) {
  return (
    <dl className="divide-y divide-divider overflow-hidden rounded-2xl bg-raised shadow-[inset_0_0_0_1px_var(--line)] dark:bg-panel">
      {facts.map(([term, detail], i) => (
        <div
          key={i}
          className="flex flex-col gap-1 px-4 py-3 sm:grid sm:grid-cols-[200px_1fr] sm:gap-4"
        >
          <dt className="text-[14px]/5 font-medium">{term}</dt>
          <dd className="text-[14px]/5 text-ink-2">{detail}</dd>
        </div>
      ))}
    </dl>
  );
}

/** Inline code in running text. */
export function Code({ children }: { children: string }) {
  return (
    <code className="rounded bg-chip px-1 py-px font-mono text-[0.86em] shadow-[inset_0_0_0_1px_var(--chip-line)]">
      {children}
    </code>
  );
}

/** Closing call to action: the trial, with the guide's campaign tags on the link. */
export function GuideCta({ slug, children }: { slug: GuideSlug; children: ReactNode }) {
  return (
    <aside className="flex flex-col gap-5 rounded-2xl bg-panel p-6 shadow-[inset_0_0_0_1px_var(--line)] sm:flex-row sm:items-center sm:justify-between sm:gap-8">
      <div className="flex flex-col gap-1.5 text-[14px]/[22px] text-ink-2">{children}</div>
      <a
        href={guideDownloadUrl(slug)}
        className={cx(
          "btn-primary inline-flex shrink-0 items-center justify-center rounded-lg px-4 py-2.5 text-sm/4.5 font-medium",
          focusRing,
        )}
      >
        Start a 7-day trial
      </a>
    </aside>
  );
}
