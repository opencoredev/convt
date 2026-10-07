import { useState } from "react";

import { cx, focusRing } from "#/components/app/ui";

/**
 * Copies a file's SHA-256. The full value sits in the title and the copied text,
 * so the page doesn't print 64 hex characters next to every download.
 */
export function CopySha({ value, className }: { value: string; className?: string }) {
  const [copied, setCopied] = useState(false);
  return (
    <button
      type="button"
      onClick={async () => {
        await navigator.clipboard.writeText(value);
        setCopied(true);
        setTimeout(() => setCopied(false), 1500);
      }}
      title={`SHA-256 ${value}`}
      aria-label={copied ? "Checksum copied" : "Copy SHA-256 checksum"}
      className={cx(
        "inline-flex shrink-0 cursor-pointer items-center gap-1 rounded-sm font-mono text-[11.5px]/4 text-ink-2 transition-colors hover:text-ink",
        focusRing,
        className,
      )}
    >
      {copied ? <CheckIcon /> : <CopyIcon />}
      {copied ? "Copied" : "SHA-256"}
    </button>
  );
}

function CopyIcon() {
  return (
    <svg width="12" height="12" viewBox="0 0 12 12" aria-hidden="true" className="shrink-0">
      <rect
        x="3.75"
        y="3.75"
        width="6.5"
        height="6.5"
        rx="1.5"
        fill="none"
        stroke="currentColor"
        strokeWidth="1.2"
      />
      <path
        d="M8.25 1.75h-5a1.5 1.5 0 0 0-1.5 1.5v5"
        fill="none"
        stroke="currentColor"
        strokeWidth="1.2"
        strokeLinecap="round"
      />
    </svg>
  );
}

function CheckIcon() {
  return (
    <svg
      width="12"
      height="12"
      viewBox="0 0 12 12"
      aria-hidden="true"
      className="shrink-0 text-green"
    >
      <path
        d="M2.5 6.25 5 8.75l4.5-5.5"
        fill="none"
        stroke="currentColor"
        strokeWidth="1.5"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}

export function DownloadIcon({ className }: { className?: string }) {
  return (
    <svg width="16" height="16" viewBox="0 0 16 16" aria-hidden="true" className={className}>
      <path
        d="M8 2.5v8m0 0L4.75 7.25M8 10.5l3.25-3.25M3 13.5h10"
        fill="none"
        stroke="currentColor"
        strokeWidth="1.5"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}
