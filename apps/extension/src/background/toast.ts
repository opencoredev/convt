// Show progress in the page the user right-clicked. Pages Chrome protects (the Web
// Store, chrome:// pages, the PDF viewer) can't host the toast; there the toolbar
// badge and the popup report instead.

import type { ToastView } from "../shared/jobs.ts";
import type { ToContent } from "../shared/messages.ts";

export async function injectToast(tabId: number): Promise<boolean> {
  try {
    await chrome.scripting.executeScript({ target: { tabId, frameIds: [0] }, files: ["toast.js"] });
    return true;
  } catch {
    return false;
  }
}

/** Sends a toast update. Returns false when the page can't show it. */
export async function showToast(tabId: number, view: ToastView): Promise<boolean> {
  const message: ToContent = { kind: "toast", view };
  try {
    await chrome.tabs.sendMessage(tabId, message, { frameId: 0 });
    return true;
  } catch {
    // The worker restarted since injecting, or the page navigated: inject and retry once.
    if (!(await injectToast(tabId))) return false;
    try {
      await chrome.tabs.sendMessage(tabId, message, { frameId: 0 });
      return true;
    } catch {
      return false;
    }
  }
}

export async function flashBadge(tabId: number, outcome: "done" | "failed"): Promise<void> {
  const done = outcome === "done";
  try {
    await chrome.action.setBadgeBackgroundColor({ tabId, color: done ? "#127a47" : "#b3261e" });
    await chrome.action.setBadgeTextColor({ tabId, color: "#ffffff" });
    await chrome.action.setBadgeText({ tabId, text: done ? "✓" : "!" });
    setTimeout(() => void chrome.action.setBadgeText({ tabId, text: "" }).catch(() => {}), 6000);
  } catch {
    // The tab closed.
  }
}
