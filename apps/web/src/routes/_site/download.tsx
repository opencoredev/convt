import { createFileRoute, Link } from "@tanstack/react-router";
import { createServerFn } from "@tanstack/react-start";
import { getRequestHeader } from "@tanstack/react-start/server";

import { cx, focusRing, PrimaryLink, SecondaryLink } from "#/components/app/ui";
import { CopySha, DownloadIcon } from "#/components/site/download";
import { TextLink, siteColumn } from "#/components/site/layout";
import {
  isOs,
  kindLabels,
  osFromUserAgent,
  osNames,
  osOrder,
  releaseFromManifest,
  type Os,
  type Release,
  type Slot,
} from "#/lib/platform";
import { fileName, formatBytes, parseReleaseManifest } from "#/lib/release-manifest";
import { fetchLatestManifest } from "#/server/latest-release";
import { GITHUB_URL, routes, seo } from "#/lib/site";

// The newest GitHub release's manifest (packaging/release/manifest.schema.json), read
// on each load. content/release-manifest.json is the fallback when GitHub has none or
// can't be reached; without either, the page shows what the first release will contain.
// The glob resolves at build time; scripts/generate-content.ts validates the file.
const files = import.meta.glob("../../../content/release-manifest.json", {
  eager: true,
  import: "default",
});
const raw = Object.values(files)[0];
const bundled = raw ? parseReleaseManifest(raw) : null;

const loadRelease = createServerFn({ method: "GET" }).handler(async () =>
  releaseFromManifest((await fetchLatestManifest()) ?? bundled),
);

const detectOs = createServerFn({ method: "GET" }).handler(() =>
  osFromUserAgent(getRequestHeader("user-agent") ?? ""),
);

const RELEASES_URL = `${GITHUB_URL}/releases`;

export const Route = createFileRoute("/_site/download")({
  validateSearch: (search: Record<string, unknown>): { os?: Os } =>
    isOs(search.os) ? { os: search.os } : {},
  loaderDeps: ({ search }) => ({ os: search.os }),
  // The User-Agent picks the build on the first load; client navigations read it locally.
  // Check `window`, not `navigator`: Workers define navigator with their own user agent.
  loader: async ({ deps }) => {
    const detected =
      typeof window === "undefined" ? await detectOs() : osFromUserAgent(navigator.userAgent);
    return { detected, os: deps.os ?? detected, release: await loadRelease() };
  },
  head: () =>
    seo({
      title: "Download convt for macOS, Windows and Linux",
      description:
        "convt for macOS (universal), Windows and Linux (AppImage, .deb, .rpm, tarball), with checksums and the matching source code.",
      path: routes.download,
    }),
  component: DownloadPage,
});

const sectionTitle = "text-[28px]/9 font-semibold tracking-[-0.03em] text-balance";
const eyebrow = "font-mono text-[11px]/3.5 text-ink-2 uppercase";
const card = "rounded-2xl bg-raised shadow-[inset_0_0_0_1px_var(--line)] dark:bg-panel";

/** "2026-10-07" to "7 Oct 2026", without going through a time zone. */
function formatDate(date: string) {
  const [y, m, d] = date.split("-").map(Number);
  const months = "Jan Feb Mar Apr May Jun Jul Aug Sep Oct Nov Dec".split(" ");
  return `${d} ${months[m - 1]} ${y}`;
}

/** ".deb", "AppImage": how a format reads on a button. */
function kindName(slot: Slot) {
  return slot.kind === "AppImage" ? slot.kind : `.${slot.kind}`;
}

const isLive = (release: Release) => release.slots.some((s) => s.artifact);
const publishedFor = (release: Release, os: Os) =>
  release.slots.filter((s) => s.os === os && s.artifact);

function DownloadPage() {
  return (
    <div className={cx(siteColumn, "flex flex-col gap-20 pt-12 pb-24 md:gap-28 md:pt-20")}>
      <div className="flex flex-col gap-12 md:gap-16">
        <Hero />
        <ProductWindow />
      </div>
      <Platforms />
      <FirstSteps />
      <Trust />
    </div>
  );
}

