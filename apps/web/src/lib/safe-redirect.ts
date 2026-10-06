// The only way a `redirect` parameter becomes a navigation. Anything that is not a
// path on this site becomes /dashboard.

export const defaultRedirect = "/dashboard";

/** A backslash or a control character, which browsers may read as part of a host. */
function hasUnsafe(text: string): boolean {
  for (const ch of text) {
    const code = ch.charCodeAt(0);
    if (ch === "\\" || code < 0x20 || code === 0x7f) return true;
  }
  return false;
}

function decodeOnce(value: string): string | null {
  try {
    return decodeURIComponent(value);
  } catch {
    return null;
  }
}

export function safeRedirect(value: unknown, origin: string): string {
  if (typeof value !== "string" || value === "") return defaultRedirect;
  const decoded = decodeOnce(value);
  if (decoded === null || hasUnsafe(value) || hasUnsafe(decoded)) return defaultRedirect;
  // A path must start with exactly one slash; "//host" and encoded variants are hosts.
  if (!value.startsWith("/") || value.startsWith("//") || decoded.startsWith("//"))
    return defaultRedirect;
  let url: URL;
  try {
    url = new URL(value, origin);
  } catch {
    return defaultRedirect;
  }
  if (url.origin !== new URL(origin).origin) return defaultRedirect;
  const result = url.pathname + url.search + url.hash;
  // Dot segments can normalize "/x/..//evil.example" into "//evil.example", which a
  // browser reads as another host. Check the result itself, not only the input.
  if (!result.startsWith("/") || result.startsWith("//") || hasUnsafe(result))
    return defaultRedirect;
  const again = new URL(result, origin);
  if (again.origin !== url.origin || again.pathname + again.search + again.hash !== result)
    return defaultRedirect;
  return result;
}
