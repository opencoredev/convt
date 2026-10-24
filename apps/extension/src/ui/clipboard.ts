// Writes a PNG to the clipboard. Works in the focused page (with the extension's
// clipboardWrite permission) and in convt's own pages.

import { fromBase64 } from "../shared/bytes.ts";
import type { CopyResult } from "../shared/messages.ts";

export async function copyPng(base64: string): Promise<CopyResult & { error?: string }> {
  try {
    const blob = new Blob([new Uint8Array(fromBase64(base64))], { type: "image/png" });
    await navigator.clipboard.write([new ClipboardItem({ "image/png": blob })]);
    return { ok: true };
  } catch (error) {
    return { ok: false, error: String(error) };
  }
}
