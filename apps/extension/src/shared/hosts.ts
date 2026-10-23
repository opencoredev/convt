// Site access is granted per host, as a match pattern.

export function hostOf(url: string): string | null {
  try {
    const parsed = new URL(url);
    return parsed.protocol === "http:" || parsed.protocol === "https:" ? parsed.hostname : null;
  } catch {
    return null;
  }
}

/** The match pattern that grants access to one host. */
export function hostPattern(url: string): string | null {
  const host = hostOf(url);
  return host === null ? null : `*://${host}/*`;
}
