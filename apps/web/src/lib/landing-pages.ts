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
  /** `operatingSystem` in the page's SoftwareApplication JSON-LD: only systems with a release. */
  operatingSystem: string;
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
  operatingSystem: "macOS 13 or later (Apple silicon)",
} as const satisfies LandingPage;

export const commandLineConverter = {
  path: routes.commandLineConverter,
  title: "Command-line file converter for images, video, audio and documents · convt",
  heading: "One command-line tool for every kind of file",
  footerLabel: "Command-line converter",
  description:
    "convt is a command-line file converter for images, video, audio, PDFs and Office documents: one command for every kind of file, whole folders, presets and JSON progress, run locally.",
  summary:
    "The convt command: one pattern for images, video, audio, PDFs and documents, folders, options, presets, JSON progress for scripts, the SKILL.md for AI agents, and how it compares with FFmpeg, ImageMagick, LibreOffice and Pandoc.",
  campaign: "command-line-file-converter",
  operatingSystem: "macOS (Apple silicon), Windows (x86_64), Linux (x86_64)",
} as const satisfies LandingPage;

/** The download page with a landing page's campaign tags. */
const landingDownloadUrl = (page: LandingPage) =>
  `${routes.download}?utm_source=convt_guides&utm_medium=organic&utm_campaign=${page.campaign}`;

export const macFileConverterDownloadUrl = landingDownloadUrl(macFileConverter);
export const commandLineConverterDownloadUrl = landingDownloadUrl(commandLineConverter);

/** Head tags for a landing page: SEO and Open Graph, plus WebPage, app and breadcrumb JSON-LD. */
function landingPageHead(page: LandingPage) {
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
        operatingSystem: page.operatingSystem,
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

/** Head tags for /mac-file-converter. */
export const macFileConverterHead = () => landingPageHead(macFileConverter);

/** Head tags for /command-line-file-converter. */
export const commandLineConverterHead = () => landingPageHead(commandLineConverter);
