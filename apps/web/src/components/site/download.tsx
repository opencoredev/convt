import { PrimaryLink, SecondaryLink, cx } from "#/components/app/ui";
import { fileName, type ManifestArtifact } from "#/lib/release-manifest";

/** Shown where a build is not published yet, instead of a link. */
export function ComingSoon({ large }: { large?: boolean }) {
  return (
    <span
      className={cx(
        "inline-flex shrink-0 items-center justify-center rounded-lg bg-chip font-medium text-ink-2 shadow-[inset_0_0_0_1px_var(--chip-line)]",
        large ? "h-10 w-full text-sm/4.5" : "px-2.5 py-1.5 text-xs/4",
      )}
    >
      Shipping today
    </span>
  );
}

export function DownloadButton({
  artifact,
  large,
  label,
}: {
  artifact: ManifestArtifact | null;
  large?: boolean;
  label?: string;
}) {
  if (!artifact) return <ComingSoon large={large} />;
  const file = fileName(artifact.url);
  return large ? (
    <PrimaryLink href={artifact.url} download={file} className="h-10 w-full">
      {label ?? `Download ${file}`}
    </PrimaryLink>
  ) : (
    <SecondaryLink href={artifact.url} download={file} className="shrink-0">
      {label ?? "Download"}
    </SecondaryLink>
  );
}
