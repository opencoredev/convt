// Landing pages: top-level marketing pages for one audience or search, such as
// /mac-file-converter. Each has a route file under routes/_site; this list feeds their head
// tags, /llms.txt and the footer. Their sitemap dates live in `sitemapPages` in site.ts.

import { SITE_NAME, SITE_ORIGIN, routes, seo } from "./site";

export type LandingPage = {
  path: string;
  /** The <title>, shared cards and search result title. */
  title: string;
  /** The page's h1. */
  heading: string;
  /** A short link label for the site footer. */
  footerLabel: string;
  description: string;
  /** One plain line on what the page covers, for /llms.txt. */
  summary: string;
  /** `utm_campaign` on the page's download links. */
  campaign: string;
};

export const macFileConverter = {
  path: routes.macFileConverter,
  title: "Mac file converter for video, audio, images and PDFs · convt",
  heading: "A file converter for your Mac",
  footerLabel: "Mac file converter",
  description:
    "Convert video, audio, images, PDFs and Office files on a Mac from Finder's right-click menu or Terminal, folders included. Built-in Mac tools first.",
  summary:
    "What Finder, Preview, QuickTime and Music convert on a Mac, then what convt adds: the Finder menu, every format, folders in Terminal, limits and price.",
  campaign: "mac-file-converter",
} as const satisfies LandingPage;

/** The download page with this page's campaign tags. */
export const macFileConverterDownloadUrl = `${routes.download}?utm_source=convt_guides&utm_medium=organic&utm_campaign=${macFileConverter.campaign}`;

/** Head tags for /mac-file-converter: SEO and Open Graph, plus WebPage, app and breadcrumb JSON-LD. */
export function macFileConverterHead() {
  const page = macFileConverter;
  const url = `${SITE_ORIGIN}${page.path}`;
  const tags = seo({ title: page.title, description: page.description, path: page.path });
  const structuredData = {
    "@context": "https://schema.org",
    "@graph": [
      {
        "@type": "WebPage",
        "@id": `${url}#webpage`,
        url,
        name: page.title,
        description: page.description,
        inLanguage: "en",
        isPartOf: { "@id": `${SITE_ORIGIN}/#website` },
        about: { "@id": `${url}#software` },
      },
      {
        "@type": "SoftwareApplication",
        "@id": `${url}#software`,
        name: SITE_NAME,
        url: `${SITE_ORIGIN}/`,
        applicationCategory: "UtilitiesApplication",
        operatingSystem: "macOS 13 or later (Apple silicon)",
        offers: { "@type": "Offer", price: "39", priceCurrency: "USD" },
        publisher: { "@id": `${SITE_ORIGIN}/#organization` },
      },
      {
        "@type": "BreadcrumbList",
        itemListElement: [
          { "@type": "ListItem", position: 1, name: SITE_NAME, item: `${SITE_ORIGIN}/` },
          { "@type": "ListItem", position: 2, name: page.heading, item: url },
        ],
      },
      {
        "@type": "Organization",
        "@id": `${SITE_ORIGIN}/#organization`,
        name: SITE_NAME,
        url: `${SITE_ORIGIN}/`,
        logo: `${SITE_ORIGIN}/apple-touch-icon.png`,
      },
    ],
  };
  return {
    ...tags,
    scripts: [{ type: "application/ld+json", children: JSON.stringify(structuredData) }],
  };
}
