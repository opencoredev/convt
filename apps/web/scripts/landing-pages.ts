// Second half of the static coming-soon build (after scripts/landing-static.py): the
// Markdown versions of the /convert pages, sitemap.xml, and the conversions section
// of llms.txt. Everything comes from src/lib/conversions.ts, like the pages themselves.
//
// Usage: bun scripts/landing-pages.ts dist/client

import { appendFileSync, mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";

import { conversionMarkdown, hubMarkdown, urlOf } from "../src/lib/conversion-copy.ts";
import { conversionPaths, conversions, slugOf, titleOf } from "../src/lib/conversions.ts";
import { SITE_ORIGIN } from "../src/lib/site.ts";

const out = process.argv[2];
if (!out) throw new Error("usage: bun scripts/landing-pages.ts <dist/client>");

mkdirSync(join(out, "convert"), { recursive: true });
writeFileSync(join(out, "convert.md"), hubMarkdown());
for (const c of conversions) {
  writeFileSync(join(out, "convert", `${slugOf(c)}.md`), conversionMarkdown(c));
}

const today = new Date().toISOString().slice(0, 10);
const paths = ["/", "/about", "/privacy", ...conversionPaths];
writeFileSync(
  join(out, "sitemap.xml"),
  '<?xml version="1.0" encoding="UTF-8"?>\n' +
    '<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">\n' +
    paths
      .map((path) => `  <url><loc>${SITE_ORIGIN}${path}</loc><lastmod>${today}</lastmod></url>\n`)
      .join("") +
    "</urlset>\n",
);

appendFileSync(
  join(out, "llms.txt"),
  [
    "",
    "## Conversions",
    "",
    `- [All conversions](${SITE_ORIGIN}/convert.md): every conversion page and all supported formats`,
    ...conversions.map((c) => `- [${titleOf(c)}](${urlOf(c)}.md)`),
    "",
  ].join("\n"),
);

console.log(`landing-pages: ${conversions.length} conversion pages, ${paths.length} sitemap URLs`);
