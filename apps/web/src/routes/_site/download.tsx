import { createFileRoute, Link } from "@tanstack/react-router";
import { createServerFn } from "@tanstack/react-start";
import { getRequestHeader } from "@tanstack/react-start/server";

import { cx, focusRing, PrimaryLink, SecondaryLink } from "#/components/app/ui";
import { DownloadIcon, Sha } from "#/components/site/download";
import { PageHeader, TextLink, siteColumn } from "#/components/site/layout";
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

const sectionTitle = "text-2xl/8 font-semibold tracking-[-0.02em]";
const eyebrow = "font-mono text-[11px]/3.5 text-ink-2 uppercase";

/** "2026-10-07" to "7 Oct 2026", without going through a time zone. */
function formatDate(date: string) {
  const [y, m, d] = date.split("-").map(Number);
  const months = "Jan Feb Mar Apr May Jun Jul Aug Sep Oct Nov Dec".split(" ");
  return `${d} ${months[m - 1]} ${y}`;
}

function archLabel(slot: Slot) {
  return slot.arch === "universal" ? "Universal" : slot.arch;
}

const isLive = (release: Release) => release.slots.some((s) => s.artifact);

function DownloadPage() {
  const { release } = Route.useLoaderData();
  const live = isLive(release);
  return (
    <div className={cx(siteColumn, "flex flex-col gap-16 pt-12 pb-20 md:gap-20 md:pt-16")}>
      <div className="flex flex-col gap-8 md:gap-10">
        <PageHeader
          eyebrow={
            live && release.version
              ? `Version ${release.version}${release.date ? ` · ${formatDate(release.date)}` : ""}`
              : "Download"
          }
          title="Download convt"
        >
          <p>
            The app, the right-click menu and the{" "}
            <code className="font-mono text-[15px]">convt</code> command line tool in one install.
            Every download starts a 7-day free trial; no account needed.
          </p>
        </PageHeader>
        <Hero />
      </div>
      {live ? <AllDownloads /> : <FirstRelease />}
      {live && <CheckDownload />}
      <SourceSection />
    </div>
  );
}

/** The dithered panel with the picked system's download card. */
function Hero() {
  const { os } = Route.useLoaderData();
  return (
    <section
      aria-label={os ? `convt for ${osNames[os]}` : "Pick your system"}
      className="dither grid items-center gap-8 overflow-clip rounded-2xl bg-panel p-3 shadow-[inset_0_0_0_1px_var(--line)] sm:p-8 md:min-h-[360px] lg:grid-cols-[480px_minmax(0,1fr)] lg:gap-12"
    >
      <div className="flex w-full max-w-[480px] flex-col gap-5 rounded-xl bg-raised p-5 shadow-note sm:p-6">
        <OsPicker />
        {os ? <HeroBody os={os} /> : <NoDesktop />}
      </div>
      <InstallPreview />
    </section>
  );
}

/**
 * What one install adds, drawn rather than photographed: the right-click menu and the
 * command line tool. Decorative; the page text says the same thing.
 */
function InstallPreview() {
  const menu = "flex h-7 items-center justify-between gap-6 rounded-md px-2.5 text-[13px]/4";
  return (
    <div aria-hidden="true" className="relative hidden h-[300px] select-none lg:block">
      <div className="absolute top-2 left-4 flex w-[210px] flex-col rounded-[10px] bg-raised p-1.5 shadow-note">
        <span className={cx(menu, "text-ink")}>Open</span>
        <span className={cx(menu, "text-ink")}>Copy</span>
        <span className="mx-2.5 my-1 h-px bg-divider" />
        <span className={cx(menu, "bg-[#1f9d5c] font-medium text-white")}>
          Convert to <span aria-hidden="true">›</span>
        </span>
        <span className={cx(menu, "text-ink")}>Rename</span>
      </div>
      <div className="absolute top-[86px] left-[218px] flex w-[150px] flex-col rounded-[10px] bg-raised p-1.5 shadow-note">
        {["WebP", "PNG", "JPEG", "More…"].map((f, i) => (
          <span
            key={f}
            className={cx(
              menu,
              i === 0 ? "bg-hover text-ink" : "text-ink",
              i === 3 && "text-ink-2",
            )}
          >
            {f}
          </span>
        ))}
      </div>
      <div className="absolute right-0 bottom-2 left-[60px] max-w-[380px] rounded-[10px] bg-code px-4 py-3.5 font-mono text-[12px]/5 text-code-ink shadow-note">
        <p>
          <span className="text-[#4cc283]">$</span> convt photo.heic --to webp
        </p>
        <p className="text-[#838985]">photo.webp · 612 KB</p>
      </div>
    </div>
  );
}

