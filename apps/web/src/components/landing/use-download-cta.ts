import { useSyncExternalStore } from "react";

import { downloadCtaLabel, osFromUserAgent } from "#/lib/platform";

const subscribe = () => () => {};

/**
 * Landing Download CTAs. The prerendered HTML is the unknown-OS fallback
 * ("Download"); the browser then labels the visitor's system.
 */
export function useDownloadCtaLabel(): string {
  const os = useSyncExternalStore(
    subscribe,
    () => osFromUserAgent(navigator.userAgent),
    () => null,
  );
  return downloadCtaLabel(os);
}