function Hero() {
  const { release } = Route.useLoaderData();
  const live = isLive(release);
  return (
    <section
      aria-labelledby="download-title"
      className="flex flex-col items-center gap-6 text-center"
    >
      {live && release.version ? (
        <a
          href={routes.changelog}
          className={cx(
            "inline-flex items-center gap-2 rounded-full bg-raised py-1 pr-3 pl-1 text-[13px]/5 text-ink-2 shadow-[inset_0_0_0_1px_var(--line)] transition-colors hover:text-ink dark:bg-panel",
            focusRing,
          )}
        >
          <span className="rounded-full bg-green-tint px-2 font-mono text-[11px]/5 font-medium text-[#157f4a] dark:text-green">
            v{release.version}
          </span>
          {release.date ? `Released ${formatDate(release.date)}` : "Latest release"}
          <span aria-hidden="true">→</span>
        </a>
      ) : (
        <p className="inline-flex items-center gap-2.5 rounded-full bg-raised py-1 pr-3.5 pl-3 text-[13px]/5 text-ink-2 shadow-[inset_0_0_0_1px_var(--line)] dark:bg-panel">
          <span className="relative flex size-2">
            <span className="absolute inline-flex size-full animate-ping rounded-full bg-green opacity-50 motion-reduce:hidden" />
            <span className="relative inline-flex size-2 rounded-full bg-green" />
          </span>
          First release in final checks
        </p>
      )}
      <div className="flex flex-col items-center gap-4">
        <h1
          id="download-title"
          className="text-[44px]/[48px] font-semibold tracking-[-0.04em] text-balance sm:text-[60px]/[64px]"
        >
          Download convt
        </h1>
        <p className="max-w-[600px] text-[17px]/[26px] text-pretty text-ink-2 sm:text-[18px]/[28px]">
          The app, the right-click menu and the{" "}
          <code className="font-mono text-[0.9em] text-ink">convt</code> command line tool in one
          install. Every download starts a 7-day free trial, no account needed.
        </p>
      </div>
      {live ? <LiveAction /> : <PreReleaseAction />}
    </section>
  );
}

/** The big button for the picked system and its file line. */
function LiveAction() {
  const { release, os } = Route.useLoaderData();
  // No build for the picked system (or a phone): point at the systems that have one.
  const main = os ? publishedFor(release, os)[0] : undefined;

  if (!os || !main) {
    return (
      <div className="flex flex-col items-center gap-4">
        <a
          href="#platforms"
          className={cx(
            "btn-primary inline-flex h-12 items-center justify-center gap-2 rounded-xl px-6 text-[15px]/5 font-medium",
            focusRing,
          )}
        >
          Choose your system
          <span aria-hidden="true">↓</span>
        </a>
        <p className="max-w-[420px] text-[13px]/5 text-pretty text-ink-2">
          {os
            ? `Version ${release.version} has no ${osNames[os]} build yet. It will appear here as soon as it is published.`
            : "convt runs on macOS, Windows and Linux. On a phone, convt Pro converts files in the browser."}
        </p>
        <OsSwitch />
      </div>
    );
  }

  const artifact = main.artifact!;
  const file = fileName(artifact.url);
  return (
    <div className="flex w-full flex-col items-center gap-4">
      <PrimaryLink
        href={artifact.url}
        download={file}
        className="h-12 w-full max-w-[320px] gap-2.5 rounded-xl px-6 text-[15px]/5 sm:w-auto"
      >
        <DownloadIcon />
        Download for {osNames[os]}
      </PrimaryLink>
      <p className="flex max-w-full flex-wrap items-center justify-center gap-x-2.5 gap-y-1 font-mono text-[11.5px]/4 text-ink-2">
        <span>
          {kindName(main)} · {formatBytes(artifact.size)}
        </span>
        <span aria-hidden="true" className="text-line-strong">
          ·
        </span>
        <span className="flex items-center gap-2">
          <span title={artifact.sha256}>
            SHA-256 {artifact.sha256.slice(0, 8)}…{artifact.sha256.slice(-4)}
          </span>
          <CopySha value={artifact.sha256} label="Copy" />
        </span>
      </p>
      <OsSwitch />
    </div>
  );
}

