// Guides: one task each, at /guides/<slug>. Each guide's route renders its own article;
// this list feeds the /guides index, the sitemap, the links on /formats and the head
// tags. Add a guide here and give it a route file under routes/_site/guides.

import { SITE_NAME, SITE_ORIGIN, routes, seo } from "./site";

export type Guide = {
  slug: string;
  /** The <title>, shared cards and search result title. */
  title: string;
  /** The page's h1 and its link text elsewhere on the site. */
  heading: string;
  description: string;
  /** One plain line on what the guide covers, for /llms.txt. */
  summary: string;
  /** The /formats section that links here. */
  category: "images" | "video" | "audio" | "documents";
  /** ISO dates. */
  published: string;
  updated: string;
};

export const guides = [
  {
    slug: "heic-to-jpg-mac",
    title: "How to convert HEIC to JPG on Mac: free, batch and Terminal",
    heading: "How to convert HEIC to JPG on a Mac",
    description:
      "Convert iPhone HEIC photos to JPG on a Mac with free built-in tools (Finder, Preview, Photos and sips), or batch-convert folders with subfolders using the convt CLI.",
    summary:
      "Finder Quick Actions, Preview, Photos and sips, then folders with subfolders in the convt CLI. Notes that convt 0.5.0 drops EXIF on this route.",
    category: "images",
    published: "2026-10-29",
    updated: "2026-10-30",
  },
  {
    slug: "batch-heic-to-jpg-windows",
    title: "How to batch convert HEIC to JPG on Windows 11, folders included",
    heading: "How to batch convert HEIC to JPG on Windows",
    description:
      "Convert iPhone HEIC photos to JPG on Windows 11: one at a time in the Photos app, or whole folders with subfolders in one tested convt command.",
    summary:
      "The HEIF and HEVC extensions and Photos for single photos, then whole folders with subfolders in the convt CLI. Notes that convt 0.5.0 drops EXIF on this route.",
    category: "images",
    published: "2026-10-29",
    updated: "2026-10-30",
  },
  {
    slug: "mov-to-mp4-mac",
    title: "How to convert MOV to MP4 on Mac: rename, iMovie, FFmpeg or batch",
    heading: "How to convert MOV to MP4 on a Mac",
    description:
      "Convert MOV videos to MP4 on a Mac: rename H.264 files, export from iMovie, remux with FFmpeg without losing quality, or batch-convert and shrink folders with convt.",
    summary:
      "When renaming works, iMovie, FFmpeg remux vs re-encode with tested commands, and whole folders plus smaller files with the convt CLI. Covers Windows too.",
    category: "video",
    published: "2026-10-30",
    updated: "2026-10-30",
  },
  {
    slug: "webp-to-png-jpg",
    title: "How to convert WebP to PNG or JPG on Mac and Windows",
    heading: "How to convert WebP to PNG or JPG",
    description:
      "Save WebP images as PNG or JPG with Preview on a Mac or Paint on Windows, or convert whole folders at once with convt, keeping transparency in PNG.",
    summary:
      "Preview and Paint for one image, then whole folders with the convt CLI. Covers transparency, JPG backgrounds and animated WebP.",
    category: "images",
    published: "2026-10-30",
    updated: "2026-10-30",
  },
] as const satisfies readonly Guide[];

export type GuideSlug = (typeof guides)[number]["slug"];

export const guidePath = (slug: GuideSlug) => `${routes.guides}/${slug}`;

export function guideBySlug(slug: GuideSlug): Guide {
  const guide = guides.find((g) => g.slug === slug);
  if (!guide) throw new Error(`unknown guide: ${slug}`);
  return guide;
}

/** Every other guide, the ones about the same kind of file first, for a guide's "Related guides" list. */
export function relatedGuides(guide: Guide): Guide[] {
  const others: Guide[] = guides.filter((g) => g.slug !== guide.slug);
  return [
    ...others.filter((g) => g.category === guide.category),
    ...others.filter((g) => g.category !== guide.category),
  ];
}

/**
 * The download page with the guide's campaign tags, so a signup that starts from a
 * guide can be told apart from one that starts on the landing page.
 */
export const guideDownloadUrl = (slug: GuideSlug) =>
  `${routes.download}?utm_source=convt_guides&utm_medium=organic&utm_campaign=${slug}`;

/** Head tags for a guide: SEO and Open Graph as an article, plus Article and breadcrumb JSON-LD. */
export function guideHead(slug: GuideSlug) {
  const guide = guideBySlug(slug);
  const path = guidePath(slug);
  const url = `${SITE_ORIGIN}${path}`;
  const tags = seo({ title: guide.title, description: guide.description, path, type: "article" });
  const structuredData = {
    "@context": "https://schema.org",
    "@graph": [
      {
        "@type": "TechArticle",
        "@id": `${url}#article`,
        headline: guide.heading,
        description: guide.description,
        url,
        mainEntityOfPage: url,
        datePublished: guide.published,
        dateModified: guide.updated,
        image: `${SITE_ORIGIN}/og.png`,
        inLanguage: "en",
        author: { "@id": `${SITE_ORIGIN}/#organization` },
        publisher: { "@id": `${SITE_ORIGIN}/#organization` },
      },
      {
        "@type": "BreadcrumbList",
        itemListElement: [
          { "@type": "ListItem", position: 1, name: SITE_NAME, item: `${SITE_ORIGIN}/` },
          {
            "@type": "ListItem",
            position: 2,
            name: "Guides",
            item: `${SITE_ORIGIN}${routes.guides}`,
          },
          { "@type": "ListItem", position: 3, name: guide.heading, item: url },
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
    meta: [
      ...tags.meta,
      { property: "article:published_time", content: guide.published },
      { property: "article:modified_time", content: guide.updated },
    ],
    scripts: [{ type: "application/ld+json", children: JSON.stringify(structuredData) }],
  };
}
