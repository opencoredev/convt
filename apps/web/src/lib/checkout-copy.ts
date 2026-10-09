import { downloadCtaLabel, downloadHref, downloadStepLabel, type Os } from "./platform";

/**
 * Where the checkout was opened. The desktop app's Start free trial opens
 * /checkout/pro?from=app, and convt-billing carries `from=app` to the success page.
 */
export type CheckoutOrigin = "app" | "web";

export function checkoutOrigin(from: unknown): CheckoutOrigin {
  return from === "app" ? "app" : "web";
}

/**
 * The checkout success page's "what's next" panel. From the app the buyer already has
 * convt open, so it sends them back there; from the web it starts with the download.
 */
export function checkoutAside({
  kind,
  os,
  origin,
}: {
  kind: "key" | "trial";
  os: Os | null;
  origin: CheckoutOrigin;
}) {
  if (origin === "app")
    return {
      title: "WHAT'S NEXT",
      items:
        kind === "trial"
          ? [
              "Switch back to convt",
              "It shows your trial within a few seconds",
              "Cancel any time from Billing before the first charge",
            ]
          : [
              "Switch back to convt",
              "It picks up your Pro key by itself",
              "Find the key again under Licenses",
            ],
    };
  return {
    title: "WHAT'S NEXT",
    items:
      kind === "trial"
        ? [
            downloadStepLabel(os),
            "Open it and sign in with this account",
            "Cancel any time from Billing before the first charge",
          ]
        : [
            downloadStepLabel(os),
            "Open the key in the app, or paste it in Settings",
            "Find it again under Licenses",
          ],
  };
}

/** The download button: the visitor's build when the OS is known, otherwise the picker. */
export function downloadAction(os: Os | null) {
  return { href: downloadHref(os), label: downloadCtaLabel(os) };
}