/** Same skeleton as a release, with the planned formats in place of a file. No dead buttons. */
function PreReleaseAction() {
  const { release, os } = Route.useLoaderData();
  const planned = os ? release.slots.filter((s) => s.os === os) : [];
  return (
    <div className="flex flex-col items-center gap-4">
      <div className="flex w-full flex-col gap-2.5 sm:w-auto sm:flex-row">
        <PrimaryLink href={RELEASES_URL} className="h-12 gap-2 rounded-xl px-6 text-[15px]/5">
          <GitHubIcon />
          Watch releases on GitHub
        </PrimaryLink>
        <SecondaryLink href={routes.pricing} className="h-12 rounded-xl px-5 text-[15px]/5">
          See pricing
        </SecondaryLink>
      </div>
      <p className="max-w-[460px] font-mono text-[11.5px]/4 text-pretty text-ink-2">
        {os ? `${osNames[os]}: ${planned.map(kindName).join(", ")}` : "macOS, Windows and Linux"}{" "}
        <span className="whitespace-nowrap">· not published yet</span>
      </p>
      <OsSwitch />
    </div>
  );
}

/** A small picker under the button. Links, so each system has its own URL and works before hydration. */
function OsSwitch() {
  const { os, detected } = Route.useLoaderData();
  return (
    <nav
      aria-label="System"
      className="flex items-center gap-0.5 rounded-full bg-sunken p-0.5 shadow-[inset_0_0_0_1px_var(--line)]"
    >
      {osOrder.map((item) => {
        const active = item === os;
        return (
          <Link
            key={item}
            to="."
            search={{ os: item }}
            replace
            resetScroll={false}
            aria-current={active ? "page" : undefined}
            className={cx(
              "flex h-7 items-center gap-1.5 rounded-full px-3 text-[12.5px]/4 font-medium transition-colors",
              active ? "bg-raised text-ink shadow-button" : "text-ink-2 hover:text-ink",
              focusRing,
            )}
          >
            {osNames[item]}
            {item === detected && (
              <span
                className="size-1.5 rounded-full bg-green"
                title="Your system"
                aria-label="(your system)"
              />
            )}
          </Link>
        );
      })}
    </nav>
  );
}

/**
 * What one install adds, drawn rather than photographed: a folder, the right-click menu
 * and the command line tool. Decorative; the page text says the same thing.
 */
