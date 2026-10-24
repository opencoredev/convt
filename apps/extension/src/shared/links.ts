// Links to convt.app. The extension sends no analytics of its own; UTM parameters let
// the website attribute visits that start here.

export const SITE = "https://convt.app";

/** Where in the extension a link was clicked. Becomes `utm_content`. */
export type LinkSource =
  | "popup-footer"
  | "popup-header"
  | "welcome-desktop"
  | "welcome-header"
  | "toast-animated"
  | "toast-unreadable"
  | "toast-tip";

export function siteUrl(path: "/" | "/download" | "/formats" | "/privacy", source: LinkSource) {
  const url = new URL(path, SITE);
  url.searchParams.set("utm_source", "chrome-extension");
  url.searchParams.set("utm_medium", "extension");
  url.searchParams.set("utm_campaign", "image-converter");
  url.searchParams.set("utm_content", source);
  return url.toString();
}
