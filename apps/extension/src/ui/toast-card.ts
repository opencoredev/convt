// One toast card. Built once per job and updated in place, so the status chip can
// cross-fade from the arrow to a check instead of the whole card being replaced.

import { toastCopy, type ToastAction, type ToastCopy } from "../shared/copy.ts";
import type { ToastView } from "../shared/jobs.ts";
import { h, icon, icons } from "./dom.ts";

export type CardHandlers = {
  onAction: (action: ToastAction) => void;
  onClose: () => void;
};

export type Card = {
  element: HTMLElement;
  update: (view: Exclude<ToastView, { phase: "dismissed" }>) => ToastCopy;
};

export function createCard(options: {
  srcUrl: string;
  inline: boolean;
  handlers: CardHandlers;
}): Card {
  const image = h("img", { class: "thumb-image", alt: "", decoding: "async" });
  const fallback = h("span", { class: "thumb-fallback", hidden: true }, [
    icon(icons.image, { size: 18, stroke: 1.5 }),
  ]);
  image.addEventListener("error", () => {
    image.hidden = true;
    fallback.hidden = false;
  });
  // The page already loaded this image, so it shows instantly from cache.
  if (options.srcUrl) image.src = options.srcUrl;
  else {
    image.hidden = true;
    fallback.hidden = false;
  }

  const chip = h("span", { class: "chip" }, [
    h("span", { class: "chip-icon", "data-icon": "working" }, [
      icon(icons.arrow, { size: 12, stroke: 2.5 }),
    ]),
    h("span", { class: "chip-icon", "data-icon": "done" }, [
      icon(icons.check, { size: 12, stroke: 2.75 }),
    ]),
    h("span", { class: "chip-icon", "data-icon": "failed" }, [
      icon(icons.alert, { size: 12, stroke: 2.75 }),
    ]),
  ]);

  const title = h("span", { class: "title" });
  const detail = h("span", { class: "detail" });
  const actionSlot = h("span", { class: "action-slot" });
  const note = h("p", { class: "note", hidden: true });
  const close = h("button", { class: "close", type: "button", "aria-label": "Dismiss" }, [
    icon(icons.close, { size: 14, stroke: 1.75 }),
  ]);
  close.addEventListener("click", options.handlers.onClose);

  const element = h(
    "section",
    { class: "toast", "data-inline": options.inline ? "true" : undefined, "data-open": "false" },
    [
      h("div", { class: "row" }, [
        h("span", { class: "thumb" }, [image, fallback, chip]),
        h("div", { class: "text" }, [title, detail, actionSlot]),
        h("div", { class: "side" }, [close]),
      ]),
      note,
    ],
  );

  const update: Card["update"] = (view) => {
    // A huge data: image starts without a thumbnail and gets the converted preview later.
    if (view.thumb && image.getAttribute("src") !== view.thumb) {
      image.hidden = false;
      fallback.hidden = true;
      image.src = view.thumb;
    }
    const copy = toastCopy(view, options.srcUrl);
    element.dataset.tone = copy.tone;
    // Failures interrupt. Progress and success are announced politely: by the card
    // itself inline, or on a page by the stack, a live region that already exists
    // when the card arrives (a region inserted with its text often goes unread).
    if (copy.tone === "failed") element.setAttribute("role", "alert");
    else if (options.inline) element.setAttribute("role", "status");
    else element.removeAttribute("role");
    element.setAttribute("aria-label", copy.title);
    title.textContent = copy.title;
    title.title = copy.title;
    detail.textContent = copy.detail.text;
    detail.dataset.mono = String(copy.detail.mono);
    detail.title = copy.detail.mono ? copy.detail.text : "";

    actionSlot.replaceChildren();
    if (copy.action) actionSlot.append(actionButton(copy.action, options.handlers));

    note.replaceChildren();
    note.hidden = copy.note === null;
    if (copy.note) {
      note.append(copy.note.text);
      if (copy.note.link) {
        note.append(" ", externalLink(copy.note.link.label, copy.note.link.href));
      }
    }
    return copy;
  };

  return { element, update };
}

function actionButton(action: ToastAction, handlers: CardHandlers): HTMLElement {
  if (action.kind === "link") {
    const link = externalLink(action.label, action.href);
    link.className = "button";
    return link;
  }
  const button = h(
    "button",
    {
      class: "button",
      type: "button",
      "data-primary": action.kind === "open-access" ? "true" : undefined,
    },
    [action.label],
  );
  button.addEventListener("click", () => handlers.onAction(action));
  return button;
}

function externalLink(label: string, href: string): HTMLAnchorElement {
  // noreferrer: inside a web page, the referrer would tell convt.app which page the
  // user was reading.
  return h("a", { class: "note-link", href, target: "_blank", rel: "noopener noreferrer" }, [
    label,
  ]);
}
