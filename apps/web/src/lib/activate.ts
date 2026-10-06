// The desktop app's activation link. The app parses convt://activate?key=... and
// asks the user to confirm (crates/convt-app/src/request.rs).

export function activationUrl(token: string): string {
  return `convt://activate?key=${encodeURIComponent(token)}`;
}

/**
 * Opens the link the way a clicked link would, which hands it to the registered
 * protocol handler. A temporary anchor rather than location.assign, because the
 * e2e check can intercept an anchor click but no page script can stub Location.
 */
export function openActivationLink(token: string) {
  const a = document.createElement("a");
  a.href = activationUrl(token);
  a.rel = "noreferrer";
  a.click();
}

/** Opens any convt:// link the same way, such as desktop sign-in's convt://auth reply. */
export function openAppLink(href: string) {
  if (!href.startsWith("convt://")) return;
  const a = document.createElement("a");
  a.href = href;
  a.rel = "noreferrer";
  a.click();
}
