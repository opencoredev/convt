import { useState } from "react";

import { SecondaryButton } from "./ui";

/** Copies `value` in the click handler. The button only shows Copy / Copied. */
export function CopyButton({
  value,
  label = "Copy",
  copiedLabel = "Copied",
  "aria-label": ariaLabel,
}: {
  value: string;
  label?: string;
  copiedLabel?: string;
  "aria-label"?: string;
}) {
  const [copied, setCopied] = useState(false);
  return (
    <SecondaryButton
      aria-label={ariaLabel ?? label}
      onClick={async () => {
        try {
          await navigator.clipboard.writeText(value);
          setCopied(true);
          setTimeout(() => setCopied(false), 1500);
        } catch {
          setCopied(false);
        }
      }}
    >
      <span aria-live="polite">{copied ? copiedLabel : label}</span>
    </SecondaryButton>
  );
}
