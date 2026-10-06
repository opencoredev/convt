import { createFileRoute } from "@tanstack/react-router";

import { cx } from "#/components/app/ui";
import { PageHeader, TextLink, siteColumn } from "#/components/site/layout";
import { routes, seo } from "#/lib/site";
import data from "../../../content/formats.json";

// content/formats.json is regenerated from the convt CLI by scripts/generate-content.ts
// before every build, so this page lists exactly what the registry routes.

type Format = (typeof data.formats)[number];

const byId = new Map(data.formats.map((f) => [f.id, f]));

// Sections on the page, each covering one or more registry categories.
const categories = [
  { id: "images", title: "Images", members: ["image", "vector"] },
  { id: "video", title: "Video", members: ["video"] },
  { id: "audio", title: "Audio", members: ["audio"] },
  {
    id: "documents",
    title: "Documents",
    members: ["pdf", "document", "presentation", "spreadsheet"],
  },
] as const;

type Category = (typeof categories)[number];

const inCategory = (c: Category) =>
  data.formats.filter((f) => (c.members as readonly string[]).includes(f.category));

// LibreOffice formats need the optional document pack; PDF and the rest ship with convt.
const packFormats = new Set([
  "docx",
  "doc",
  "odt",
  "rtf",
  "txt",
  "html",
  "pptx",
  "ppt",
  "odp",
  "xlsx",
  "xls",
  "ods",
  "csv",
]);

const description = `All ${data.formats.length} formats convt converts between: images, video, audio and documents, and the targets each one reaches.`;

export const Route = createFileRoute("/_site/formats")({
  head: () => seo({ title: "Supported formats · convt", description, path: routes.formats }),
  component: FormatsPage,
});

function FormatsPage() {
  return (
    <div className={cx(siteColumn, "flex flex-col gap-14 pt-12 pb-20 md:pt-16")}>
      <PageHeader eyebrow="Formats" title={`${data.formats.length} formats, one menu.`}>
        <p>
          Every format convt reads and the formats each one converts to. Right-click a file and the
          menu lists exactly these targets. This page is generated from the converter itself, so it
          matches the app.
        </p>
      </PageHeader>

      <nav aria-label="Categories" className="-mt-4 flex flex-wrap gap-2">
        {categories.map((c) => (
          <a
            key={c.id}
            href={`#${c.id}`}
            className="rounded-lg bg-chip px-3 py-1.5 text-[13px]/4 font-medium text-ink shadow-[inset_0_0_0_1px_var(--chip-line)] hover:bg-hover"
          >
            {c.title} <span className="font-mono text-xs text-ink-2">{inCategory(c).length}</span>
          </a>
        ))}
      </nav>

      {categories.map((c) => (
        <CategorySection key={c.id} category={c} />
      ))}
    </div>
  );
}

function CategorySection({ category }: { category: Category }) {
  const { id, title } = category;
  const formats = inCategory(category);
  return (
    <section aria-labelledby={`${id}-title`} id={id} className="flex scroll-mt-6 flex-col gap-5">
      <h2 id={`${id}-title`} className="text-2xl/8 font-semibold tracking-[-0.02em]">
        {title}
      </h2>
      {id === "documents" && <DocumentPack />}
      <div className="overflow-clip rounded-2xl bg-raised shadow-[inset_0_0_0_1px_var(--line)] dark:bg-panel">
        <div
          aria-hidden="true"
          className="hidden grid-cols-[220px_1fr] gap-6 border-b border-line px-5 py-2.5 font-mono text-[11px]/3.5 text-ink-2 uppercase md:grid"
        >
          <span>Format</span>
          <span>Converts to</span>
        </div>
        <ul className="divide-y divide-divider">
          {formats.map((format) => (
            <FormatRow key={format.id} format={format} />
          ))}
        </ul>
      </div>
    </section>
  );
}

function FormatRow({ format }: { format: Format }) {
  return (
    <li className="flex flex-col gap-3 px-5 py-4 md:grid md:grid-cols-[220px_1fr] md:gap-6">
      <div className="flex flex-col gap-1">
        <h3 className="flex items-center gap-2 text-[15px]/5 font-medium">
          {format.name}
          {packFormats.has(format.id) && (
            <span className="rounded-[5px] bg-green-tint px-1.5 py-px font-mono text-[10.5px]/4 font-normal text-[#157f4a] dark:text-green">
              pack
            </span>
          )}
        </h3>
        <p className="font-mono text-xs/4 text-ink-2">
          {format.extensions.map((ext) => `.${ext}`).join(" ")}
        </p>
      </div>
      <div className="flex flex-col gap-1.5">
        <span className="font-mono text-[11px]/3.5 text-ink-2 uppercase md:hidden">
          Converts to
        </span>
        <ul aria-label={`${format.name} converts to`} className="flex flex-wrap gap-1.5">
          {format.targets.map((id) => (
            <li
              key={id}
              className="rounded-md bg-chip px-[7px] py-[3px] font-mono text-[11.5px]/[14px] text-ink-2 shadow-[inset_0_0_0_1px_var(--chip-line)]"
            >
              {byId.get(id)?.name ?? id}
            </li>
          ))}
        </ul>
      </div>
    </li>
  );
}

function DocumentPack() {
  return (
    <div className="flex flex-col gap-2 rounded-xl bg-green-tint/60 px-5 py-4 shadow-[inset_0_0_0_1px_var(--green-line)] md:flex-row md:gap-6">
      <h3 className="shrink-0 text-sm/5 font-semibold md:w-[196px]">The document pack</h3>
      <div className="flex flex-col gap-2 text-sm/[21px] text-ink-2">
        <p>
          Word, Excel and PowerPoint files and their open formats (marked{" "}
          <span className="font-mono text-xs text-[#157f4a] dark:text-green">pack</span>) convert
          through LibreOffice, which comes as an optional download instead of inside every install.
          The first time you pick one of these files, convt offers to install document support. It
          downloads once, only after you click Install, and from then on documents convert offline
          like everything else.
        </p>
        <p>
          PDF, images, video and audio work right after install. If LibreOffice is already on your
          computer, convt can use it. PDFs convert to one image per page. Questions?{" "}
          <TextLink href={routes.contact}>Get in touch</TextLink>.
        </p>
      </div>
    </div>
  );
}
