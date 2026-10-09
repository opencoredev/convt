// bun run --cwd packages/mail export:marketing [out-dir]
//
// Writes each campaign template as HTML and plain text, ready to paste into a
// Sequenzy campaign or sequence step. Sends nothing and calls no API. Set
// MARKETING_POSTAL_ADDRESS to fill the footer's postal address; without it the
// footer keeps a visible marker so a campaign cannot go out with a blank one.

import { mkdirSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";

import { marketingTemplates } from "../src/marketing";

const out = resolve(process.argv[2] ?? "marketing-export");
const siteUrl = process.env.SITE_URL ?? "https://convt.app";
const postalAddress = process.env.MARKETING_POSTAL_ADDRESS?.trim() || "[postal address required]";

mkdirSync(out, { recursive: true });
for (const [name, email] of Object.entries(marketingTemplates({ siteUrl, postalAddress }))) {
  writeFileSync(join(out, `${name}.html`), email.html);
  writeFileSync(
    join(out, `${name}.txt`),
    `Subject: ${email.subject}\nPreview: ${email.preheader}\n\n${email.text}`,
  );
  console.log(`${name}: ${email.subject}`);
}
if (!process.env.MARKETING_POSTAL_ADDRESS?.trim())
  console.warn("export:marketing: MARKETING_POSTAL_ADDRESS is not set; the footer shows a marker");
console.log(`export:marketing: wrote ${out}`);
