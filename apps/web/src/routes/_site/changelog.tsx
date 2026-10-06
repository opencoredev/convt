import { createFileRoute } from "@tanstack/react-router";

import { cx } from "#/components/app/ui";
import { PageHeader, siteColumn } from "#/components/site/layout";
import { Blocks, Inline, parseMarkdown, splitAtLevel2 } from "#/components/site/markdown";
import { routes, seo } from "#/lib/site";
import source from "../../../content/changelog.md?raw";

// Edit content/changelog.md; the page is built from it. Each "## " heading is a release.
const blocks = parseMarkdown(source);
const title = blocks.find((b) => b.type === "heading" && b.level === 1);
const { intro, sections } = splitAtLevel2(blocks.filter((b) => b !== title));

export const Route = createFileRoute("/_site/changelog")({
  head: () =>
    seo({
      title: "Changelog · convt",
      description: "What changed in each convt release.",
      path: routes.changelog,
    }),
  component: ChangelogPage,
});

function ChangelogPage() {
  return (
    <div className={cx(siteColumn, "flex flex-col gap-12 pt-12 pb-20 md:pt-16")}>
      <PageHeader eyebrow="Changelog" title={title?.type === "heading" ? title.text : "Changelog"}>
        {intro.map((block, i) =>
          block.type === "paragraph" ? (
            <p key={i}>
              <Inline text={block.text} />
            </p>
          ) : null,
        )}
      </PageHeader>
      <ol className="flex flex-col">
        {sections.map((section) => (
          <li
            key={section.title}
            className="grid gap-4 border-t border-line py-10 md:grid-cols-[220px_1fr] md:gap-10"
          >
            <h2
              id={`v${section.title}`}
              className="text-[22px]/7 font-semibold tracking-[-0.02em] md:sticky md:top-6 md:self-start"
            >
              {section.title}
            </h2>
            <div className="flex max-w-[640px] flex-col gap-4">
              <Blocks blocks={section.blocks} />
            </div>
          </li>
        ))}
      </ol>
    </div>
  );
}