function ProductWindow() {
  const files = [
    { name: "miso.heic", tone: "photo" },
    { name: "launch.mov", tone: "#2a2f2c" },
    { name: "notes.docx", tone: "#2f6fd6" },
    { name: "intro.wav", tone: "#c2731c" },
    { name: "logo.svg", tone: "#7a4fd1" },
    { name: "report.pdf", tone: "#c8382c" },
  ];
  const item = "flex h-7 items-center justify-between gap-6 rounded-md px-2.5 text-[13px]/4";
  return (
    <div
      aria-hidden="true"
      className="dither relative flex justify-center overflow-clip rounded-[20px] bg-panel px-4 pt-10 shadow-[inset_0_0_0_1px_var(--line)] select-none sm:px-10 sm:pt-14"
    >
      <div className="relative w-full max-w-[860px] rounded-t-xl bg-raised shadow-note">
        <div className="flex h-10 items-center gap-2 border-b border-divider px-4">
          <span className="size-3 rounded-full bg-[#ff5f57]" />
          <span className="size-3 rounded-full bg-[#febc2e]" />
          <span className="size-3 rounded-full bg-[#28c840]" />
          <span className="ml-3 text-[13px]/4 font-medium text-ink-2">Downloads</span>
        </div>
        <div className="grid grid-cols-3 gap-x-2 gap-y-5 px-4 pt-6 pb-8 sm:grid-cols-6 sm:px-6 sm:pb-44">
          {files.map((f, i) => (
            <div key={f.name} className="flex flex-col items-center gap-2">
              <div
                className={cx(
                  "flex h-[64px] w-[52px] items-end justify-center overflow-clip rounded-md pb-1.5 shadow-[0_0_0_1px_rgb(0_0_0/8%),0_1px_2px_rgb(0_0_0/10%)]",
                  i === 0 && "ring-2 ring-[#1f9d5c] ring-offset-2 ring-offset-raised",
                )}
                style={f.tone === "photo" ? undefined : { background: f.tone }}
              >
                {f.tone === "photo" ? (
                  <img src="/landing/miso.jpg" alt="" className="size-full object-cover" />
                ) : (
                  <span className="font-mono text-[9px]/3 font-medium text-white/85 uppercase">
                    {f.name.split(".")[1]}
                  </span>
                )}
              </div>
              <span
                className={cx(
                  "rounded px-1.5 text-[12px]/[18px]",
                  i === 0 ? "bg-[#1f9d5c] text-white" : "text-ink-2",
                )}
              >
                {f.name}
              </span>
            </div>
          ))}
        </div>
        {/* The context menu opens from the selected photo. */}
        <div className="absolute top-[64px] left-[136px] hidden w-[200px] flex-col rounded-[10px] bg-raised p-1.5 shadow-note sm:flex">
          <span className={cx(item, "text-ink")}>Open</span>
          <span className={cx(item, "text-ink")}>Get Info</span>
          <span className="mx-2.5 my-1 h-px bg-divider" />
          <span className={cx(item, "bg-[#1f9d5c] font-medium text-white")}>
            Convert to <span>›</span>
          </span>
          <span className={cx(item, "text-ink")}>Rename</span>
          <span className={cx(item, "text-ink")}>Compress</span>
        </div>
        <div className="absolute top-[146px] left-[330px] hidden w-[150px] flex-col rounded-[10px] bg-raised p-1.5 shadow-note sm:flex">
          {["WebP", "PNG", "JPEG", "AVIF"].map((f, i) => (
            <span key={f} className={cx(item, i === 0 ? "bg-hover text-ink" : "text-ink")}>
              {f}
            </span>
          ))}
          <span className="mx-2.5 my-1 h-px bg-divider" />
          <span className={cx(item, "text-ink-2")}>More formats…</span>
        </div>
        <div className="absolute right-6 bottom-6 hidden w-[330px] rounded-[10px] bg-code px-4 py-3.5 font-mono text-[12px]/5 text-code-ink shadow-note md:block">
          <p>
            <span className="text-[#4cc283]">$</span> convt miso.heic --to webp
          </p>
          <p className="text-[#838985]">miso.webp · 612 KB · 0.4s</p>
        </div>
      </div>
    </div>
  );
}

/** One card per system: every format with its size and checksum, the visitor's system marked. */
function Platforms() {
  const { release, detected } = Route.useLoaderData();
  const live = isLive(release);
  return (
    <section
      aria-labelledby="platforms-title"
      id="platforms"
      className="flex scroll-mt-8 flex-col gap-8"
    >
      <div className="flex flex-col gap-2 sm:flex-row sm:items-end sm:justify-between">
        <h2 id="platforms-title" className={sectionTitle}>
          Every system, every format
        </h2>
        <p className="text-sm/5 text-ink-2">
          {live
            ? `Version ${release.version}${release.date ? ` · ${formatDate(release.date)}` : ""}`
            : "Listed with size and checksum once published"}
        </p>
      </div>
      <div className="grid grid-cols-1 gap-4 lg:grid-cols-2">
        {osOrder.map((os) => (
          <PlatformCard key={os} os={os} mine={os === detected} />
        ))}
      </div>
    </section>
  );
}

const osBlurb: Record<Os, string> = {
  macos: "One universal app for Apple silicon and Intel Macs.",
  windows: "A per-user installer for 64-bit Windows. No admin rights needed.",
  linux: "Packages for most distributions, plus an AppImage that runs anywhere.",
};

