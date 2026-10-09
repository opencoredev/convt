import {
  ArrowDown01Icon,
  Copy01Icon,
  Download01Icon,
  Tick02Icon,
} from "@hugeicons/core-free-icons";
import { useState, type CSSProperties, type ReactNode } from "react";

import { CommandBlock, homebrewCommands } from "#/components/app/command-block";
import { DitherGlow } from "#/components/app/dither-glow";
import { cx, focusRing } from "#/components/app/ui";
import { Icon } from "#/components/icon";
import { Mark } from "#/components/logo";
import { kindLabels, osNames, osOrder, type Os, type Release, type Slot } from "#/lib/platform";
import { fileName, formatBytes } from "#/lib/release-manifest";
import { GITHUB_URL } from "#/lib/site";

/*
 * /download. Direction, from the Mobbin references (download-refs/):
 * - Reference: Linear's and Raycast's download pages: the app icon, one big button for
 *   the visitor's system with a quiet line under it, and every other build as a plain
 *   list. The page answers one question, which file do I want, and stops.
 * - Palette: the shared tokens, the green .btn-primary for the one primary action, and
 *   the dithered glow from sign-in rising behind it.
 * - Type: Inter for everything people read; Geist Mono only for commands and checksums.
 * - Layout: a centered stage for the recommended build, then Homebrew for Mac visitors,
 *   then every platform as rows in a narrow column, then checksums folded away.
 * - Avoiding: eyebrow labels, numbered steps, terminal chrome, raw file names on buttons.
 */

/** "Apple silicon", "64-bit", "x86_64": what a person checks against their computer. */
function archLabel(slot: Slot) {
  if (slot.os === "macos") return "Apple silicon";
  if (slot.os === "windows") return "64-bit";
  return slot.arch;
}

/** The line under the primary button: "Apple silicon · v0.3.0 · 50.0 MB". */
export function primaryMeta(slot: Slot, version: string | null) {
  const parts = [
    slot.os === "linux" ? kindLabels[slot.kind].title : null,
    archLabel(slot),
    version ? `v${version}` : null,
    slot.artifact ? formatBytes(slot.artifact.size) : null,
  ];
  return parts.filter(Boolean).join(" · ");
}

/** Children arrive in order, 60 ms apart (styles.css .auth-rise; reduced motion: at once). */
function Rise({
  index,
  className,
  children,
}: {
  index: number;
  className?: string;
  children: ReactNode;
}) {
  return (
    <div
      className={cx("auth-rise", className)}
      style={{ "--d": `${60 + index * 60}ms` } as CSSProperties}
    >
      {children}
    </div>
  );
}

/** The recommended build for the visitor's system, centered over the glow. */
export function DownloadStage({ os, release }: { os: Os | null; release: Release }) {
  const slot = os ? release.slots.find((s) => s.os === os) : undefined;
  // Linux has more than one package; the AppImage leads and the rest are links under it.
  const extras = release.slots.flatMap((s) =>
    s.os === os && s !== slot && s.artifact ? [{ slot: s, artifact: s.artifact }] : [],
  );
  return (
    <section aria-labelledby="download-title" className="relative isolate overflow-hidden">
      <DitherGlow className="absolute inset-x-0 bottom-0 -z-10 h-[70%] w-full [mask-image:linear-gradient(to_bottom,transparent,black_45%,black_70%,transparent)]" />
      <div className="mx-auto flex max-w-[640px] flex-col items-center px-5 pt-16 pb-20 text-center md:pt-24 md:pb-28">
        <Rise index={0}>
          <span className="flex size-[72px] items-center justify-center rounded-[20px] bg-raised shadow-[0_0_0_1px_var(--line),0_1px_2px_rgb(0_0_0/6%),0_12px_32px_rgb(10_60_35/14%)] dark:shadow-[0_0_0_1px_#2e3331,0_12px_32px_rgb(0_0_0/50%)]">
            <Mark size={38} />
          </span>
        </Rise>
        <Rise index={1} className="mt-8 flex flex-col items-center gap-4">
          <h1
            id="download-title"
            className="text-[40px]/[44px] font-semibold tracking-[-0.035em] text-balance md:text-[52px]/[56px]"
          >
            Download convt
          </h1>
          <p className="max-w-[440px] text-[17px]/[26px] text-pretty text-ink-2">
            One install gives you the app, the{" "}
            <span className="whitespace-nowrap">right-click</span> menu and the{" "}
            <span className="font-medium text-ink">convt</span> command.
          </p>
        </Rise>
        <Rise index={2} className="mt-10 flex w-full flex-col items-center gap-3.5">
          {slot ? (
            <>
              <PrimaryDownload slot={slot} />
              <p className="text-[13px]/5 text-ink-2 tabular-nums">
                {primaryMeta(slot, release.version)}
              </p>
              {extras.length > 0 && (
                <p className="-mt-1.5 text-[13px]/5 text-ink-2">
                  Also as{" "}
                  {extras.map(({ slot: s, artifact }, i, list) => (
                    <span key={s.kind}>
                      <a
                        href={artifact.url}
                        download={fileName(artifact.url)}
                        className={cx(
                          "rounded-sm font-medium text-ink underline decoration-line-strong underline-offset-3 transition-colors duration-150 hover:decoration-ink",
                          focusRing,
                        )}
                      >
                        {shortKind(s)}
                      </a>
                      {i < list.length - 2 ? ", " : i === list.length - 2 ? " or " : ""}
                    </span>
                  ))}
                </p>
              )}
            </>
          ) : (
            <NoDesktop />
          )}
        </Rise>
      </div>
    </section>
  );
}

