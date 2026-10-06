// Words for the /convert pages, shared by the React pages and their Markdown versions
// (scripts/landing-pages.ts), so an agent reading Markdown gets the same page.

import registry from "../../content/formats.json";
import {
  type Conversion,
  categories,
  commandFor,
  conversions,
  engines,
  formats,
  relatedTo,
  slugOf,
  titleOf,
} from "./conversions";
import { SITE_ORIGIN } from "./site";

export const pageTitle = (c: Conversion) =>
  `Convert ${titleOf(c)} on your computer, without uploading · convt`;

export const pageDescription = (c: Conversion) =>
  `${c.why} It all runs on your own computer, so nothing gets uploaded.`;

export const hubTitle = "Convert files on your computer, without uploading · convt";
export const hubDescription = `Image, video, audio and document conversions that run on your own computer: HEIC to JPG, MOV to MP4, PDF to JPG, DOCX to PDF and more. ${registry.formats.length} formats, nothing uploaded.`;

const registryNames = new Map(registry.formats.map((f) => [f.id, f.name]));

export const urlOf = (c: Conversion) => `${SITE_ORIGIN}/convert/${slugOf(c)}`;

/** Pair-specific questions first, then the ones every page answers. */
export function faqFor(c: Conversion): { q: string; a: string }[] {
  const from = formats[c.from].label;
  const to = formats[c.to].label;
  const folderExample = `convt ${c.category === "images" ? "Photos" : "Folder"} --to ${formats[c.to].ext}`;
  return [
    ...(c.faq ?? []),
    {
      q: `Does convt upload my ${from} files?`,
      a: `No. convt converts ${from} to ${to} on your own computer. Your files never leave it, and it works offline.`,
    },
    {
      q: `Can I convert many ${from} files at once?`,
      a: `Yes. Select several files and right-click, or pass a folder to the command line: \`${folderExample}\`. Add \`-r\` to include subfolders.`,
    },
    {
      q: "Which computers does convt run on?",
      a: "macOS, Windows and Linux. The right-click menu works in Finder on macOS and in Nautilus, Dolphin, Nemo and Thunar on Linux, with Explorer on Windows to follow. The `convt` command works on all three.",
    },
    {
      q: "Is convt free?",
      a: "convt is coming soon. The desktop app will cost $29 once, with a 7-day free trial, and you keep your version forever.",
    },
  ];
}

/** Steps for the "How to" section. */
export function stepsFor(c: Conversion) {
  const from = formats[c.from].label;
  const to = formats[c.to].label;
  // The menus list formats by their registry name ("JPEG"), not the page label ("JPG").
  const menuName = registryNames.get(c.to) ?? to;
  return [
    `Right-click a ${from} file, or several, in Finder or your file manager.`,
    c.menu === false
      ? `Choose Convert with convt, then More options…, and pick ${menuName} in Quick convert.`
      : `Choose Convert with convt, then ${menuName}.`,
    `The ${to} file appears next to the original. The ${from} stays untouched.`,
  ];
}

const plain = (text: string) => text.replace(/`([^`]+)`/g, "$1");

/** schema.org data for one conversion page: breadcrumbs and the FAQ. */
export function conversionStructuredData(c: Conversion) {
  return {
    "@context": "https://schema.org",
    "@graph": [
      {
        "@type": "BreadcrumbList",
        itemListElement: [
          { "@type": "ListItem", position: 1, name: "convt", item: `${SITE_ORIGIN}/` },
          { "@type": "ListItem", position: 2, name: "Conversions", item: `${SITE_ORIGIN}/convert` },
          { "@type": "ListItem", position: 3, name: titleOf(c), item: urlOf(c) },
        ],
      },
      {
        "@type": "FAQPage",
        mainEntity: faqFor(c).map(({ q, a }) => ({
          "@type": "Question",
          name: q,
          acceptedAnswer: { "@type": "Answer", text: plain(a) },
        })),
      },
    ],
  };
}

export function hubStructuredData() {
  return {
    "@context": "https://schema.org",
    "@type": "CollectionPage",
    name: "Conversions",
    url: `${SITE_ORIGIN}/convert`,
    description: hubDescription,
    mainEntity: {
      "@type": "ItemList",
      itemListElement: conversions.map((c, i) => ({
        "@type": "ListItem",
        position: i + 1,
        name: titleOf(c),
        url: urlOf(c),
      })),
    },
  };
}

/** Registry formats grouped the way the hub lists them. */
export const formatGroups = [
  { title: "Images", members: ["image", "vector"] },
  { title: "Video", members: ["video"] },
  { title: "Audio", members: ["audio"] },
  { title: "Documents", members: ["pdf", "document", "presentation", "spreadsheet"] },
].map((group) => ({
  title: group.title,
  formats: registry.formats.filter((f) => group.members.includes(f.category)),
}));

export function conversionMarkdown(c: Conversion) {
  const from = formats[c.from];
  const to = formats[c.to];
  const lines = [
    `# Convert ${titleOf(c)} on your computer`,
    "",
    c.why,
    "",
    "convt converts files with a right-click, on your own computer. Nothing gets uploaded. It is coming soon to macOS, Windows and Linux.",
    "",
    `## How to convert ${titleOf(c)} with convt`,
    "",
    ...stepsFor(c).map((step, i) => `${i + 1}. ${step}`),
    "",
    "From a terminal:",
    "",
    "```sh",
    commandFor(c),
    ...(c.options ?? []).map((o) => `${commandFor(c)} ${o.flag}   # ${o.does}`),
    "```",
    "",
    "## Good to know",
    "",
    ...c.notes.map((note) => `- ${note}`),
    "",
    "## The formats",
    "",
    `- **${from.label}:** ${from.about}`,
    `- **${to.label}:** ${to.about}`,
    "",
    `## How convt does it`,
    "",
    engines[c.engine].about,
    "",
    "## Questions",
    "",
    ...faqFor(c).flatMap(({ q, a }) => [`### ${q}`, "", a, ""]),
    "## Related conversions",
    "",
    ...relatedTo(c).map((r) => `- [${titleOf(r)}](${urlOf(r)}.md)`),
    "",
    `[All conversions](${SITE_ORIGIN}/convert.md) · [Home](${SITE_ORIGIN}/index.md)`,
    "",
  ];
  return lines.join("\n");
}

export function hubMarkdown() {
  const lines = [
    "# Convert files on your computer",
    "",
    "Every conversion below runs on your own computer with convt: right-click a file and pick a format, or use the `convt` command. Nothing gets uploaded. convt is coming soon to macOS, Windows and Linux.",
    "",
  ];
  for (const category of categories) {
    lines.push(`## ${category.title}`, "");
    for (const c of conversions.filter((x) => x.category === category.id)) {
      lines.push(`- [${titleOf(c)}](${urlOf(c)}.md): ${c.why}`);
    }
    lines.push("");
  }
  lines.push(`## All ${registry.formats.length} formats`, "");
  for (const group of formatGroups) {
    lines.push(`- **${group.title}:** ${group.formats.map((f) => f.name).join(", ")}`);
  }
  lines.push("", `[Home](${SITE_ORIGIN}/index.md)`, "");
  return lines.join("\n");
}
