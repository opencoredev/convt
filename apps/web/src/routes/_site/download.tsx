import { createFileRoute } from "@tanstack/react-router";
import { createServerFn } from "@tanstack/react-start";
import { getRequestHeader } from "@tanstack/react-start/server";

import { cx } from "#/components/app/ui";
import {
  MobileEmailNote,
  useMobileDownloadIntercept,
} from "#/components/mobile-download/mobile-download";
import { DownloadButton, Sha } from "#/components/site/download";
import { PageHeader, TextLink, siteColumn } from "#/components/site/layout";
import {
  isOs,
  kindLabels,
  osFromUserAgent,
  osNames,
  osOrder,
  releaseFromManifest,
  type Os,
  type Slot,
} from "#/lib/platform";
import { isMobileUserAgent } from "#/lib/mobile";
import { formatBytes, parseReleaseManifest } from "#/lib/release-manifest";
import { fetchLatestManifest } from "#/server/latest-release";
import { routes, seo } from "#/lib/site";

// The newest GitHub release's manifest (packaging/release/manifest.schema.json), read
// on each load. content/release-manifest.json is the fallback when GitHub has none or
// can't be reached; without either, every download says "Shipping today". The glob
// resolves at build time; scripts/generate-content.ts validates the file.
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

const detectMobile = createServerFn({ method: "GET" }).handler(() =>
  isMobileUserAgent(getRequestHeader("user-agent") ?? ""),
);

export const Route = createFileRoute("/_site/download")({
  validateSearch: (search: Record<string, unknown>): { os?: Os } =>
    isOs(search.os) ? { os: search.os } : {},
  loaderDeps: ({ search }) => ({ os: search.os }),
  // The User-Agent picks the build on the first load; client navigations read it locally.
  // Check `window`, not `navigator`: Workers define navigator with their own user agent.
  loader: async ({ deps }) => ({
    os:
      deps.os ??
      (typeof window === "undefined" ? await detectOs() : osFromUserAgent(navigator.userAgent)),
    release: await loadRelease(),
    mobile:
      typeof window === "undefined"
        ? await detectMobile()
        : isMobileUserAgent(navigator.userAgent, navigator.maxTouchPoints),
  }),
  head: () =>
    seo({
      title: "Download convt for macOS, Windows and Linux",
      description:
        "convt for macOS (Apple silicon), Windows and Linux (AppImage, .deb, .rpm, tarball).",
      path: routes.download,
    }),
  component: DownloadPage,
});

function DownloadPage() {
  const { os, release, mobile } = Route.useLoaderData();
  const published = release.slots.some((s) => s.artifact);
  // On a phone, an installer tap opens the download-link card instead.
  const intercept = useMobileDownloadIntercept("download");
  return (
    <div
      className={cx(siteColumn, "flex flex-col gap-16 pt-12 pb-20 md:gap-20 md:pt-16")}
      onClickCapture={(event) => {
        if (event.target instanceof Element && event.target.closest("a[download]"))
          intercept.onClick(event);
      }}
    >
      <div className="flex flex-col gap-6">
        <PageHeader
          eyebrow={release.version ? `Version ${release.version}` : "Download"}
          title="Download convt"
        >
          <p>
            The app, the right-click menu and the{" "}
            <code className="font-mono text-[15px]">convt</code> command line tool in one install.
            Every download starts a 7-day free trial; no account needed.
          </p>
        </PageHeader>
        {!published && (
          <p className="max-w-[680px] rounded-xl bg-sunken px-4 py-3 text-sm/[21px] text-ink-2 shadow-[inset_0_0_0_1px_var(--line)]">
            The first signed builds are on their way. Each download appears here as soon as it is
            published.
          </p>
        )}
      </div>

      {mobile && <MobileEmailNote source="download" />}
      <div className={mobile ? "max-[1024px]:hidden" : undefined}>
        {os ? <Recommended os={os} /> : <NoDesktop />}
      </div>

      <section
        aria-labelledby="platforms-title"
        id="platforms"
        className="flex scroll-mt-6 flex-col gap-6"
      >
        <h2 id="platforms-title" className="text-2xl/8 font-semibold tracking-[-0.02em]">
          All platforms
        </h2>
        <div className="grid items-start gap-4 lg:grid-cols-3">
          {osOrder.map((platform) => (
            <PlatformCard key={platform} os={platform} current={platform === os} />
          ))}
        </div>
      </section>
      {intercept.dialog}
    </div>
  );
}

