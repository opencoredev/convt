import { createFileRoute } from "@tanstack/react-router";
import { createServerFn } from "@tanstack/react-start";
import { getRequestHeader } from "@tanstack/react-start/server";

import { cx } from "#/components/app/ui";
import { DownloadButton } from "#/components/site/download";
import { InstallGuide } from "#/components/site/install-guide";
import { TextLink, siteColumn } from "#/components/site/layout";
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
import { primarySlot } from "#/lib/install-guide";
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
  }),
  head: () =>
    seo({
      title: "Download convt for macOS, Windows and Linux",
      description:
        "Get convt for macOS, Windows and Linux. One install covers the app, the right-click menu and the command line.",
      path: routes.download,
    }),
  component: DownloadPage,
});

function DownloadPage() {
  const { os, release } = Route.useLoaderData();
  const published = release.slots.some((s) => s.artifact);
  const primary = os ? primarySlot(release.slots, os) : undefined;
  return (
    <div className={cx(siteColumn, "flex flex-col gap-16 pt-12 pb-24 md:gap-20 md:pt-20")}>
      <div className="flex flex-col items-center gap-8 text-center">
        <div className="flex max-w-[640px] flex-col items-center gap-3">
          {release.version ? (
            <p className="font-mono text-xs/4 text-ink-2 uppercase">Version {release.version}</p>
          ) : (
            <p className="font-mono text-xs/4 text-ink-2 uppercase">Download</p>
          )}
          <h1 className="text-[34px]/10 font-semibold tracking-[-0.03em] text-balance md:text-[44px]/12">
            Download convt
          </h1>
          <p className="text-[17px]/[26px] text-ink-2">
            The app, the right-click menu and the{" "}
            <code className="font-mono text-[15px]">convt</code> command line tool in one install.
            Every download starts a 7-day free trial; no account needed.
          </p>
        </div>
        {!published && (
          <p className="max-w-[560px] rounded-xl bg-sunken px-4 py-3 text-sm/[21px] text-ink-2 shadow-[inset_0_0_0_1px_var(--line)]">
            The first signed builds are on their way. Each download appears here as soon as it is
            published.
          </p>
        )}
        {os && primary ? <PrimaryDownload os={os} primary={primary} /> : <ChooseSystem />}
        <OsSwitcher current={os} />
      </div>

      {os && primary && <InstallGuide os={os} kind={primary.kind} />}
    </div>
  );
}

function PrimaryDownload({ os, primary }: { os: Os; primary: Slot }) {
  const { release } = Route.useLoaderData();
  const extras = release.slots.filter((s) => s.os === os && s.kind !== primary.kind && s.artifact);
  return (
    <div className="flex w-full max-w-[400px] flex-col items-center gap-3">
      <DownloadButton artifact={primary.artifact} large label={`Download for ${osNames[os]}`} />
      <p className="text-sm/5 text-ink-2">
        {kindLabels[primary.kind].title}
        {primary.artifact
          ? ` · ${formatBytes(primary.artifact.size)}`
          : ` · ${kindLabels[primary.kind].note}`}
      </p>
      {extras.length > 0 && (
        <p className="text-[13px]/5 text-ink-2">
          Also{" "}
          {extras.map((slot, index) => {
            const artifact = slot.artifact;
            if (!artifact) return null;
            return (
              <span key={slot.kind}>
                {index > 0 && ", "}
                <TextLink href={artifact.url} className="font-normal">
                  {kindLabels[slot.kind].title}
                </TextLink>
              </span>
            );
          })}
        </p>
      )}
    </div>
  );
}

function ChooseSystem() {
  return (
    <p className="max-w-[520px] text-[15px]/6 text-ink-2">
      convt runs on a computer. Choose your system to see the download and install steps. On a
      phone, convt Pro converts files in the browser;{" "}
      <TextLink href={routes.pricing}>see pricing</TextLink>.
    </p>
  );
}

function OsSwitcher({ current }: { current: Os | null }) {
  return (
    <p className="text-[13px]/5 text-ink-2">
      {current ? "Not your system? " : "Choose a system: "}
      {osOrder.map((os, index) => (
        <span key={os}>
          {index > 0 && <span aria-hidden="true"> · </span>}
          {os === current ? (
            <span className="font-medium text-ink">{osNames[os]}</span>
          ) : (
            <TextLink href={`${routes.download}?os=${os}`} className="font-normal">
              {osNames[os]}
            </TextLink>
          )}
        </span>
      ))}
    </p>
  );
}
