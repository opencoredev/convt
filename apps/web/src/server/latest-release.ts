// The newest published release, read from GitHub when the download page loads, so
// merging the Version Packages PR is enough to put a release on the site. The release
// workflow uploads the unsigned manifest as `release-manifest.json` on every release;
// GitHub's /releases/latest/download/ redirect always points at the newest one.

import { parseReleaseManifest, type ReleaseManifest } from "#/lib/release-manifest";
import { GITHUB_URL } from "#/lib/site";

export const LATEST_MANIFEST_URL = `${GITHUB_URL}/releases/latest/download/release-manifest.json`;
/** The signed update manifest the desktop app polls through convt.app/updates/manifest.json. */
export const LATEST_UPDATE_MANIFEST_URL = `${GITHUB_URL}/releases/latest/download/update-manifest.json`;

const assetPrefix = `${GITHUB_URL}/releases/download/`;

/** The latest release manifest, or null when there is none yet or it can't be read. */
export async function fetchLatestManifest(): Promise<ReleaseManifest | null> {
  try {
    const response = await fetch(LATEST_MANIFEST_URL, {
      redirect: "follow",
      cf: { cacheTtl: 300, cacheEverything: true },
    } as RequestInit);
    if (!response.ok) return null;
    const manifest = parseReleaseManifest(await response.json());
    // Only link files from this repository's releases.
    const urls = manifest.builds.flatMap((b) => [...b.artifacts, b.source].map((a) => a.url));
    if (!urls.every((u) => u.startsWith(assetPrefix))) return null;
    return manifest;
  } catch (e) {
    console.error("[download] latest release manifest:", e instanceof Error ? e.message : e);
    return null;
  }
}