function shortKind(slot: Slot) {
  return slot.kind === "AppImage" ? "AppImage" : `.${slot.kind}`;
}

/** The one big button. Never shows the file name; the meta line under it says what it is. */
function PrimaryDownload({ slot }: { slot: Slot }) {
  const label = `Download for ${osNames[slot.os]}`;
  if (!slot.artifact)
    return (
      <span className="inline-flex h-14 items-center justify-center rounded-full bg-chip px-8 text-[16px]/5 font-medium text-ink-2 shadow-[inset_0_0_0_1px_var(--chip-line)]">
        Shipping today
      </span>
    );
  return (
    <a
      href={slot.artifact.url}
      download={fileName(slot.artifact.url)}
      className={cx(
        "btn-primary inline-flex h-14 w-full max-w-[320px] items-center justify-center gap-2.5 rounded-full px-8 text-[17px]/5 font-semibold tracking-[-0.01em] transition-[filter,scale] duration-150 ease-out active:scale-[0.98] motion-reduce:active:scale-100 sm:w-auto",
        focusRing,
      )}
    >
      <Icon icon={Download01Icon} size={20} strokeWidth={2} />
      {label}
    </a>
  );
}

// Phones and unknown systems: convt is desktop software, so say so and list them all.
function NoDesktop() {
  return (
    <div className="flex flex-col items-center gap-4">
      <p className="max-w-[420px] text-[15px]/6 text-pretty text-ink-2">
        convt runs on macOS, Windows and Linux. Open this page on your computer, or pick its system
        below.
      </p>
      <a
        href="#platforms"
        className={cx(
          "btn-primary inline-flex h-12 items-center gap-2 rounded-full px-6 text-[15px]/5 font-semibold",
          focusRing,
        )}
      >
        See all platforms
        <Icon icon={ArrowDown01Icon} size={16} strokeWidth={2} />
      </a>
    </div>
  );
}

/** The Homebrew cask, for Mac visitors who'd rather use the terminal. */
export function Homebrew() {
  return (
    <section aria-labelledby="homebrew-title" className="flex flex-col gap-4">
      <div className="flex flex-col gap-1">
        <h2 id="homebrew-title" className="text-[17px]/6 font-semibold tracking-[-0.015em]">
          Install with Homebrew
        </h2>
        <p className="text-[14px]/[22px] text-ink-2">
          The same app, installed and updated from the terminal.
        </p>
      </div>
      <CommandBlock label="Homebrew" commands={homebrewCommands} />
    </section>
  );
}

/** Every build, one group per system and a row per file. */
export function Platforms({ os, release }: { os: Os | null; release: Release }) {
  return (
    <section
      aria-labelledby="platforms-title"
      id="platforms"
      className="flex scroll-mt-6 flex-col gap-4"
    >
      <div className="flex flex-wrap items-baseline justify-between gap-x-6 gap-y-1">
        <h2 id="platforms-title" className="text-[17px]/6 font-semibold tracking-[-0.015em]">
          All platforms
        </h2>
        <a
          href={`${GITHUB_URL}/releases`}
          className={cx(
            "rounded-sm text-[13px]/5 text-ink-2 transition-colors duration-150 hover:text-ink",
            focusRing,
          )}
        >
          Older versions on GitHub
        </a>
      </div>
      <ul className="flex flex-col rounded-2xl bg-raised shadow-[0_0_0_1px_var(--line),0_1px_2px_rgb(0_0_0/4%)] dark:bg-panel">
        {osOrder.map((platform) => (
          <PlatformGroup
            key={platform}
            os={platform}
            current={platform === os}
            slots={release.slots.filter((s) => s.os === platform)}
          />
        ))}
      </ul>
    </section>
  );
}

function PlatformGroup({ os, current, slots }: { os: Os; current: boolean; slots: Slot[] }) {
  return (
    <li
      aria-labelledby={`os-${os}`}
      id={os}
      className="flex scroll-mt-6 flex-col gap-3 border-t border-line px-5 py-4 first:border-t-0 sm:flex-row sm:gap-6"
    >
      <div className="flex shrink-0 items-baseline gap-2 sm:w-[132px] sm:flex-col sm:gap-0.5 sm:pt-2">
        <h3 id={`os-${os}`} className="text-[15px]/5 font-semibold">
          {osNames[os]}
        </h3>
        {/* Every Linux package is built for one architecture; say it once. */}
        {(current || os === "linux") && (
          <p className="flex gap-1.5 text-[13px]/5 text-ink-2 sm:flex-col sm:gap-0">
            {current && <span className="font-medium text-green">This computer</span>}
            {current && os === "linux" && (
              <span aria-hidden="true" className="sm:hidden">
                ·
              </span>
            )}
            {os === "linux" && <span>{slots[0].arch}</span>}
          </p>
        )}
      </div>
      <ul className="flex min-w-0 flex-1 flex-col divide-y divide-divider">
        {slots.map((slot) => (
          <li key={slot.kind} className="flex items-center justify-between gap-4 py-2.5">
            <div className="flex min-w-0 flex-col gap-0.5">
              <span className="text-[14px]/5 font-medium">{kindLabels[slot.kind].title}</span>
              <span className="text-[13px]/[18px] text-ink-2">{slotNote(slot)}</span>
            </div>
            <DownloadButton
              artifact={slot.artifact}
              label={`Download ${kindLabels[slot.kind].title} for ${osNames[os]}`}
            />
          </li>
        ))}
      </ul>
    </li>
  );
}

