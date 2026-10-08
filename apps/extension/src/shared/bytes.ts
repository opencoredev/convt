// Chrome's extension messaging only carries JSON, so image bytes travel as base64.

const CHUNK = 0x8000;

export function toBase64(bytes: Uint8Array): string {
  let binary = "";
  for (let i = 0; i < bytes.length; i += CHUNK) {
    binary += String.fromCharCode(...bytes.subarray(i, i + CHUNK));
  }
  return btoa(binary);
}

export function fromBase64(base64: string): Uint8Array {
  const binary = atob(base64);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i);
  return bytes;
}

/** Decodes a `data:` URL. Null when it isn't one or its payload is malformed. */
export function parseDataUrl(url: string): { mime: string; bytes: Uint8Array } | null {
  const match = /^data:([^,]*?),(.*)$/s.exec(url);
  if (!match) return null;
  const meta = match[1] ?? "";
  const payload = match[2] ?? "";
  const mime = meta.split(";")[0] || "text/plain";
  try {
    if (/;base64$/i.test(meta)) return { mime, bytes: fromBase64(payload.replace(/\s+/g, "")) };
    return { mime, bytes: new TextEncoder().encode(decodeURIComponent(payload)) };
  } catch {
    return null;
  }
}