function slotMeta(slot: Slot) {
  const arch = slot.arch === "arm64" && slot.os === "macos" ? "Apple silicon" : slot.arch;
  return slot.artifact
    ? `${arch} · ${formatBytes(slot.artifact.size)}`
    : `${arch} · ${kindLabels[slot.kind].note}`;
}

function Recommended({ os }: { os: Os }) {
  const { release } = Route.useLoaderData();
  const slot = release.slots.find((s) => s.os === os);
  return (
    <section
      aria-label={`Download for ${osNames[os]}`}
      className="dither flex flex-col justify-end overflow-clip rounded-2xl bg-panel p-4 shadow-[inset_0_0_0_1px_var(--line)] sm:p-8 md:min-h-[320px]"
    >
      <div className="flex w-full max-w-[460px] flex-col gap-5 rounded-xl bg-raised p-5 shadow-note sm:p-6">
        <div className="flex flex-col gap-1.5">
          <p className="font-mono text-[11px]/3.5 text-ink-2 uppercase">For your computer</p>
          <h2 className="text-[22px]/7 font-semibold tracking-[-0.02em]">
            convt for {osNames[os]}
          </h2>
          {slot && (
            <p className="text-sm/5 text-ink-2">
              {kindLabels[slot.kind].title} · {slotMeta(slot)}
            </p>
          )}
        </div>
        <DownloadButton artifact={slot?.artifact ?? null} large />
        {slot?.artifact && <Sha value={slot.artifact.sha256} />}
        <p className="text-[13px]/5 text-ink-2">
          Not your system?{" "}
          <TextLink href="#platforms" className="font-normal">
            See all platforms
          </TextLink>
        </p>
      </div>
    </section>
  );
}

// Phones and unknown systems: convt is desktop software, so point at the web options.
function NoDesktop() {
  return (
    <section className="flex flex-col gap-3 rounded-2xl bg-sunken p-6 shadow-[inset_0_0_0_1px_var(--line)]">
      <h2 className="text-lg/6 font-semibold">convt runs on macOS, Windows and Linux</h2>
      <p className="max-w-[560px] text-[15px]/6 text-ink-2">
        Pick your computer's system below. On a phone, convt Pro converts files in the browser;{" "}
        <TextLink href={routes.pricing}>see pricing</TextLink>.
      </p>
    </section>
  );
}

function PlatformCard({ os, current }: { os: Os; current: boolean }) {
  const { release } = Route.useLoaderData();
  const slots = release.slots.filter((s) => s.os === os);
  return (
    <section
      aria-labelledby={`os-${os}`}
      id={os}
      className={cx(
        "flex scroll-mt-6 flex-col gap-4 rounded-2xl bg-raised p-5 dark:bg-panel",
        current
          ? "shadow-[inset_0_0_0_1px_var(--green-line)]"
          : "shadow-[inset_0_0_0_1px_var(--line)]",
      )}
    >
      <div className="flex items-baseline justify-between gap-3">
        <h3 id={`os-${os}`} className="text-lg/6 font-semibold">
          {osNames[os]}
        </h3>
        {current && (
          <span className="font-mono text-[11px]/3.5 text-[#157f4a] dark:text-green">
            YOUR SYSTEM
          </span>
        )}
      </div>
      <ul className="flex flex-col divide-y divide-divider">
        {slots.map((slot) => (
          <li key={slot.kind} className="flex flex-col gap-2.5 py-3.5 first:pt-0 last:pb-0">
            <div className="flex items-start justify-between gap-3">
              <div className="flex min-w-0 flex-col gap-0.5">
                <span className="text-sm/5 font-medium">{kindLabels[slot.kind].title}</span>
                <span className="text-[13px]/[18px] text-ink-2">{slotMeta(slot)}</span>
              </div>
              <DownloadButton artifact={slot.artifact} />
            </div>
            {slot.artifact && <Sha value={slot.artifact.sha256} />}
          </li>
        ))}
      </ul>
    </section>
  );
}
