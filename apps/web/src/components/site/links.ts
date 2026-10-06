import { GITHUB_URL, STATUS_URL, routes } from "#/lib/site";

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
      { label: "Status", href: STATUS_URL },
    ],
  },
  {
    title: "Company",
    links: [
      { label: "Privacy", href: routes.privacy },
      { label: "Terms", href: routes.terms },
      { label: "Contact", href: routes.contact },
    ],
  },
];

/** Header links on the public pages other than the landing page. */
export const headerLinks = [
  { label: "Download", href: routes.download },
  { label: "Formats", href: routes.formats },
  { label: "Pricing", href: routes.pricing },
  { label: "API docs", href: routes.apiDocs },
];
