import { createFileRoute } from "@tanstack/react-router";

import { HubPage } from "#/components/convert/hub-page";
import { hubDescription, hubStructuredData, hubTitle } from "#/lib/conversion-copy";
import { seo } from "#/lib/site";

export const Route = createFileRoute("/convert/")({
  // Same dark design as the landing page; __root puts `dark` on <html>.
  staticData: { theme: "dark" },
  head: () => {
    const tags = seo({ title: hubTitle, description: hubDescription, path: "/convert" });
    return {
      ...tags,
      meta: [...tags.meta, { name: "theme-color", content: "#0a0b0b" }],
      scripts: [{ type: "application/ld+json", children: JSON.stringify(hubStructuredData()) }],
    };
  },
  component: HubPage,
});