function PlatformCard({ os, mine }: { os: Os; mine: boolean }) {
  const { release } = Route.useLoaderData();
  const slots = release.slots.filter((s) => s.os === os);
  const live = isLive(release);
  return (
    <article
      id={os}
      className={cx(
        card,
        "flex min-w-0 scroll-mt-8 flex-col gap-5 p-5 sm:p-6",
        // Linux lists four formats; it takes the right column beside macOS and Windows.
        os === "linux" && "lg:col-start-2 lg:row-span-2 lg:row-start-1",
        mine &&
          "shadow-[inset_0_0_0_1px_var(--green-line),0_0_0_3px_var(--green-tint)] dark:shadow-[inset_0_0_0_1px_#1f5a3b]",
      )}
    >
      <div className="flex items-start justify-between gap-3">
        <OsThumb os={os} />
        {mine && (
          <span className="inline-flex items-center gap-1.5 rounded-full bg-green-tint px-2 py-0.5 font-mono text-[10.5px]/4 font-medium text-[#157f4a] uppercase dark:text-green">
            <span className="size-1.5 rounded-full bg-green" aria-hidden="true" />
            Your system
          </span>
        )}
      </div>
      <div className="flex flex-col gap-1.5">
        <h3 className="text-xl/7 font-semibold tracking-[-0.02em]">{osNames[os]}</h3>
        <p className="text-sm/5 text-pretty text-ink-2">{osBlurb[os]}</p>
      </div>
      <ul className="mt-auto flex flex-col border-t border-divider">
        {slots.map((slot) => (
          <FormatRow key={slot.kind} slot={slot} live={live} />
        ))}
      </ul>
    </article>
  );
}

function FormatRow({ slot, live }: { slot: Slot; live: boolean }) {
  const { release } = Route.useLoaderData();
  const artifact = slot.artifact;
  const file = artifact ? fileName(artifact.url) : null;
  return (
    <li
      id={`file-${slot.kind}`}
      className="flex min-h-[60px] items-center justify-between gap-3 border-b border-divider py-3 last:border-b-0 last:pb-0"
    >
      <div className="flex min-w-0 flex-col gap-0.5">
        <span className="flex flex-wrap items-center gap-x-2.5 gap-y-0.5">
          <span className="text-sm/5 font-medium">{kindName(slot)}</span>
          {artifact ? (
            <>
              <span className="font-mono text-[11.5px]/4 text-ink-2">
                {formatBytes(artifact.size)}
              </span>
              <CopySha value={artifact.sha256} />
            </>
          ) : (
            <span className="font-mono text-[11.5px]/4 text-ink-3">
              {live ? `Not in ${release.version}` : "Not published yet"}
            </span>
          )}
        </span>
        <span className="text-[13px]/[18px] text-pretty text-ink-2">
          {kindLabels[slot.kind].note}
        </span>
      </div>
      {artifact && (
        <SecondaryLink
          href={artifact.url}
          download={file!}
          aria-label={`Download ${kindName(slot)} for ${osNames[slot.os]}`}
          className="h-9 shrink-0 gap-1.5 px-3.5"
        >
          <DownloadIcon className="size-3.5" />
          Download
        </SecondaryLink>
      )}
    </li>
  );
}

/** A tiny window in each system's style instead of borrowed logos. */
function OsThumb({ os }: { os: Os }) {
  return (
    <div
      aria-hidden="true"
      className="flex h-[44px] w-[64px] flex-col overflow-clip rounded-md bg-sunken shadow-[inset_0_0_0_1px_var(--line-strong)]"
    >
      {os === "macos" && (
        <div className="flex h-3 items-center gap-[3px] px-1.5">
          <span className="size-[5px] rounded-full bg-[#ff5f57]" />
          <span className="size-[5px] rounded-full bg-[#febc2e]" />
          <span className="size-[5px] rounded-full bg-[#28c840]" />
        </div>
      )}
      {os === "windows" && (
        <div className="flex h-3 items-center justify-end gap-[5px] px-1.5 text-ink-2">
          <span className="h-px w-[5px] bg-current" />
          <span className="size-[5px] border border-current" />
          <span className="relative size-[6px] before:absolute before:top-1/2 before:left-0 before:h-px before:w-full before:rotate-45 before:bg-current after:absolute after:top-1/2 after:left-0 after:h-px after:w-full after:-rotate-45 after:bg-current" />
        </div>
      )}
      {os === "linux" && (
        <div className="flex h-3 items-center justify-between bg-code px-1.5">
          <span className="h-[3px] w-4 rounded-full bg-white/25" />
          <span className="size-[5px] rounded-full bg-white/25" />
        </div>
      )}
      <div
        className={cx(
          "flex-1",
          os === "linux" ? "bg-code px-1.5 pt-1" : "border-t border-line px-1.5 pt-1.5",
        )}
      >
        {os === "linux" ? (
          <span className="font-mono text-[8px]/[10px] text-[#4cc283]">
            $ <span className="inline-block h-[7px] w-[4px] translate-y-px bg-code-ink/70" />
          </span>
        ) : (
          <div className="flex gap-1">
            <span className="h-[18px] w-3 rounded-[2px] bg-line-strong" />
            <span className="h-[18px] w-3 rounded-[2px] bg-[#1f9d5c]" />
            <span className="h-[18px] w-3 rounded-[2px] bg-line-strong" />
          </div>
        )}
      </div>
    </div>
  );
}