/** Links rather than buttons, so each system has its own URL and works before hydration. */
function OsPicker() {
  const { os, detected } = Route.useLoaderData();
  return (
    <nav
      aria-label="System"
      className="flex rounded-lg bg-sunken p-0.5 shadow-[inset_0_0_0_1px_var(--line)]"
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
              "flex h-8 flex-1 items-center justify-center gap-1.5 rounded-md text-[13px]/4 font-medium transition-colors",
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

function HeroBody({ os }: { os: Os }) {
  const { release, detected } = Route.useLoaderData();
  const slots = release.slots.filter((s) => s.os === os);
  const published = slots.filter((s) => s.artifact);
  const [main, ...others] = published;
  const label = os === detected ? "For your computer" : `For ${osNames[os]}`;

  if (!isLive(release)) {
    const first = slots[0];
    return (
      <>
        <div className="flex flex-col gap-2">
          <p className={eyebrow}>{label}</p>
          <h2 className="text-[22px]/7 font-semibold tracking-[-0.02em]">
            convt for {osNames[os]}
          </h2>
          <p className="text-sm/5 text-ink-2">
            {slots.length > 1
              ? `${kindLabels[first.kind].title} and ${slots.length - 1} other formats`
              : `${kindLabels[first.kind].title} · ${kindLabels[first.kind].note}`}
          </p>
          <p className="text-sm/[21px] text-ink-2">
            The download, its checksum and the matching source appear here when the first release
            ships.
          </p>
        </div>
        <ReleaseStatus />
        <div className="flex flex-col gap-2.5 sm:flex-row">
          <PrimaryLink href={RELEASES_URL} className="h-10 gap-2 sm:flex-1">
            Follow releases on GitHub
          </PrimaryLink>
          <SecondaryLink href={routes.pricing} className="h-10 sm:px-4">
            See pricing
          </SecondaryLink>
        </div>
      </>
    );
  }

  if (!main)
    return (
      <div className="flex flex-col gap-2">
        <p className={eyebrow}>{label}</p>
        <h2 className="text-[22px]/7 font-semibold tracking-[-0.02em]">convt for {osNames[os]}</h2>
        <p className="text-sm/[21px] text-ink-2">
          Version {release.version} has no {osNames[os]} build. It will be listed here as soon as
          one is published; <TextLink href={RELEASES_URL}>follow releases on GitHub</TextLink> to
          hear first.
        </p>
      </div>
    );

  const artifact = main.artifact!;
  const file = fileName(artifact.url);
  return (
    <>
      <div className="flex flex-col gap-1.5">
        <p className={eyebrow}>{label}</p>
        <h2 className="text-[22px]/7 font-semibold tracking-[-0.02em]">convt for {osNames[os]}</h2>
        <p className="text-sm/5 text-ink-2">
          {kindLabels[main.kind].title} · {archLabel(main)} · {kindLabels[main.kind].note}
        </p>
      </div>
      <div className="flex flex-col gap-2">
        <PrimaryLink href={artifact.url} download={file} className="h-11 gap-2 text-[15px]/5">
          <DownloadIcon />
          Download for {osNames[os]}
        </PrimaryLink>
        <p className="flex min-w-0 justify-between gap-3 font-mono text-[11.5px]/4 text-ink-2">
          <span className="min-w-0 truncate" title={file}>
            {file}
          </span>
          <span className="shrink-0">{formatBytes(artifact.size)}</span>
        </p>
      </div>
      <Sha value={artifact.sha256} block />
      {others.length > 0 && (
        <p className="text-[13px]/5 text-ink-2">
          Also for {osNames[os]}:{" "}
          {others.map((slot, i) => (
            <span key={slot.kind}>
              {i > 0 && " · "}
              <TextLink href={`#file-${slot.kind}`} className="font-mono text-xs">
                {slot.kind === "AppImage" ? slot.kind : `.${slot.kind}`}
              </TextLink>
            </span>
          ))}
        </p>
      )}
    </>
  );
}

/** Pre-release: a quiet status line in the hero card instead of a disabled button. */
function ReleaseStatus() {
  return (
    <div className="flex items-start gap-3 rounded-lg bg-sunken px-3.5 py-3 shadow-[inset_0_0_0_1px_var(--line)]">
      <span className="relative mt-1.5 flex size-2 shrink-0">
        <span className="absolute inline-flex size-full animate-ping rounded-full bg-green opacity-50 motion-reduce:hidden" />
        <span className="relative inline-flex size-2 rounded-full bg-green" />
      </span>
      <p className="text-[13px]/5 text-ink-2">
        <span className="font-medium text-ink">First release in preparation.</span> Each build
        appears on this page as soon as it is published.
      </p>
    </div>
  );
}

// Phones and unknown systems: convt is desktop software, so point at the web options.
function NoDesktop() {
  return (
    <div className="flex flex-col gap-2">
      <p className={eyebrow}>Desktop app</p>
      <h2 className="text-[22px]/7 font-semibold tracking-[-0.02em]">
        convt runs on your computer
      </h2>
      <p className="text-sm/[21px] text-ink-2">
        Pick a system above to see its download. On a phone, convt Pro converts files in the
        browser; <TextLink href={routes.pricing}>see pricing</TextLink>.
      </p>
    </div>
  );
}

/** Pre-release: what the first release contains, as a spec sheet rather than empty buttons. */
function FirstRelease() {
  const { release, detected } = Route.useLoaderData();
  return (
    <section
      aria-labelledby="first-title"
      id="platforms"
      className="flex scroll-mt-6 flex-col gap-6"
    >
      <div className="flex max-w-[680px] flex-col gap-2">
        <h2 id="first-title" className={sectionTitle}>
          In the first release
        </h2>
        <p className="text-[15px]/6 text-ink-2">
          One build per system, each listed with its file size and SHA-256 checksum.
        </p>
      </div>
      <div className="flex flex-col overflow-clip rounded-2xl bg-raised shadow-[inset_0_0_0_1px_var(--line)] dark:bg-panel">
        {osOrder.map((os, i) => (
          <div
            key={os}
            id={os}
            className={cx(
              "grid scroll-mt-6 gap-4 p-5 sm:p-6 md:grid-cols-[180px_minmax(0,1fr)] md:gap-8",
              i > 0 && "border-t border-line",
            )}
          >
            <div className="flex items-center justify-between gap-3 md:flex-col md:items-start md:justify-start md:gap-1.5">
              <h3 className="text-lg/6 font-semibold">{osNames[os]}</h3>
              {os === detected && <YourSystem />}
            </div>
            <ul className="grid gap-x-8 gap-y-3 sm:grid-cols-2">
              {release.slots
                .filter((s) => s.os === os)
                .map((slot) => (
                  <li key={slot.kind} className="flex flex-col gap-0.5">
                    <span className="text-sm/5 font-medium">{kindLabels[slot.kind].title}</span>
                    <span className="text-[13px]/[18px] text-ink-2">
                      {archLabel(slot)} · {kindLabels[slot.kind].note}
                    </span>
                  </li>
                ))}
            </ul>
          </div>
        ))}
      </div>
      <dl className="grid gap-6 border-t border-line pt-6 sm:grid-cols-3 sm:gap-8">
        <div className="flex flex-col gap-1.5">
          <dt className={eyebrow}>Checksums</dt>
          <dd className="text-sm/[21px] text-ink-2">
            Every file lists its SHA-256, so you can confirm the download is intact.
          </dd>
        </div>
        <div className="flex flex-col gap-1.5">
          <dt className={eyebrow}>Source</dt>
          <dd className="text-sm/[21px] text-ink-2">
            Each release ships a source archive built from the same commit.
          </dd>
        </div>
        <div className="flex flex-col gap-1.5">
          <dt className={eyebrow}>Release notes</dt>
          <dd className="text-sm/[21px] text-ink-2">
            What changed in each version is on the{" "}
            <TextLink href={routes.changelog}>changelog</TextLink>.
          </dd>
        </div>
      </dl>
    </section>
  );
}

function YourSystem() {
  return (
    <span className="inline-flex items-center gap-1.5 font-mono text-[11px]/3.5 text-[#157f4a] dark:text-green">
      <span className="size-1.5 rounded-full bg-green" aria-hidden="true" />
      YOUR SYSTEM
    </span>
  );
}

/** Live: every published file, grouped by system, the picked system first. */
function AllDownloads() {
  const { release, os, detected } = Route.useLoaderData();
  const order = os ? [os, ...osOrder.filter((o) => o !== os)] : osOrder;
  return (
    <section aria-labelledby="all-title" id="platforms" className="flex scroll-mt-6 flex-col gap-6">
      <div className="flex flex-col gap-2 sm:flex-row sm:items-end sm:justify-between">
        <h2 id="all-title" className={sectionTitle}>
          All downloads
        </h2>
        <p className="text-sm/5 text-ink-2">
          Version {release.version}
          {release.date && ` · ${formatDate(release.date)}`} ·{" "}
          <TextLink href={routes.changelog}>Release notes</TextLink>
        </p>
      </div>
      <div className="flex flex-col overflow-clip rounded-2xl bg-raised shadow-[inset_0_0_0_1px_var(--line)] dark:bg-panel">
        {order.map((platform, i) => {
          const published = release.slots.filter((s) => s.os === platform && s.artifact);
          return (
            <div
              key={platform}
              id={platform}
              className={cx("flex scroll-mt-6 flex-col", i > 0 && "border-t border-line")}
            >
              <div className="flex items-center justify-between gap-3 bg-sunken/60 px-5 py-3 sm:px-6">
                <h3 className="text-[15px]/5 font-semibold">{osNames[platform]}</h3>
                {platform === detected && <YourSystem />}
              </div>
              {published.length === 0 ? (
                <p className="border-t border-divider px-5 py-4 text-sm/5 text-ink-2 sm:px-6">
                  No {osNames[platform]} build in version {release.version}.{" "}
                  <TextLink href={RELEASES_URL} className="font-normal">
                    Follow releases
                  </TextLink>
                </p>
              ) : (
                <ul className="flex flex-col">
                  {published.map((slot) => (
                    <FileRow key={slot.kind} slot={slot} />
                  ))}
                </ul>
              )}
            </div>
          );
        })}
      </div>
    </section>
  );
}

function FileRow({ slot }: { slot: Slot }) {
  const artifact = slot.artifact!;
  const file = fileName(artifact.url);
  return (
    <li
      id={`file-${slot.kind}`}
      className="grid scroll-mt-6 gap-3 border-t border-divider px-5 py-4 sm:grid-cols-[minmax(0,1fr)_auto] sm:items-center sm:gap-x-6 sm:px-6"
    >
      <div className="flex min-w-0 flex-col gap-1">
        <div className="flex flex-wrap items-baseline gap-x-2 gap-y-0.5">
          <span className="text-sm/5 font-medium">{kindLabels[slot.kind].title}</span>
          <span className="text-[13px]/[18px] text-ink-2">
            {archLabel(slot)} · {formatBytes(artifact.size)} · {kindLabels[slot.kind].note}
          </span>
        </div>
        <span className="truncate font-mono text-[11.5px]/4 text-ink" title={file}>
          {file}
        </span>
        <Sha value={artifact.sha256} />
      </div>
      <SecondaryLink
        href={artifact.url}
        download={file}
        className="h-9 gap-1.5 justify-self-start px-3.5 sm:justify-self-end"
      >
        <DownloadIcon className="size-3.5" />
        Download
      </SecondaryLink>
    </li>
  );
}

const verifyCommands: Record<Os, { label: string; command: (file: string) => string }> = {
  macos: { label: "macOS (Terminal)", command: (f) => `shasum -a 256 ${f}` },
  windows: { label: "Windows (PowerShell)", command: (f) => `Get-FileHash ${f}` },
  linux: { label: "Linux", command: (f) => `sha256sum ${f}` },
};

/** Live only: how to compare a checksum, with the real file names filled in. */
function CheckDownload() {
  const { release, os } = Route.useLoaderData();
  const order = os ? [os, ...osOrder.filter((o) => o !== os)] : osOrder;
  const rows = order.flatMap((platform) => {
    const slot = release.slots.find((s) => s.os === platform && s.artifact);
    return slot ? [{ platform, file: fileName(slot.artifact!.url) }] : [];
  });
  return (
    <section aria-labelledby="verify-title" className="grid gap-8 md:grid-cols-2 md:gap-12">
      <div className="flex flex-col gap-3">
        <h2 id="verify-title" className={sectionTitle}>
          Check your download
        </h2>
        <p className="text-[15px]/6 text-ink-2">
          Run the command for your system in the folder you saved the file to, then compare the
          result with the SHA-256 listed above. If they differ, delete the file and download it
          again.
        </p>
        <p className="text-[15px]/6 text-ink-2">
          Documents (Word, Excel and PowerPoint files) use an optional document pack. The app offers
          it the first time you select one, and downloads it only when you click Install.
        </p>
      </div>
      <dl className="flex min-w-0 flex-col gap-3 font-mono text-[12.5px]/5">
        {rows.map(({ platform, file }) => (
          <div key={platform} className="flex flex-col gap-1.5">
            <dt className="text-xs/4 text-ink-2">{verifyCommands[platform].label}</dt>
            <dd className="overflow-x-auto rounded-lg bg-code px-3.5 py-2.5 whitespace-nowrap text-code-ink shadow-[inset_0_0_0_1px_var(--code-ring)]">
              {verifyCommands[platform].command(file)}
            </dd>
          </div>
        ))}
      </dl>
    </section>
  );
}

function SourceSection() {
  const { release } = Route.useLoaderData();
  const source = release.source;
  return (
    <section
      aria-labelledby="source-title"
      className="flex flex-col gap-6 border-t border-line pt-12"
    >
      <div className="flex max-w-[680px] flex-col gap-3">
        <h2 id="source-title" className={sectionTitle}>
          Source code
        </h2>
        <p className="text-[15px]/6 text-ink-2">
          convt is free software under the{" "}
          <TextLink href={`${GITHUB_URL}/blob/main/LICENSE`}>GNU AGPL-3.0</TextLink>.{" "}
          {source
            ? "This archive is built from the same commit as the downloads above, with the build scripts and the source of every bundled component."
            : "The code is public today. Each release will also publish a source archive built from the same commit as its downloads."}
        </p>
      </div>
      <div className="flex flex-col gap-3 rounded-2xl bg-raised px-5 py-4 shadow-[inset_0_0_0_1px_var(--line)] sm:flex-row sm:items-center sm:justify-between sm:gap-6 dark:bg-panel">
        <div className="flex min-w-0 flex-col gap-1">
          <span className="truncate text-sm/5 font-medium">
            {source ? fileName(source.url) : "opencoredev/convt"}
          </span>
          <span className="text-[13px]/[18px] text-ink-2">
            {source
              ? `Version ${release.version} · ${formatBytes(source.size)}`
              : "Repository, issues and release history on GitHub"}
          </span>
          {source && <Sha value={source.sha256} />}
        </div>
        {source ? (
          <SecondaryLink
            href={source.url}
            download={fileName(source.url)}
            className="h-9 shrink-0 gap-1.5 self-start px-3.5 sm:self-auto"
          >
            <DownloadIcon className="size-3.5" />
            Download source
          </SecondaryLink>
        ) : (
          <SecondaryLink href={GITHUB_URL} className="h-9 shrink-0 self-start px-3.5 sm:self-auto">
            View on GitHub
          </SecondaryLink>
        )}
      </div>
    </section>
  );
}
