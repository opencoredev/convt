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
    // Percent escapes stand for raw bytes (%89 in a PNG), not UTF-8 text, and base64
    // payloads may escape their own characters (%3D for =).
    if (/;base64$/i.test(meta)) {
      // Base64 is plain ASCII, so escapes like %3D can be undone as text.
      const text = payload.includes("%")
        ? payload.replace(/%([0-9a-f]{2})/gi, (_, hex: string) =>
            String.fromCharCode(Number.parseInt(hex, 16)),
          )
        : payload;
      return { mime, bytes: fromBase64(text.replace(/\s+/g, "")) };
    }
    return { mime, bytes: percentDecode(payload) };
  } catch {
    return null;
  }
}

/** Decodes %XX escapes to bytes; the text between them keeps its UTF-8 bytes. */
function percentDecode(text: string): Uint8Array {
  const encoder = new TextEncoder();
  if (!text.includes("%")) return encoder.encode(text);
  // UTF-8 needs at most 3 bytes per UTF-16 unit, and an escape shrinks 3 units to 1.
  const out = new Uint8Array(text.length * 3);
  let length = 0;
  let i = 0;
  while (i < text.length) {
    const hex = text.slice(i + 1, i + 3);
    if (text[i] === "%" && /^[0-9a-f]{2}$/i.test(hex)) {
      out[length++] = Number.parseInt(hex, 16);
      i += 3;
    } else {
      const next = text.indexOf("%", i + 1);
      const end = next === -1 ? text.length : next;
      length += encoder.encodeInto(text.slice(i, end), out.subarray(length)).written;
      i = end;
    }
  }
  return out.slice(0, length);
}
