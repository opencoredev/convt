import { conversionBySlug, titleOf } from "#/lib/conversions";
import { GITHUB_URL, STATUS_URL, routes } from "#/lib/site";

/** Footer columns, shared by the landing page and the other public pages. */
export const footerColumns = [
  {
    title: "Product",
    links: [
      { label: "Download", href: routes.download },
      { label: "Formats", href: routes.formats },
      { label: "Conversions", href: "/convert" },
      { label: "Pricing", href: routes.pricing },
      { label: "Changelog", href: routes.changelog },
    ],
  },
  {
    title: "Developers",
    links: [
      { label: "API docs", href: routes.apiDocs },
      { label: "GitHub", href: GITHUB_URL },
      { label: "Status", href: STATUS_URL },
    ],
  },
  {
    title: "Company",
    links: [
      { label: "Privacy", href: routes.privacy },
      { label: "Terms", href: routes.terms },
      { label: "Contact", href: routes.contact },
    ],
  },
];

/** Header links on the public pages other than the landing page. */
export const headerLinks = [
  { label: "Download", href: routes.download },
  { label: "Formats", href: routes.formats },
  { label: "Pricing", href: routes.pricing },
  { label: "API docs", href: routes.apiDocs },
];

/** The most searched conversions, linked from the home page and the footer. */
export const popularConversions = [
  "heic-to-jpg",
  "mov-to-mp4",
  "pdf-to-jpg",
  "docx-to-pdf",
  "webp-to-png",
  "mp4-to-mp3",
].map((slug) => {
  const conversion = conversionBySlug.get(slug);
  if (!conversion) throw new Error(`links.ts: no conversion page ${slug}`);
  return { label: titleOf(conversion), href: `/convert/${slug}` };
});

/**
 * Footer columns while convt.app is a coming-soon page (LAUNCHED in src/lib/site.ts):
 * only pages the static deploy serves.
 */
export const comingSoonFooterColumns = [
  {
    title: "Convert",
    links: [...popularConversions.slice(0, 4), { label: "All conversions", href: "/convert" }],
  },
  {
    title: "convt",
    links: [
      { label: "Pricing", href: "/#pricing" },
      { label: "About", href: "/about" },
      { label: "Privacy", href: "/privacy" },
    ],
  },
];
