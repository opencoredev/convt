import { GITHUB_URL, routes } from "#/lib/site";

/** Footer columns, shared by the landing page and the other public pages. */
export const footerColumns = [
  {
    title: "Product",
    links: [
      { label: "Download", href: routes.download },
      { label: "Formats", href: routes.formats },
      { label: "Pricing", href: routes.pricing },
      { label: "Changelog", href: routes.changelog },
    ],
  },
  {
    title: "Developers",
    links: [
      { label: "API docs", href: routes.apiDocs },
      { label: "GitHub", href: GITHUB_URL },
    ],
  },
  {
    title: "Company",
    links: [
      { label: "Privacy", href: routes.privacy },
      { label: "Terms", href: routes.terms },
      { label: "Contact", href: routes.contact },
      { label: "Brand", href: routes.brand },
    ],
  },
];

/** Header links on every public page that uses the shared site nav. */
export const headerLinks = [
  { label: "Download", href: routes.download },
  { label: "Formats", href: routes.formats },
  { label: "Pricing", href: routes.pricing },
  { label: "API docs", href: routes.apiDocs },
];
