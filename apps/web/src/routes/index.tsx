import { createFileRoute } from "@tanstack/react-router";

import { LandingPage } from "#/components/landing/landing-page";
import { fontPreloads } from "#/lib/font-preloads";
import { SITE_NAME, SITE_ORIGIN, seo } from "#/lib/site";

const title = "convt: convert any file with a right-click";
const description =
  "Images, video, audio and documents, converted on your own computer. Nothing gets uploaded.";

// Lets search engines and agents read what convt is without parsing the page.
const structuredData = {
  "@context": "https://schema.org",
  "@graph": [
    {
      "@type": "SoftwareApplication",
      name: SITE_NAME,
      url: `${SITE_ORIGIN}/`,
      description,
      applicationCategory: "UtilitiesApplication",
      operatingSystem: "macOS, Windows, Linux",
      license: "https://www.gnu.org/licenses/agpl-3.0.html",
      image: `${SITE_ORIGIN}/og.png`,
      offers: { "@type": "Offer", price: "29", priceCurrency: "USD" },
      publisher: { "@id": `${SITE_ORIGIN}/#organization` },
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

export const Route = createFileRoute("/")({
  // The landing page is designed dark only. __root reads this and puts `dark` on <html>.
  staticData: { theme: "dark" },
  head: () => {
    const tags = seo({ title, description, path: "/" });
    return {
      ...tags,
      meta: [...tags.meta, { name: "theme-color", content: "#0a0b0b" }],
      links: [
        ...tags.links,
        // The glow behind the hero is the largest paint, but CSS hides it from the preload
        // scanner, so ask for it up front.
        { rel: "preload", as: "image", href: "/landing/dither-glow.png", fetchPriority: "high" },
        ...fontPreloads,
      ],
      scripts: [{ type: "application/ld+json", children: JSON.stringify(structuredData) }],
    };
  },
  component: LandingPage,
});
