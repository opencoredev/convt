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
    const bytes = percentDecode(payload);
    if (/;base64$/i.test(meta)) {
      let text = "";
      for (const byte of bytes) text += String.fromCharCode(byte);
      return { mime, bytes: fromBase64(text.replace(/\s+/g, "")) };
    }
    return { mime, bytes };
  } catch {
    return null;
  }
}

/** Decodes %XX escapes to bytes; other characters keep their UTF-8 bytes. */
function percentDecode(text: string): Uint8Array {
  const out: number[] = [];
  const encoder = new TextEncoder();
  for (let i = 0; i < text.length; i++) {
    const char = text[i] ?? "";
    const hex = text.slice(i + 1, i + 3);
    if (char === "%" && /^[0-9a-f]{2}$/i.test(hex)) {
      out.push(Number.parseInt(hex, 16));
      i += 2;
    } else {
      out.push(...encoder.encode(char));
    }
  }
  return new Uint8Array(out);
}
