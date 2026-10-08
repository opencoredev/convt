// The page that opens after install. It has one job: get a first conversion done.
// The steps tick themselves off as the user right-clicks the photo and the file
// arrives, so the page always says what to do next.

import { parseRecent, type RecentItem, type ToastView } from "../shared/jobs.ts";
import { siteUrl } from "../shared/links.ts";
import { parseToContent, type CopyResult, type ToBackground } from "../shared/messages.ts";
import { copyPng } from "../ui/clipboard.ts";
import { h, icon, icons, mark } from "../ui/dom.ts";
import { menuDemo } from "../ui/menu-demo.ts";
import { createCard } from "../ui/toast-card.ts";

const DEMO_IMAGE = "images/miso.webp";

type Stage = { kind: "start" } | { kind: "menu-open" } | { kind: "done"; item: RecentItem };

type Step = { title: string; hint: string };
const STEPS: [Step, Step, Step] = [
  { title: "Right-click the photo of Miso", hint: "Any image on the web works the same way." },
  { title: "Choose Convert with convt", hint: "It's near the bottom of the menu." },
  { title: "Pick a format", hint: "Save as PNG is a good first try." },
];

function link(label: string, href: string, className: string): HTMLAnchorElement {
  return h("a", { class: className, href, target: "_blank", rel: "noopener" }, [label]);
}

export async function renderOnboarding(root: HTMLElement) {
  document.title = "Welcome to convt";
  const openedAt = Date.now();
  let stage: Stage = { kind: "start" };

  // --- The photo -------------------------------------------------------------------
  const photo = h("img", {
    src: DEMO_IMAGE,
    alt: "Miso the cat. Right-click this photo to try convt.",
    width: "640",
    height: "440",
  });
  const cue = h("span", { class: "cue" }, [rightClickIcon(), "Right-click me"]);
  const figure = h("figure", { class: "photo" }, [
    h("div", { class: "photo-frame" }, [photo, cue]),
    h("figcaption", { class: "photo-caption" }, [
      h("span", { class: "photo-name" }, ["miso.webp"]),
      h("span", { class: "photo-size" }, ["WebP image"]),
    ]),
  ]);

  // --- The steps ---------------------------------------------------------------------
  const stepItems = STEPS.map((step, index) =>
    h("li", { class: "step" }, [
      h("span", { class: "step-marker", "aria-hidden": "true" }, [
        h("span", { class: "step-number" }, [String(index + 1)]),
        h("span", { class: "step-check" }, [icon(icons.check, { size: 13, stroke: 2.75 })]),
      ]),
      h("span", { class: "step-text" }, [
        h("span", { class: "step-title" }, [step.title]),
        h("span", { class: "step-hint" }, [step.hint]),
      ]),
    ]),
  );
  const steps = h("ol", { class: "steps", "aria-label": "Try convt" }, stepItems);

  // --- What appears under the steps: the menu drawing, then the result --------------
  const drawingNote = h("p", { class: "drawing-note" }, [
    "This is a picture of the menu. Right-click the photo to open the real one.",
  ]);
  const drawing = h("div", { class: "drawing" }, [
    h("span", { class: "drawing-label" }, ["What you'll see"]),
    menuDemo({ compact: true }),
  ]);
  // People try to click pictures of menus. Point them back to the photo.
  drawing.addEventListener("click", () => {
    drawing.dataset.nudged = "true";
    figure.dataset.nudge = "true";
    setTimeout(() => (figure.dataset.nudge = "false"), 900);
  });
  const below = h("div", { class: "below", "aria-live": "polite" }, [drawing, drawingNote]);

  const stageTitle = h("h2", { class: "stage-title", id: "stage-title" }, ["Try it now"]);
  const stageEl = h("section", { class: "stage", "aria-labelledby": "stage-title" }, [
    figure,
    h("div", { class: "stage-side" }, [stageTitle, steps, below]),
  ]);

  const pin = await pinCard();

  const update = () => {
    const current = stage.kind === "start" ? 0 : stage.kind === "menu-open" ? 1 : 3;
    stepItems.forEach((item, index) => {
      item.dataset.state = index < current ? "done" : index === current ? "current" : "todo";
      if (index === current) item.setAttribute("aria-current", "step");
      else item.removeAttribute("aria-current");
    });
    // Step 2 and 3 happen in one menu, so both stay "current" while it's open.
    if (stage.kind === "menu-open") stepItems[2]?.setAttribute("data-state", "current");
    cue.hidden = stage.kind !== "start";
    stageEl.dataset.stage = stage.kind;
    if (stage.kind === "done") {
      below.replaceChildren(resultCard(stage.item), doneNote());
      stageTitle.textContent = "You converted your first image";
    }
    pin.dataset.highlight = String(stage.kind === "done");
  };

  photo.addEventListener("contextmenu", () => {
    if (stage.kind === "start") {
      stage = { kind: "menu-open" };
      update();
    }
  });

  chrome.storage.onChanged.addListener((changes, area) => {
    if (area !== "local" || !changes.recent) return;
    const [latest] = parseRecent(changes.recent.newValue);
    if (latest && latest.at >= openedAt) {
      stage = { kind: "done", item: latest };
      update();
    }
  });

  // This page is convt's own, so the toast script can't run here; Copy as PNG lands
  // here instead. Only the tab the worker names answers.
  const ownTabId = (await chrome.tabs.getCurrent())?.id ?? null;
  chrome.runtime.onMessage.addListener((raw: unknown, _sender, sendResponse) => {
    const message = parseToContent(raw);
    if (message?.kind !== "copy-image" || message.tabId !== ownTabId) return false;
    void copyPng(message.png).then(({ ok }) => {
      const result: CopyResult = { ok };
      sendResponse(result);
    });
    return true;
  });

  root.replaceChildren(
    h("div", { class: "wrap" }, [
      h("nav", { class: "nav" }, [
        h("span", { class: "brand" }, [mark(24), "convt"]),
        link("convt.app", siteUrl("/", "welcome-header"), "nav-link"),
      ]),
      h("header", { class: "hero" }, [
        h("p", { class: "eyebrow" }, [
          icon(icons.check, { size: 14, stroke: 2.5 }),
          "convt is installed",
        ]),
        h("h1", {}, ["Save any image on the web as PNG, JPG or WebP."]),
        h("p", { class: "lede" }, [
          "Right-click it and choose the format. convt converts it inside Chrome, so the image never leaves your computer.",
        ]),
      ]),
      stageEl,
      h("div", { class: "facts" }, [pin, privacyCard()]),
      desktopBand(),
      h("footer", { class: "footer" }, [
        link("convt.app", siteUrl("/", "welcome-header"), ""),
        link("Privacy", siteUrl("/privacy", "welcome-header"), ""),
      ]),
    ]),
  );
  update();
}