const installStep: Record<Os, string> = {
  macos: "Open the .dmg and drag convt into Applications.",
  windows: "Run the installer. It sets up convt for your account, no admin rights needed.",
  linux: "Install the .deb or .rpm, or make the AppImage executable and run it.",
};

/** What happens after the download, so the page ends at "it works", not at a file. */
function FirstSteps() {
  const { os } = Route.useLoaderData();
  const steps = [
    {
      title: "Install",
      body: os ? installStep[os] : "Open the download for your system and install it.",
    },
    {
      title: "Right-click any file",
      body: "Choose Convert to in the menu. Images, video, audio and documents all work.",
    },
    {
      title: "Pick a format",
      body: "The converted file lands next to the original. Nothing leaves your computer.",
    },
  ];
  return (
    <section aria-labelledby="steps-title" className="flex flex-col gap-8">
      <h2 id="steps-title" className={sectionTitle}>
        Converting in under a minute
      </h2>
      <ol className="grid gap-px overflow-clip rounded-2xl bg-line shadow-[0_0_0_1px_var(--line)] md:grid-cols-3">
        {steps.map((step, i) => (
          <li key={step.title} className="flex flex-col gap-3 bg-raised p-6 dark:bg-panel">
            <span className="flex size-7 items-center justify-center rounded-full bg-sunken font-mono text-xs/4 font-medium text-ink shadow-[inset_0_0_0_1px_var(--line-strong)]">
              {i + 1}
            </span>
            <h3 className="text-base/6 font-semibold">{step.title}</h3>
            <p className="text-sm/[21px] text-pretty text-ink-2">{step.body}</p>
          </li>
        ))}
      </ol>
    </section>
  );
}

const verifyCommands: Record<Os, { label: string; command: (file: string) => string }> = {
  macos: { label: "macOS, in Terminal", command: (f) => `shasum -a 256 ${f}` },
  windows: { label: "Windows, in PowerShell", command: (f) => `Get-FileHash ${f}` },
  linux: { label: "Linux", command: (f) => `sha256sum ${f}` },
};

