import { useState } from "react";

import { cx, focusRing, PrimaryLink, SecondaryLink } from "#/components/app/ui";
import { fileName, type ManifestArtifact } from "#/lib/release-manifest";

/** Shown on a download button when that build is not published yet. */
export function ComingSoon({ large }: { large?: boolean }) {
  return (
    <span
      className={cx(
        "inline-flex shrink-0 items-center justify-center rounded-lg bg-chip font-medium text-ink-2 shadow-[inset_0_0_0_1px_var(--chip-line)]",
        large ? "h-10 w-full text-sm/4.5" : "px-2.5 py-1.5 text-xs/4",
      )}
    >
      Coming soon
    </span>
  );
}

export function DownloadButton({
  artifact,
  large,
}: {
  artifact: ManifestArtifact | null;
  large?: boolean;
}) {
  if (!artifact) return <ComingSoon large={large} />;
  const file = fileName(artifact.url);
  return large ? (
    <PrimaryLink href={artifact.url} download={file} className="h-10 w-full">
      Download {file}
    </PrimaryLink>
  ) : (
    <SecondaryLink href={artifact.url} download={file} className="shrink-0">
      Download
    </SecondaryLink>
  );
}

/** A SHA-256 checksum, shortened on screen, with a button that copies all of it. */
export function Sha({ value }: { value: string }) {
  const [copied, setCopied] = useState(false);
  return (
    <div className="flex min-w-0 items-center gap-2 font-mono text-[11.5px]/4 text-ink-2">
      <span className="shrink-0">SHA-256</span>
      <span className="min-w-0 truncate text-ink-2" title={value}>
        {value}
      </span>
      <button
        type="button"
        onClick={async () => {
          await navigator.clipboard.writeText(value);
          setCopied(true);
          setTimeout(() => setCopied(false), 1500);
        }}
        className={cx(
          "shrink-0 cursor-pointer rounded-sm font-sans text-xs/4 font-medium text-[#157f4a] dark:text-green hover:underline",
          focusRing,
        )}
      >
        {copied ? "Copied" : "Copy"}
      </button>
    </div>
  );
}
