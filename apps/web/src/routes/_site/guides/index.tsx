import { createFileRoute } from "@tanstack/react-router";

import { cx, focusRing } from "#/components/app/ui";
import { PageHeader, siteColumn } from "#/components/site/layout";
import { guidePath, guides } from "#/lib/guides";
import { routes, seo } from "#/lib/site";

export const Route = createFileRoute("/_site/guides/")({
  head: () =>
    seo({
      title: "Guides · convt",
      description:
        "Step-by-step guides to converting files on your own computer, free methods first.",
      path: routes.guides,
    }),
  component: GuidesPage,
});

function GuidesPage() {
  return (
    <div className={cx(siteColumn, "flex flex-col gap-12 pt-12 pb-20 md:pt-16")}>
      <PageHeader eyebrow="Guides" title="Convert files on your own computer.">
        <p>
          One task per guide. Each starts with what your computer can already do for free, then
          shows where convt helps. Every convt command was run before it went on the page.
        </p>
      </PageHeader>
      <ul className="flex max-w-[720px] flex-col divide-y divide-divider border-y border-line">
        {guides.map((guide) => (
          <li key={guide.slug}>
            <a
              href={guidePath(guide.slug)}
              className={cx("group flex flex-col gap-1.5 rounded-sm py-5", focusRing)}
            >
              <span className="text-[18px]/6 font-semibold tracking-[-0.01em] group-hover:text-green">
                {guide.heading}
              </span>
              <span className="text-[15px]/[23px] text-ink-2">{guide.description}</span>
            </a>
          </li>
        ))}
      </ul>
    </div>
  );
}
