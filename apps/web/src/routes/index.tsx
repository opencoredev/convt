import { createFileRoute } from "@tanstack/react-router";

import geistMonoUrl from "@fontsource-variable/geist-mono/files/geist-mono-latin-wght-normal.woff2?url";
import geistUrl from "@fontsource-variable/geist/files/geist-latin-wght-normal.woff2?url";

import { getPublicConfig } from "#/server/public-config";
import { getSession } from "#/server/session";

import { LandingPage } from "#/components/landing/landing-page";
import { accountFromSession } from "#/lib/account";
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
        // Both fonts are in the first paint; without these they load only after the CSS.
        {
          rel: "preload",
          as: "font",
          type: "font/woff2",
          href: geistUrl,
          crossOrigin: "anonymous",
        },
        {
          rel: "preload",
          as: "font",
          type: "font/woff2",
          href: geistMonoUrl,
          crossOrigin: "anonymous",
        },
      ],
      scripts: [{ type: "application/ld+json", children: JSON.stringify(structuredData) }],
    };
  },
  loader: async () => {
    const [config, session] = await Promise.all([getPublicConfig(), getSession()]);
    return {
      sales: config.sales,
      account: session ? accountFromSession(session) : null,
    };
  },
  component: () => {
    const { sales, account } = Route.useLoaderData();
    return <LandingPage sales={sales} account={account} />;
  },
});