function resultCard(item: RecentItem): HTMLElement {
  const view: ToastView =
    item.kind === "saved"
      ? {
          phase: "saved",
          jobId: item.jobId,
          thumb: item.thumb,
          name: item.name,
          bytes: item.bytes,
          from: "webp",
          target: item.target,
          downloadId: item.downloadId,
          note: null,
        }
      : {
          phase: "copied",
          jobId: item.jobId,
          thumb: item.thumb,
          bytes: item.bytes,
          from: "webp",
          note: null,
        };
  const card = createCard({
    srcUrl: item.thumb,
    inline: true,
    handlers: {
      onAction: (action) => {
        if (action.kind === "show-download") {
          const message: ToBackground = { kind: "show-download", downloadId: action.downloadId };
          void chrome.runtime.sendMessage(message);
        }
      },
      onClose: () => card.element.remove(),
    },
  });
  card.update(view);
  requestAnimationFrame(() => requestAnimationFrame(() => (card.element.dataset.open = "true")));
  return card.element;
}

function doneNote(): HTMLElement {
  return h("p", { class: "done-note" }, [
    h("strong", {}, ["That's all there is to it."]),
    " Try it on any image on any site. If a site asks for permission first, convt will tell you.",
  ]);
}

/** Pinning keeps recent files and settings one click away. Ticks itself when done. */
async function pinCard(): Promise<HTMLElement> {
  const status = h("p", { class: "fact-status" });
  const card = h("section", { class: "fact fact-pin" }, [
    h("span", { class: "fact-next" }, ["Next"]),
    h("h2", {}, ["Pin convt to your toolbar"]),
    h("p", {}, [
      "Click the puzzle piece next to the address bar, then the pin next to convt. Your recent files and settings live there.",
    ]),
    status,
  ]);
  const draw = (pinned: boolean) => {
    card.dataset.pinned = String(pinned);
    status.replaceChildren(
      ...(pinned ? [icon(icons.check, { size: 14, stroke: 2.5 }), "Pinned"] : []),
    );
  };
  const check = async () => {
    const settings = await chrome.action.getUserSettings().catch(() => null);
    if (settings) draw(settings.isOnToolbar);
  };
  await check();
  if (chrome.action.onUserSettingsChanged) {
    chrome.action.onUserSettingsChanged.addListener((change) => {
      if (change.isOnToolbar !== undefined) draw(change.isOnToolbar);
    });
  } else {
    // Before Chrome 130 there's no event; checking while the page is open is enough.
    setInterval(() => void check(), 1500);
  }
  return card;
}

function privacyCard(): HTMLElement {
  return h("section", { class: "fact" }, [
    h("h2", {}, ["Your images stay on your computer"]),
    h("p", {}, [
      "convt has no account, no server to send images to and no tracking. It only reads the image you right-click.",
    ]),
  ]);
}

function desktopBand(): HTMLElement {
  return h("section", { class: "desktop", "aria-labelledby": "desktop-title" }, [
    h("div", {}, [
      h("h2", { id: "desktop-title" }, ["Need more than images?"]),
      h("p", {}, [
        "convt for desktop puts the same right-click on every file on your Mac or PC: video, audio, PDFs, documents, HEIC photos and whole folders. Free for 7 days, then $29 once.",
      ]),
    ]),
    h("div", { class: "card-actions" }, [
      link("Get convt for desktop", siteUrl("/download", "welcome-desktop"), "btn"),
    ]),
  ]);
}

/** A mouse with its right button filled in. */
function rightClickIcon(): SVGSVGElement {
  const svg = icon(["M12 3v7", "M6 10h12"], { size: 15, stroke: 1.75 });
  const ns = "http://www.w3.org/2000/svg";
  const body = document.createElementNS(ns, "rect");
  body.setAttribute("x", "6");
  body.setAttribute("y", "3");
  body.setAttribute("width", "12");
  body.setAttribute("height", "18");
  body.setAttribute("rx", "6");
  const button = document.createElementNS(ns, "path");
  button.setAttribute("d", "M12 3a6 6 0 0 1 6 6v1h-6z");
  button.setAttribute("fill", "currentColor");
  svg.prepend(body);
  svg.append(button);
  return svg;
}