/** "Apple silicon · 50.0 MB". */
function slotNote(slot: Slot) {
  return [kindLabels[slot.kind].note, slot.artifact ? formatBytes(slot.artifact.size) : null]
    .filter(Boolean)
    .join(" · ");
}

/** The small button in the platform list; "Shipping today" until the build is published. */
export function DownloadButton({ artifact, label }: { artifact: Slot["artifact"]; label: string }) {
  if (!artifact) return <span className="shrink-0 text-[13px]/5 text-ink-2">Shipping today</span>;
  return (
    <a
      href={artifact.url}
      download={fileName(artifact.url)}
      aria-label={label}
      className={cx(
        "inline-flex h-8 shrink-0 items-center gap-1.5 rounded-lg bg-raised px-3 text-[13px]/4 font-medium text-ink shadow-[var(--shadow-input)] transition-colors duration-150 hover:bg-hover dark:bg-sunken dark:hover:bg-hover",
        focusRing,
      )}
    >
      <Icon icon={Download01Icon} size={15} strokeWidth={1.8} className="text-ink-2" />
      Download
    </a>
  );
}

/** SHA-256 for every published file, folded away; the source link the AGPL asks for. */
export function Checksums({ release }: { release: Release }) {
  const files = release.slots.flatMap((s) => (s.artifact ? [s.artifact] : []));
  if (!files.length) return null;
  return (
    <details className="group rounded-2xl bg-sunken shadow-[inset_0_0_0_1px_var(--line)]">
      <summary
        className={cx(
          "flex cursor-pointer list-none items-center justify-between gap-4 rounded-2xl px-5 py-4 [&::-webkit-details-marker]:hidden",
          focusRing,
        )}
      >
        <span className="flex flex-col gap-0.5">
          <span className="text-[15px]/5 font-medium">Verify your download</span>
          <span className="text-[13px]/[18px] text-ink-2">
            SHA-256 checksums and the source for this build
          </span>
        </span>
        <Icon
          icon={ArrowDown01Icon}
          size={18}
          className="text-ink-2 transition-transform duration-200 ease-out group-open:rotate-180 motion-reduce:transition-none"
        />
      </summary>
      <div className="flex flex-col gap-4 border-t border-line px-5 pt-4 pb-5">
        <p className="text-[13px]/5 text-ink-2">
          Compare with <code className="font-mono text-[12px]">shasum -a 256 &lt;file&gt;</code> on
          macOS and Linux, or <code className="font-mono text-[12px]">Get-FileHash</code> on
          Windows.
        </p>
        <ul className="flex flex-col divide-y divide-line">
          {files.map((file) => (
            <li key={file.url} className="flex flex-col gap-1 py-2.5 first:pt-0 last:pb-0">
              <span className="truncate font-mono text-[12px]/4 text-ink">
                {fileName(file.url)}
              </span>
              <Sha value={file.sha256} />
            </li>
          ))}
        </ul>
        {release.source && (
          <a
            href={release.source.url}
            className={cx(
              "self-start rounded-sm text-[13px]/5 font-medium text-green hover:underline hover:underline-offset-2",
              focusRing,
            )}
          >
            Source for this build ({formatBytes(release.source.size)})
          </a>
        )}
      </div>
    </details>
  );
}

/** A SHA-256 checksum, shortened on screen, with a button that copies all of it. */
export function Sha({ value }: { value: string }) {
  const [copied, setCopied] = useState(false);
  return (
    <div className="flex min-w-0 items-center gap-2">
      <span className="min-w-0 truncate font-mono text-[11.5px]/4 text-ink-2" title={value}>
        {value}
      </span>
      <button
        type="button"
        aria-label="Copy checksum"
        onClick={async () => {
          try {
            await navigator.clipboard.writeText(value);
            setCopied(true);
            setTimeout(() => setCopied(false), 1500);
          } catch {
            setCopied(false);
          }
        }}
        className={cx(
          "flex size-7 shrink-0 cursor-pointer items-center justify-center rounded-md text-ink-2 hover:bg-hover hover:text-ink",
          focusRing,
        )}
      >
        <Icon icon={copied ? Tick02Icon : Copy01Icon} size={14} strokeWidth={1.8} />
      </button>
    </div>
  );
}
