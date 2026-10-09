import { createFileRoute, redirect } from "@tanstack/react-router";
import { createServerFn } from "@tanstack/react-start";

import { Checksums, DownloadStage, NextSteps, Platforms } from "#/components/site/download";
import { siteColumn } from "#/components/site/layout";
import { cx } from "#/components/app/ui";
import { isOs, releaseFromManifest, type Os } from "#/lib/platform";
import { parseReleaseManifest } from "#/lib/release-manifest";
import { downloadGate } from "#/lib/sign-in";
import { fetchLatestManifest } from "#/server/latest-release";
import { getSession } from "#/server/session";
import { visitorOs } from "#/server/visitor-os";
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

export const Route = createFileRoute("/_site/download")({
  validateSearch: (search: Record<string, unknown>): { os?: Os } =>
    isOs(search.os) ? { os: search.os } : {},
  // Downloads need an account, so a new user meets sign-up first (CNV-69).
  beforeLoad: async ({ location }) => {
    const session = await getSession();
    const gate = downloadGate(session ? session.user : null, location.href);
    if (gate) throw redirect({ href: gate });
  },
  loaderDeps: ({ search }) => ({ os: search.os }),
  // The User-Agent picks the build unless the link names one.
  loader: async ({ deps }) => ({
    os: deps.os ?? (await visitorOs()),
    release: await loadRelease(),
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
  const { os, release } = Route.useLoaderData();
  const published = release.slots.some((s) => s.artifact);
  return (
    <>
      <DownloadStage os={os} release={release} />
      <div className={cx(siteColumn, "flex flex-col gap-16 pt-14 pb-20 md:gap-20 md:pt-16")}>
        {!published && (
          <p className="rounded-xl bg-sunken px-4 py-3 text-sm/[21px] text-ink-2 shadow-[inset_0_0_0_1px_var(--line)]">
            The first signed builds are on their way. Each download appears here as soon as it is
            published.
          </p>
        )}
        <NextSteps os={os} />
        <Platforms os={os} release={release} />
        <Checksums release={release} />
      </div>
    </>
  );
}
