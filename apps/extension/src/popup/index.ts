// The toolbar popup: recent conversions, settings, site access, and the desktop app.

import { targetInfo } from "../shared/formats.ts";
import type { RecentItem, ToastView } from "../shared/jobs.ts";
import { siteUrl } from "../shared/links.ts";
import type { ToBackground } from "../shared/messages.ts";
import { formatBytes } from "../shared/naming.ts";
import { QUALITIES, qualityInfo, type Settings } from "../shared/settings.ts";
import {
  getLastFailure,
  getRecent,
  getSettings,
  saveSettings,
  setLastFailure,
} from "../shared/store.ts";
import { relativeTime } from "../shared/time.ts";
import { h, icon, icons, mark } from "../ui/dom.ts";
import { menuDemo } from "../ui/menu-demo.ts";
import { createCard } from "../ui/toast-card.ts";

const ALL_SITES = { origins: ["<all_urls>"] };
/** A failure on a page without a toast stays in the popup this long. */
const FAILURE_SHOWN_MS = 2 * 60 * 1000;

/** The popup shows the newest few; storage keeps a few more. */
const SHOWN_RECENT = 5;

type View = "home" | "settings";

const app = document.getElementById("app");
if (app) void render(app, "home");

async function render(root: HTMLElement, view: View) {
  const show = (next: View) => void render(root, next);
  if (view === "settings") {
    const [settings, allSites] = await Promise.all([
      getSettings(),
      chrome.permissions.contains(ALL_SITES),
    ]);
    root.replaceChildren(settingsHeader(show), settingsSection(settings, allSites));
    return;
  }

  const [recent, failure] = await Promise.all([getRecent(), getLastFailure()]);
  const shown = recent.slice(0, SHOWN_RECENT);
  const existing = await downloadsThatExist(shown);
  const status =
    failure && Date.now() - failure.at < FAILURE_SHOWN_MS ? failureCard(failure.view) : null;

  root.replaceChildren(
    ...[
      homeHeader(show),
      status ? h("div", { class: "status" }, [status]) : null,
      recentSection(shown, existing, () => show("home")),
      desktopCard(),
    ].filter((node) => node !== null),
  );
}

function iconButton(label: string, paths: readonly string[], onClick: () => void): HTMLElement {
  const button = h(
    "button",
    { class: "icon-button", type: "button", "aria-label": label, title: label },
    [icon(paths, { size: 18, stroke: 1.5 })],
  );
  button.addEventListener("click", onClick);
  return button;
}

function homeHeader(show: (view: View) => void): HTMLElement {
  return h("header", { class: "header" }, [
    h(
      "a",
      { class: "brand", href: siteUrl("/", "popup-header"), target: "_blank", rel: "noopener" },
      [mark(20), "convt"],
    ),
    h("div", { class: "header-actions" }, [
      h(
        "a",
        { class: "header-link", href: chrome.runtime.getURL("welcome.html"), target: "_blank" },
        ["How it works"],
      ),
      iconButton("Settings", icons.settings, () => show("settings")),
    ]),
  ]);
}

function settingsHeader(show: (view: View) => void): HTMLElement {
  return h("header", { class: "header header-sub" }, [
    iconButton("Back", icons.back, () => show("home")),
    h("h1", { class: "header-title" }, ["Settings"]),
  ]);
}

function failureCard(view: Extract<ToastView, { phase: "failed" }>): HTMLElement {
  const card = createCard({
    srcUrl: view.thumb,
    inline: true,
    handlers: {
      onAction: (action) => {
        if (action.kind === "open-access") {
          const message: ToBackground = { kind: "open-access", jobId: view.jobId };
          void chrome.runtime.sendMessage(message);
          window.close();
        }
      },
      onClose: () => {
        void setLastFailure(null);
        card.element.remove();
      },
    },
  });
  card.update(view);
  card.element.dataset.open = "true";
  return card.element;
}

/** Ids of downloads whose files are still on disk. */
async function downloadsThatExist(recent: RecentItem[]): Promise<Set<number>> {
  const ids = recent.flatMap((item) => (item.kind === "saved" ? [item.downloadId] : []));
  const found = await Promise.all(
    ids.map(async (id) => {
      const [item] = await chrome.downloads.search({ id });
      return item?.exists && item.state === "complete" ? id : null;
    }),
  );
  return new Set(found.filter((id) => id !== null));
}

function recentSection(
  recent: RecentItem[],
  existing: Set<number>,
  refresh: () => void,
): HTMLElement {
  const clear = h("button", { class: "btn btn-quiet btn-small", type: "button" }, ["Clear"]);
  clear.addEventListener("click", async () => {
    const message: ToBackground = { kind: "clear-recent" };
    await chrome.runtime.sendMessage(message);
    refresh();
  });

  if (recent.length === 0) {
    return h("section", { class: "section", "aria-labelledby": "recent-title" }, [
      h("div", { class: "section-head" }, [
        h("h2", { class: "section-title", id: "recent-title" }, ["Recent"]),
      ]),
      h("div", { class: "empty" }, [
        menuDemo({ compact: true }),
        h("p", {}, ["Right-click any image on a page, then choose Convert with convt."]),
      ]),
    ]);
  }

  const now = Date.now();
  return h("section", { class: "section", "aria-labelledby": "recent-title" }, [
    h("div", { class: "section-head" }, [
      h("h2", { class: "section-title", id: "recent-title" }, ["Recent"]),
      clear,
    ]),
    h(
      "ul",
      { class: "recent" },
      recent.map((item) => h("li", {}, [recentRow(item, existing, now)])),
    ),
  ]);
}

