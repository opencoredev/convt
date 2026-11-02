import { expect, test } from "bun:test";

import { guidePath, guides, guidesIndexHead } from "#/lib/guides";
import { topLevelBreadcrumb } from "#/lib/site";

test("/guides keeps its title and canonical and lists every guide in its ItemList", () => {
  const head = guidesIndexHead();
  expect(head.meta).toContainEqual({ title: "Guides · convt" });
  expect(head.links).toContainEqual({ rel: "canonical", href: "https://convt.app/guides" });
  const data: unknown = JSON.parse(head.scripts[0].children);
  expect(data).toMatchObject({
    "@context": "https://schema.org",
    "@graph": [
      {
        "@type": "CollectionPage",
        url: "https://convt.app/guides",
        mainEntity: {
          "@type": "ItemList",
          numberOfItems: guides.length,
          itemListElement: guides.map((guide, index) => ({
            "@type": "ListItem",
            position: index + 1,
            name: guide.heading,
            url: `https://convt.app${guidePath(guide.slug)}`,
          })),
        },
      },
      {
        "@type": "BreadcrumbList",
        itemListElement: [
          { position: 1, item: "https://convt.app/" },
          { position: 2, name: "Guides", item: "https://convt.app/guides" },
        ],
      },
      { "@type": "Organization", "@id": "https://convt.app/#organization" },
    ],
  });
});

test("a top-level breadcrumb runs from the homepage to the page", () => {
  expect(topLevelBreadcrumb({ name: "Formats", path: "/formats" })).toEqual({
    "@type": "BreadcrumbList",
    itemListElement: [
      { "@type": "ListItem", position: 1, name: "convt", item: "https://convt.app/" },
      { "@type": "ListItem", position: 2, name: "Formats", item: "https://convt.app/formats" },
    ],
  });
});
