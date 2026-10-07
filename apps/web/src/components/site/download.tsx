import { useState } from "react";

import { cx, focusRing } from "#/components/app/ui";

/** A SHA-256 checksum with a button that copies all of it. `block` shows it in full, wrapped. */
export function Sha({ value, block }: { value: string; block?: boolean }) {
  const [copied, setCopied] = useState(false);
  const copy = (
    <button
      type="button"
      onClick={async () => {
        await navigator.clipboard.writeText(value);
        setCopied(true);
        setTimeout(() => setCopied(false), 1500);
      }}
      aria-label={copied ? "Checksum copied" : "Copy SHA-256 checksum"}
      className={cx(
        "shrink-0 cursor-pointer rounded-sm font-sans text-xs/4 font-medium text-[#157f4a] hover:underline hover:underline-offset-2 dark:text-green",
        focusRing,
      )}
    >
      {copied ? "Copied" : "Copy"}
    </button>
  );
  if (block)
    return (
      <div className="flex flex-col gap-1.5 rounded-lg bg-sunken px-3 py-2.5 shadow-[inset_0_0_0_1px_var(--line)]">
        <div className="flex items-center justify-between gap-3">
          <span className="font-mono text-[11px]/3.5 text-ink-2 uppercase">SHA-256</span>
          {copy}
        </div>
        <span className="font-mono text-[11.5px]/[17px] break-all text-ink-2">{value}</span>
      </div>
    );
  return (
    <div className="flex min-w-0 items-center gap-2 font-mono text-[11.5px]/4 text-ink-2">
      <span className="shrink-0">SHA-256</span>
      <span className="min-w-0 truncate" title={value}>
        {value}
      </span>
      {copy}
    </div>
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