function recentRow(item: RecentItem, existing: Set<number>, now: number): HTMLElement {
  const when = relativeTime(item.at, now);
  const thumb = h("img", { class: "recent-thumb", src: item.thumb, alt: "" });

  if (item.kind === "copied") {
    return h("div", { class: "recent-row", "aria-disabled": "true" }, [
      thumb,
      h("div", {}, [
        h("div", { class: "recent-name" }, ["Copied to clipboard"]),
        h("div", { class: "recent-meta mono" }, [`PNG · ${formatBytes(item.bytes)} · ${when}`]),
      ]),
    ]);
  }

  const onDisk = existing.has(item.downloadId);
  const meta = onDisk
    ? `${targetInfo[item.target].label} · ${formatBytes(item.bytes)} · ${when}`
    : `Moved or deleted · ${when}`;
  const row = h(
    "button",
    {
      class: "recent-row",
      type: "button",
      title: onDisk ? `Show ${item.name} in folder` : undefined,
      "aria-disabled": onDisk ? undefined : "true",
    },
    [
      thumb,
      h("div", {}, [
        h("div", { class: "recent-name" }, [item.name]),
        h("div", { class: "recent-meta mono", "data-missing": onDisk ? undefined : "true" }, [
          meta,
        ]),
      ]),
      onDisk
        ? h("span", { class: "recent-hint" }, [icon(icons.folder, { size: 16, stroke: 1.5 })])
        : null,
    ],
  );
  if (onDisk) {
    row.addEventListener("click", () => {
      const message: ToBackground = { kind: "show-download", downloadId: item.downloadId };
      void chrome.runtime.sendMessage(message);
    });
  }
  return row;
}

function settingsSection(settings: Settings, allSites: boolean): HTMLElement {
  let current = settings;
  const update = (next: Partial<Settings>) => {
    current = { ...current, ...next };
    void saveSettings(current);
  };

  const quality = h(
    "div",
    { class: "segmented", role: "radiogroup", "aria-label": "Quality" },
    QUALITIES.flatMap((q) => {
      const id = `quality-${q}`;
      const input = h("input", {
        type: "radio",
        name: "quality",
        id,
        value: q,
        checked: settings.quality === q,
      });
      input.addEventListener("change", () => update({ quality: q }));
      return [input, h("label", { for: id }, [qualityInfo[q].label])];
    }),
  );

  const ask = h("input", {
    type: "checkbox",
    class: "switch",
    id: "ask",
    role: "switch",
    checked: settings.askWhereToSave,
  });
  ask.addEventListener("change", () => update({ askWhereToSave: ask.checked }));

  const access = h("input", {
    type: "checkbox",
    class: "switch",
    id: "access",
    role: "switch",
    checked: allSites,
  });
  access.addEventListener("change", async () => {
    // Requesting needs this click; Chrome shows its own confirmation.
    const granted = access.checked
      ? await chrome.permissions.request(ALL_SITES)
      : !(await chrome.permissions.remove(ALL_SITES));
    access.checked = granted;
  });

  return h("section", { class: "section section-settings" }, [
    h("div", { class: "setting" }, [
      h("div", { class: "setting-text" }, [
        h("span", { class: "setting-label" }, ["Quality"]),
        h("span", { class: "setting-help" }, ["For JPG and WebP. PNG is lossless."]),
      ]),
      quality,
    ]),
    h("div", { class: "setting" }, [
      h("label", { class: "setting-text", for: "ask" }, [
        h("span", { class: "setting-label" }, ["Ask where to save"]),
        h("span", { class: "setting-help" }, ["Otherwise files go to Downloads."]),
      ]),
      ask,
    ]),
    h("div", { class: "setting" }, [
      h("label", { class: "setting-text", for: "access" }, [
        h("span", { class: "setting-label" }, ["Read images on all sites"]),
        h("span", { class: "setting-help" }, ["Off: convt asks when a site needs it."]),
      ]),
      access,
    ]),
  ]);
}

function desktopCard(): HTMLElement {
  return h("aside", { class: "desktop" }, [
    h("div", {}, [
      h("p", { class: "desktop-title" }, ["Video, audio and PDFs too"]),
      h("p", { class: "desktop-text" }, [
        "convt for desktop converts any file from your computer's right-click menu.",
      ]),
    ]),
    h(
      "a",
      {
        class: "btn btn-small",
        href: siteUrl("/download", "popup-footer"),
        target: "_blank",
        rel: "noopener",
      },
      ["Get the app"],
    ),
  ]);
}
