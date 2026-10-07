// Who gets the "convt runs on your computer" card instead of a download: a phone or
// tablet operating system AND a narrow viewport. A desktop browser in a narrow window
// keeps the normal download.

/** Widest viewport that still counts as mobile: phones, and tablets in portrait. */
export const mobileMaxWidth = 1024;

/**
 * The viewport half of the check in CSS, for server-rendered markup: `show` and `hide`
 * classes, and the same for a Mac user agent that also needs a coarse pointer (iPadOS).
 * Written out in full so Tailwind finds them; `max-[1024px]:` would stop at 1023px.
 */
export const mobileCss = {
  any: {
    show: "hidden [@media(max-width:1024px)]:block",
    hide: "[@media(max-width:1024px)]:hidden",
  },
  touch: {
    show: "hidden [@media(max-width:1024px)_and_(pointer:coarse)]:block",
    hide: "[@media(max-width:1024px)_and_(pointer:coarse)]:hidden",
  },
} as const;

/**
 * True for iOS, iPadOS and Android user agents. iPadOS Safari reports a desktop Mac
 * user agent, so a Mac with more than one touch point counts too; the server, which
 * has no touch information, only sees the user agent.
 */
export function isMobileUserAgent(ua: string, maxTouchPoints = 0): boolean {
  if (/iPhone|iPad|iPod|Android/i.test(ua)) return true;
  return /Macintosh/i.test(ua) && maxTouchPoints > 1;
}

/**
 * What a user agent alone says, for the server's first render. "maybe" is a Mac user
 * agent, which iPadOS Safari sends too; the page lets CSS settle it with a coarse
 * pointer at a mobile width, so a real Mac keeps the normal download.
 */
export type UserAgentMobile = "yes" | "maybe" | "no";

export function mobileFromUserAgent(ua: string): UserAgentMobile {
  if (isMobileUserAgent(ua)) return "yes";
  return /Macintosh/i.test(ua) ? "maybe" : "no";
}

export type DeviceSignals = { userAgent: string; maxTouchPoints: number; viewportWidth: number };

export function isMobileDevice(d: DeviceSignals): boolean {
  return isMobileUserAgent(d.userAgent, d.maxTouchPoints) && d.viewportWidth <= mobileMaxWidth;
}

/** The browser's answer. False on the server. */
export function isMobileBrowser(): boolean {
  if (typeof window === "undefined") return false;
  return isMobileDevice({
    userAgent: navigator.userAgent,
    maxTouchPoints: navigator.maxTouchPoints ?? 0,
    viewportWidth: window.innerWidth,
  });
}

/** Where a capture started; sent with the request and the analytics event. */
export const captureSources = ["landing", "download", "checkout_success"] as const;
export type CaptureSource = (typeof captureSources)[number];

export function isCaptureSource(value: unknown): value is CaptureSource {
  return captureSources.some((s) => s === value);
}

const maxEmailLength = 254;
const emailShape = /^[^\s@<>()[\]\\,;:"]{1,64}@[^\s@<>()[\]\\,;:"]+\.[^\s@<>()[\]\\,;:".]{2,}$/;

/** The address to send to, lowercased and trimmed, or null when it can't be one. */
export function normalizeEmail(value: unknown): string | null {
  if (typeof value !== "string") return null;
  const email = value.trim().toLowerCase();
  if (email.length > maxEmailLength || !emailShape.test(email)) return null;
  const domain = email.slice(email.indexOf("@") + 1);
  if (domain.startsWith(".") || domain.includes("..")) return null;
  return email;
}
