import { downloadCtaLabel, downloadHref, downloadStepLabel, type Os } from "./platform";

/** The checkout success page's "what's next" panel, naming the visitor's OS when known. */
export function checkoutAside(kind: "key" | "trial", os: Os | null) {
  return {
    title: "WHAT'S NEXT",
    items:
      kind === "trial"
        ? [
            downloadStepLabel(os),
            "Open the app and start converting",
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
