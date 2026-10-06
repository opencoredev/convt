import { useState } from "react";

import { getLicenseKey } from "#/lib/account";
import { openActivationLink } from "#/lib/activate";
import type { License } from "#/lib/types";

import { useNotice } from "./notice";
import { TextButton } from "./ui";

// The full key is fetched only when one of these is pressed, so it is never in the
// page HTML or the loader data.

export function CopyKeyButton({ license }: { license: License }) {
  const notice = useNotice();
  const [busy, setBusy] = useState(false);
  if (license.revoked) return null;
  return (
    <TextButton
      disabled={busy}
      aria-label={`Copy ${license.product} license key`}
      onClick={async () => {
        setBusy(true);
        try {
          await navigator.clipboard.writeText(await getLicenseKey(license.id));
          notice("License key copied.");
        } catch {
          notice("Couldn't copy the key. Try again.");
        } finally {
          setBusy(false);
        }
      }}
    >
      Copy key
    </TextButton>
  );
}

/**
 * Opens the desktop app with the key filled in (convt://activate). The app asks
 * the user to confirm. Without the app installed, nothing happens.
 */
export function ActivateButton({ license }: { license: License }) {
  const notice = useNotice();
  const [busy, setBusy] = useState(false);
  if (license.revoked) return null;
  return (
    <TextButton
      disabled={busy}
      aria-label={`Activate ${license.product} license on this computer`}
      onClick={async () => {
        setBusy(true);
        try {
          openActivationLink(await getLicenseKey(license.id));
        } catch {
          notice("Couldn't open the key. Try again.");
        } finally {
          setBusy(false);
        }
      }}
    >
      Activate on this computer
    </TextButton>
  );
}
