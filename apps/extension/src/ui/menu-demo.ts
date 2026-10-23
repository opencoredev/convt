// A drawing of Chrome's image right-click menu with convt's submenu open. It shows
// people where to look, which a sentence alone doesn't.

import { h, mark } from "./dom.ts";

export function menuDemo(options: { compact: boolean }): HTMLElement {
  const item = (
    label: string,
    extra: { highlighted?: boolean; withMark?: boolean; submenu?: boolean } = {},
  ) =>
    h("div", { class: "md-item", "data-highlighted": extra.highlighted ? "true" : undefined }, [
      extra.withMark ? mark(14) : null,
      h("span", { class: "md-label" }, [label]),
      extra.submenu ? h("span", { class: "md-caret" }, ["›"]) : null,
    ]);
  const separator = () => h("div", { class: "md-separator" });

  return h(
    "div",
    { class: "md", "data-compact": options.compact ? "true" : undefined, "aria-hidden": "true" },
    [
      h("div", { class: "md-menu" }, [
        options.compact ? null : item("Open image in new tab"),
        item("Save image as…"),
        item("Copy image"),
        separator(),
        item("Convert with convt", { highlighted: true, withMark: true, submenu: true }),
      ]),
      h("div", { class: "md-menu md-sub" }, [
        item("Save as PNG", { highlighted: true }),
        item("Save as JPG"),
        item("Save as WebP"),
        separator(),
        item("Copy as PNG"),
      ]),
    ],
  );
}