/** Checksums, source and privacy as three short claims; the commands fold away. */
function Trust() {
  const { release, os } = Route.useLoaderData();
  const source = release.source;
  const order = os ? [os, ...osOrder.filter((o) => o !== os)] : osOrder;
  const files = order.flatMap((platform) =>
    publishedFor(release, platform).map((slot) => ({
      platform,
      slot,
      file: fileName(slot.artifact!.url),
    })),
  );
  // One command per system, filled in with that system's first file.
  const rows = order.flatMap((platform) => {
    const first = files.find((f) => f.platform === platform);
    return first ? [first] : [];
  });
  return (
    <section
      aria-label="Verify, source and privacy"
      className="grid gap-10 border-t border-line pt-12 md:grid-cols-3 md:gap-12"
    >
      <div className="flex flex-col gap-2">
        <p className={eyebrow}>Checksums</p>
        <p className="text-sm/[21px] text-pretty text-ink-2">
          Every file lists its SHA-256. Copy it from the download and compare it with your copy.
        </p>
        {rows.length > 0 && (
          <details id="verify" className="group mt-1 scroll-mt-8">
            <summary
              className={cx(
                "inline-flex cursor-pointer list-none items-center gap-1 rounded-sm text-sm/5 font-medium text-[#157f4a] dark:text-green [&::-webkit-details-marker]:hidden",
                focusRing,
              )}
            >
              Verify download
              <span aria-hidden="true" className="transition-transform group-open:rotate-90">
                ›
              </span>
            </summary>
            <div className="mt-3 flex min-w-0 flex-col gap-4">
              <p className="text-[13px]/5 text-ink-2">
                Run the command in the folder you saved the file to. The result should match the
                SHA-256 below; if it differs, delete the file and download it again.
              </p>
              <dl className="flex min-w-0 flex-col gap-2.5 font-mono text-[12px]/5">
                {rows.map(({ platform, file }) => (
                  <div key={platform} className="flex flex-col gap-1">
                    <dt className="font-sans text-xs/4 text-ink-2">
                      {verifyCommands[platform].label}
                    </dt>
                    <dd className="overflow-x-auto rounded-lg bg-code px-3 py-2 whitespace-nowrap text-code-ink">
                      {verifyCommands[platform].command(file)}
                    </dd>
                  </div>
                ))}
              </dl>
              <dl className="flex min-w-0 flex-col gap-2 font-mono text-[11px]/4">
                {files.map(({ slot, file }) => (
                  <div key={file} className="flex flex-col gap-0.5">
                    <dt className="truncate text-ink">{file}</dt>
                    <dd className="break-all text-ink-2">{slot.artifact!.sha256}</dd>
                  </div>
                ))}
              </dl>
            </div>
          </details>
        )}
      </div>
      <div className="flex flex-col gap-2">
        <p className={eyebrow}>Source code</p>
        <p className="text-sm/[21px] text-pretty text-ink-2">
          convt is free software under the{" "}
          <TextLink href={`${GITHUB_URL}/blob/main/LICENSE`}>GNU AGPL-3.0</TextLink>.{" "}
          {source
            ? "The source archive is built from the same commit as these downloads."
            : "Each release publishes a source archive built from the same commit as its downloads."}
        </p>
        {source ? (
          <p className="mt-1 flex flex-wrap items-center gap-x-3 gap-y-1">
            <TextLink href={source.url} download={fileName(source.url)} className="text-sm/5">
              Download source · {formatBytes(source.size)}
            </TextLink>
            <CopySha value={source.sha256} />
          </p>
        ) : (
          <TextLink href={GITHUB_URL} className="mt-1 self-start text-sm/5">
            View on GitHub
          </TextLink>
        )}
      </div>
      <div className="flex flex-col gap-2">
        <p className={eyebrow}>Private by design</p>
        <p className="text-sm/[21px] text-pretty text-ink-2">
          Conversions run on your computer, offline. Documents use an optional pack the app offers
          the first time you need it.
        </p>
        <TextLink href={routes.changelog} className="mt-1 self-start text-sm/5">
          Release notes
        </TextLink>
      </div>
    </section>
  );
}

function GitHubIcon() {
  return (
    <svg width="16" height="16" viewBox="0 0 16 16" aria-hidden="true" fill="currentColor">
      <path d="M8 0C3.58 0 0 3.58 0 8c0 3.54 2.29 6.53 5.47 7.59.4.07.55-.17.55-.38 0-.19-.01-.82-.01-1.49-2.01.37-2.53-.49-2.69-.94-.09-.23-.48-.94-.82-1.13-.28-.15-.68-.52-.01-.53.63-.01 1.08.58 1.23.82.72 1.21 1.87.87 2.33.66.07-.52.28-.87.51-1.07-1.78-.2-3.64-.89-3.64-3.95 0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82.64-.18 1.32-.27 2-.27.68 0 1.36.09 2 .27 1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82 1.27.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48 0 1.07-.01 1.93-.01 2.2 0 .21.15.46.55.38A8.013 8.013 0 0016 8c0-4.42-3.58-8-8-8z" />
    </svg>
  );
}
