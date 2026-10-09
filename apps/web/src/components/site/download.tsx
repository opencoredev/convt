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
 * - Reference: Linear's download page (one centered app tile, one big button for the
 *   visitor's system with a quiet meta line, every other build as a plain list) and
 *   Tailscale's ("install the app and sign in"): the page answers one question, which
 *   file do I want, and then says what happens after.
 * - Palette: the shared tokens, the green gradient of .btn-primary for the one primary
 *   action, and the dithered glow from sign-in rising behind it.
 * - Type: Inter; Geist Mono for versions, sizes, commands and checksums.
 * - Layout: a centered stage for the recommended build, then the steps after
 *   installing, then every platform as rows (not three uneven cards), then checksums
 *   folded away.
 * - Signature: the app tile floating on the pixel-grain glow, the same glow the account
 *   pages use, so download and sign-in read as one flow.
 * - Avoiding: raw file names on buttons, wrapping commands, equal feature cards.
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

/** What to do with the file once it's downloaded, per system. */
const openStep: Record<Os, string> = {
  macos: "Open the disk image and drag convt to Applications.",
  windows: "Run the installer, then open convt from the Start menu.",
  linux: "Make the AppImage executable and run it, or install a package.",
};

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
      <DitherGlow className="absolute inset-x-0 bottom-0 -z-10 h-[78%] w-full [mask-image:linear-gradient(to_bottom,transparent,black_35%,black_78%,transparent)]" />
      <div className="mx-auto flex max-w-[640px] flex-col items-center px-5 pt-14 pb-16 text-center md:pt-20 md:pb-24">
        <Rise index={0}>
          <span className="flex size-[76px] items-center justify-center rounded-[22px] bg-raised shadow-[0_0_0_1px_var(--line),0_1px_2px_rgb(0_0_0/6%),0_12px_32px_rgb(10_60_35/14%)] dark:shadow-[0_0_0_1px_#2e3331,0_12px_32px_rgb(0_0_0/50%)]">
            <Mark size={40} />
          </span>
        </Rise>
        <Rise index={1} className="mt-7 flex flex-col items-center gap-3">
          {release.version && (
            <p className="font-mono text-[12px]/4 text-ink-2 uppercase">
              Version {release.version}
              {release.date ? ` · ${formatReleaseDate(release.date)}` : ""}
            </p>
          )}
          <h1
            id="download-title"
            className="text-[40px]/[44px] font-semibold tracking-[-0.035em] text-balance md:text-[52px]/[56px]"
          >
            Download convt
          </h1>
          <p className="max-w-[460px] text-[17px]/[26px] text-pretty text-ink-2">
            One install gives you the app, the{" "}
            <span className="whitespace-nowrap">right-click</span> menu and the{" "}
            <code className="font-mono text-[15px] text-ink">convt</code> command.
          </p>
        </Rise>
        <Rise index={2} className="mt-9 flex w-full flex-col items-center gap-3">
          {slot ? (
            <>
              <PrimaryDownload slot={slot} />
              <p className="font-mono text-[12.5px]/[18px] text-ink-2">
                {primaryMeta(slot, release.version)}
              </p>
              {extras.length > 0 && (
                <p className="text-[13px]/5 text-ink-2">
                  Also as{" "}
                  {extras.map(({ slot: s, artifact }, i, list) => (
                    <span key={s.kind}>
                      <a
                        href={artifact.url}
                        download={fileName(artifact.url)}
                        className={cx(
                          "rounded-sm font-medium text-ink underline decoration-line-strong underline-offset-3 hover:decoration-ink",
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
        {os === "macos" && slot?.artifact && (
          <Rise index={3} className="mt-8 w-full max-w-[520px]">
            <CommandBlock label="Or with Homebrew" commands={homebrewCommands} />
          </Rise>
        )}
        {os && (
          <Rise index={4} className="mt-6">
            <a
              href="#platforms"
              className={cx(
                "inline-flex items-center gap-1 rounded-sm text-[13px]/5 text-ink-2 hover:text-ink",
                focusRing,
              )}
            >
              Other platforms
              <Icon icon={ArrowDown01Icon} size={14} />
            </a>
          </Rise>
        )}
      </div>
    </section>
  );
}

function shortKind(slot: Slot) {
  return slot.kind === "AppImage" ? "AppImage" : `.${slot.kind}`;
}

const releaseDate = new Intl.DateTimeFormat("en-US", {
  month: "short",
  day: "numeric",
  year: "numeric",
  timeZone: "UTC",
});

function formatReleaseDate(day: string) {
  return releaseDate.format(new Date(`${day}T00:00:00Z`));
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

/** Open, sign in, trial: the account flow after the download. */
export function NextSteps({ os }: { os: Os | null }) {
  const steps = [
    {
      title: "Install convt",
      body: os ? openStep[os] : "Download it for your Mac, Windows or Linux computer.",
    },
    {
      title: "Sign in",
      body: "Open convt and sign in with this account. Your browser handles the rest.",
    },
    {
      title: "Start your free trial",
      body: "7 days of convt Pro. Cancel before it ends and you won't be charged.",
    },
  ];
  return (
    <section aria-labelledby="next-title" className="flex flex-col gap-6">
      <h2 id="next-title" className="font-mono text-[12px]/4 text-ink-2 uppercase">
        What happens next
      </h2>
      <ol className="grid gap-6 md:grid-cols-3 md:gap-0">
        {steps.map((step, i) => (
          <li key={step.title} className="relative flex gap-4 md:flex-col md:gap-4 md:pr-8">
            <span className="relative z-10 flex size-8 shrink-0 items-center justify-center rounded-full bg-green-tint font-mono text-[13px]/4 font-medium text-green shadow-[inset_0_0_0_1px_var(--green-line)]">
              {i + 1}
            </span>
            {/* The thread between the numbers, desktop only. */}
            {i < steps.length - 1 && (
              <span
                aria-hidden="true"
                className="absolute top-4 right-0 left-10 hidden h-px bg-[linear-gradient(90deg,var(--green-line),var(--line))] md:block"
              />
            )}
            <div className="flex flex-col gap-1">
              <h3 className="text-[16px]/6 font-semibold tracking-[-0.01em]">{step.title}</h3>
              <p className="max-w-[300px] text-[14px]/[22px] text-pretty text-ink-2">{step.body}</p>
            </div>
          </li>
        ))}
      </ol>
    </section>
  );
}

/** Every build, one row per system and a line per file. */
export function Platforms({ os, release }: { os: Os | null; release: Release }) {
  return (
    <section
      aria-labelledby="platforms-title"
      id="platforms"
      className="flex scroll-mt-6 flex-col gap-6"
    >
      <div className="flex flex-wrap items-baseline justify-between gap-x-6 gap-y-2">
        <h2 id="platforms-title" className="text-[28px]/9 font-semibold tracking-[-0.025em]">
          All platforms
        </h2>
        <a
          href={`${GITHUB_URL}/releases`}
          className={cx("rounded-sm text-[13px]/5 text-ink-2 hover:text-ink", focusRing)}
        >
          Older versions on GitHub
        </a>
      </div>
      <ul className="flex flex-col overflow-hidden rounded-2xl bg-raised shadow-[0_0_0_1px_var(--line),0_1px_2px_rgb(0_0_0/4%)] dark:bg-panel">
        {osOrder.map((platform) => (
          <PlatformRow
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

function PlatformRow({ os, current, slots }: { os: Os; current: boolean; slots: Slot[] }) {
  return (
    <li
      aria-labelledby={`os-${os}`}
      id={os}
      className="flex scroll-mt-6 flex-col gap-4 border-t border-line p-5 first:border-t-0 md:flex-row md:gap-8 md:p-6"
    >
      <div className="flex shrink-0 flex-col gap-1 md:w-[220px]">
        <h3 id={`os-${os}`} className="text-[18px]/6 font-semibold tracking-[-0.015em]">
          {osNames[os]}
        </h3>
        {/* macOS and Windows say the architecture in each file's note; Linux packages don't. */}
        {(current || os === "linux") && (
          <p className="flex items-center gap-1.5 text-[13px]/5 text-ink-2">
            {current && (
              <>
                <span aria-hidden="true" className="size-1.5 rounded-full bg-green" />
                <span className="font-medium text-green">Your computer</span>
              </>
            )}
            {current && os === "linux" && <span aria-hidden="true">·</span>}
            {os === "linux" && <span className="font-mono text-[12px]">{slots[0].arch}</span>}
          </p>
        )}
      </div>
      <ul className="flex min-w-0 flex-1 flex-col divide-y divide-divider">
        {slots.map((slot) => (
          <li
            key={slot.kind}
            className="flex items-center justify-between gap-4 py-3 first:pt-0 last:pb-0 md:first:pt-0"
          >
            <div className="flex min-w-0 flex-col gap-0.5">
              <span className="text-[14px]/5 font-medium">{kindLabels[slot.kind].title}</span>
              <span className="text-[13px]/[18px] text-ink-2">
                {kindLabels[slot.kind].note}
                {slot.artifact ? ` · ${formatBytes(slot.artifact.size)}` : ""}
              </span>
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

/** The small button in the platform list; "Shipping today" until the build is published. */
export function DownloadButton({ artifact, label }: { artifact: Slot["artifact"]; label: string }) {
  if (!artifact)
    return (
      <span className="shrink-0 rounded-full bg-chip px-3 py-1.5 text-[12px]/4 font-medium text-ink-2 shadow-[inset_0_0_0_1px_var(--chip-line)]">
        Shipping today
      </span>
    );
  return (
    <a
      href={artifact.url}
      download={fileName(artifact.url)}
      aria-label={label}
      className={cx(
        "inline-flex h-9 shrink-0 items-center gap-2 rounded-full bg-pill pr-4 pl-3.5 text-[13px]/4 font-medium text-ink shadow-[inset_0_0_0_1px_var(--pill-line)] transition-colors duration-150 hover:bg-pill-hover",
        focusRing,
      )}
    >
      <Icon icon={Download01Icon} size={16} strokeWidth={1.75} className="text-ink-2" />
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
