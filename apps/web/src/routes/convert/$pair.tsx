import { createFileRoute, notFound } from "@tanstack/react-router";

import { ConversionPage } from "#/components/convert/conversion-page";
import { conversionStructuredData, pageDescription, pageTitle } from "#/lib/conversion-copy";
import { conversionBySlug, slugOf } from "#/lib/conversions";
import { seo } from "#/lib/site";

// /convert/heic-to-jpg and the rest. The pages come from src/lib/conversions.ts; any
// other slug is a 404.
export const Route = createFileRoute("/convert/$pair")({
  staticData: { theme: "dark" },
  loader: ({ params }) => {
    const conversion = conversionBySlug.get(params.pair);
    if (!conversion) throw notFound();
    return conversion;
  },
  head: ({ loaderData: c }) => {
    if (!c) return {};
    const tags = seo({
      title: pageTitle(c),
      description: pageDescription(c),
      path: `/convert/${slugOf(c)}`,
    });
    return {
      ...tags,
      meta: [...tags.meta, { name: "theme-color", content: "#0a0b0b" }],
      scripts: [
        { type: "application/ld+json", children: JSON.stringify(conversionStructuredData(c)) },
      ],
    };
  },
  component: PairPage,
});

function PairPage() {
  return <ConversionPage conversion={Route.useLoaderData()} />;
}
