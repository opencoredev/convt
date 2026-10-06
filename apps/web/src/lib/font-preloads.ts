import geistMonoUrl from "@fontsource-variable/geist-mono/files/geist-mono-latin-wght-normal.woff2?url";
import geistUrl from "@fontsource-variable/geist/files/geist-latin-wght-normal.woff2?url";

/**
 * Preloads for the two fonts in the first paint of the dark public pages (home and
 * /convert). Without them the fonts load only after the CSS, and text reflows when
 * they arrive (a measurable layout shift on phones).
 */
export const fontPreloads = [geistUrl, geistMonoUrl].map((href) => ({
  rel: "preload",
  as: "font",
  type: "font/woff2",
  href,
  crossOrigin: "anonymous" as const,
}));
