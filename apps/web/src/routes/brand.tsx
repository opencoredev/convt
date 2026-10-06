import { createFileRoute } from "@tanstack/react-router";

import { BrandPage } from "#/components/brand/brand-page";
import { seo } from "#/lib/site";

export const Route = createFileRoute("/brand")({
  // Designed dark, like the landing page. __root reads this and puts `dark` on <html>.
  staticData: { theme: "dark" },
  head: () => {
    const tags = seo({
      title: "Brand kit · convt",
      description:
        "The convt logo, wordmark, colors and type. Copy any logo as SVG or download the whole kit.",
      path: "/brand",
    });
    return { ...tags, meta: [...tags.meta, { name: "theme-color", content: "#0a0b0b" }] };
  },
  component: BrandPage,
});
