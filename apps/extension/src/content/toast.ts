// Injected into a page when the user converts an image there (activeTab: only after a
// right-click on that page, never on its own). Shows the toast and does Copy as PNG,
// which has to happen in the focused page.

import type { ToastAction } from "../shared/copy.ts";
import type { JobId } from "../shared/jobs.ts";
import { parseToContent, type CopyResult, type ToBackground } from "../shared/messages.ts";
import { copyPng } from "../ui/clipboard.ts";
import { createCard, type Card } from "../ui/toast-card.ts";
import styles from "../ui/toast.css" with { type: "text" };
import tokens from "../ui/tokens.css" with { type: "text" };

const MAX_VISIBLE = 3;
const EXIT_MS = 250;

type Entry = { card: Card; timer: number | null; lingerMs: number | null };

declare global {
  // The isolated world persists between injections into the same page, so the
  // second right-click reuses the first one's listener and stack.
  var __convtToast: true | undefined;
  /** End-to-end builds only: every toast state shown, in order. */
  var __convtLog: unknown[] | undefined;
}

if (!globalThis.__convtToast) {
  globalThis.__convtToast = true;
  start();
}

function start() {
  const entries = new Map<JobId, Entry>();
  let mount: { host: HTMLElement; stack: HTMLElement } | null = null;

  const ensureMount = () => {
    if (mount?.host.isConnected) return mount;
    const host = document.createElement("convt-toast");
    // Set through CSSOM: page CSP can't block it, and !important beats page rules.
    const pin: Record<string, string> = {
      all: "initial",
      position: "fixed",
      inset: "auto 16px 16px auto",
      margin: "0",
      padding: "0",
      border: "0",
      background: "transparent",
      overflow: "visible",
      width: "min(372px, calc(100vw - 32px))",
      height: "auto",
      "z-index": "2147483647",
      display: "block",
      "pointer-events": "none",
    };
    for (const [property, value] of Object.entries(pin)) {
      host.style.setProperty(property, value, "important");
    }
    const shadow = host.attachShadow({ mode: "closed" });
    const sheet = new CSSStyleSheet();
    sheet.replaceSync(`${tokens}\n${styles}`);
    shadow.adoptedStyleSheets = [sheet];
    const stack = document.createElement("div");
    stack.className = "stack";
    stack.setAttribute("role", "status");
    stack.setAttribute("aria-live", "polite");
    shadow.append(stack);
    // The popover top layer keeps the toast above modals and fullscreen video.
    host.setAttribute("popover", "manual");
    document.documentElement.append(host);
    mount = { host, stack };
    return mount;
  };

  const raise = (host: HTMLElement) => {
    if (typeof host.showPopover !== "function") return;
    try {
      if (host.matches(":popover-open")) host.hidePopover();
      host.showPopover();
    } catch {
      // Not in a document that allows popovers; the fixed position still works.
    }
  };

  const remove = (jobId: JobId) => {
    const entry = entries.get(jobId);
    if (!entry) return;
    entries.delete(jobId);
    if (entry.timer !== null) clearTimeout(entry.timer);
    const element = entry.card.element;
    element.dataset.open = "false";
    setTimeout(() => {
      element.remove();
      if (entries.size === 0 && mount?.host.matches(":popover-open")) mount.host.hidePopover();
    }, EXIT_MS);
  };

  const schedule = (jobId: JobId, entry: Entry) => {
    if (entry.timer !== null) clearTimeout(entry.timer);
    entry.timer =
      entry.lingerMs === null ? null : window.setTimeout(() => remove(jobId), entry.lingerMs);
  };

  const send = (message: ToBackground) => {
    void chrome.runtime.sendMessage(message).catch(() => {
      // The extension was reloaded or removed while the toast was up.
    });
  };

  const onAction = (jobId: JobId, action: ToastAction) => {
    switch (action.kind) {
      case "show-download":
        send({ kind: "show-download", downloadId: action.downloadId });
        remove(jobId);
        return;
      case "open-access":
        send({ kind: "open-access", jobId });
        return;
      case "link":
        return;
      default: {
        const _exhaustive: never = action;
        return _exhaustive;
      }
    }
  };

  chrome.runtime.onMessage.addListener((raw: unknown, _sender, sendResponse) => {
    const message = parseToContent(raw);
    if (message === null) return false;

    if (message.kind === "copy-image") {
      void copyPng(message.png).then(({ ok, error }) => {
        if (__E2E__ && error) (globalThis.__convtLog ??= []).push({ copyError: error });
        const result: CopyResult = { ok };
        sendResponse(result);
      });
      return true;
    }

    const view = message.view;
    if (view.phase === "dismissed") {
      remove(view.jobId);
      return false;
    }

    const { host, stack } = ensureMount();
    let entry = entries.get(view.jobId);
    if (!entry) {
      const jobId = view.jobId;
      const card = createCard({
        srcUrl: view.thumb,
        inline: false,
        handlers: { onAction: (action) => onAction(jobId, action), onClose: () => remove(jobId) },
      });
      const created: Entry = { card, timer: null, lingerMs: null };
      entry = created;
      const element = card.element;
      // Hovering or focusing a toast holds it; leaving restarts its clock.
      element.addEventListener("pointerenter", () => {
        if (created.timer !== null) clearTimeout(created.timer);
        created.timer = null;
      });
      element.addEventListener("pointerleave", () => schedule(jobId, created));
      element.addEventListener("focusin", () => {
        if (created.timer !== null) clearTimeout(created.timer);
        created.timer = null;
      });
      element.addEventListener("focusout", (event) => {
        if (!element.contains(event.relatedTarget instanceof Node ? event.relatedTarget : null)) {
          schedule(jobId, created);
        }
      });
      element.addEventListener("keydown", (event) => {
        if (event.key === "Escape") remove(jobId);
      });
      entries.set(jobId, created);
      stack.append(element);
      raise(host);
      // Next frame, so the closed state paints first and the entrance animates.
      requestAnimationFrame(() => requestAnimationFrame(() => (element.dataset.open = "true")));
      const overflow = [...entries.keys()].slice(0, Math.max(0, entries.size - MAX_VISIBLE));
      for (const old of overflow) remove(old);
    }
    const copy = entry.card.update(view);
    if (__E2E__) {
      // The end-to-end test reads this from the same isolated world.
      (globalThis.__convtLog ??= []).push({ jobId: view.jobId, phase: view.phase, ...copy });
    }
    entry.lingerMs = copy.lingerMs;
    if (!entry.card.element.matches(":hover, :focus-within")) schedule(view.jobId, entry);
    return false;
  });
}
