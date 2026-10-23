import { links } from "./config";
import {
  latestBuild,
  type ArtifactKind,
  type ManifestArtifact,
  type Platform,
  type ReleaseManifest,
} from "./release-manifest";

export type Os = "macos" | "windows" | "linux";

export const osNames: Record<Os, string> = {
  macos: "macOS",
  windows: "Windows",
  linux: "Linux",
};

export const osOrder: Os[] = ["macos", "windows", "linux"];

const osPlatform: Record<Os, Exclude<Platform, "source">> = {
  macos: "macos-arm64",
  windows: "windows-x86_64",
  linux: "linux-x86_64",
};

export const kindLabels: Record<ArtifactKind, { title: string; note: string }> = {
  dmg: { title: "Disk image (.dmg)", note: "Apple silicon" },
  exe: { title: "Installer (.exe)", note: "64-bit Windows" },
  msi: { title: "Installer (.msi)", note: "64-bit Windows, unsigned" },
  zip: { title: "Archive (.zip)", note: "No installer" },
  AppImage: { title: "AppImage", note: "Runs on most distributions" },
  deb: { title: "Debian package (.deb)", note: "Debian, Ubuntu and derivatives" },
  rpm: { title: "RPM package (.rpm)", note: "Fedora, openSUSE and derivatives" },
  "tar.gz": { title: "Archive (.tar.gz)", note: "Unpack anywhere" },
};

/** The downloads each OS always lists, first one offered first. Missing ones say "Shipping today". */
const expected: Record<Os, ArtifactKind[]> = {
  macos: ["dmg"],
  windows: ["msi"],
  linux: ["AppImage", "deb", "rpm", "tar.gz"],
};

export type Slot = {
  os: Os;
  kind: ArtifactKind;
  arch: string;
  /** Null until the build is published and distribution_ready is true. */
  artifact: ManifestArtifact | null;
};

export type Release = {
  version: string | null;
  date: string | null;
  slots: Slot[];
  source: ManifestArtifact | null;
};

/**
 * What the download page offers. Nothing is linked unless the manifest marks the
 * release distribution_ready, so an unpublishable build never shows up as a link.
 */
export function releaseFromManifest(manifest: ReleaseManifest | null): Release {
  const build = manifest ? latestBuild(manifest) : null;
  const ready = manifest?.distribution_ready === true && build != null;
  const slots = osOrder.flatMap((os) => {
    const platform = osPlatform[os];
    const published = ready ? build.artifacts.filter((a) => a.platform === platform) : [];
    const kinds = [
      ...expected[os],
      ...published.map((a) => a.kind).filter((k) => !expected[os].includes(k)),
    ];
    return kinds.map((kind) => ({
      os,
      kind,
      arch: platform.split("-")[1],
      artifact: published.find((a) => a.kind === kind) ?? null,
    }));
  });
  return {
    version: build?.version ?? null,
    date: ready ? build.build_date : null,
    slots,
    source: ready ? build.source : null,
  };
}

/** Best guess at the visitor's OS from a User-Agent string; null for phones and unknowns. */
export function osFromUserAgent(ua: string): Os | null {
  if (/iPhone|iPad|iPod|Android/i.test(ua)) return null;
  if (/Mac OS X|Macintosh/i.test(ua)) return "macos";
  if (/Windows/i.test(ua)) return "windows";
  if (/Linux|X11|CrOS/i.test(ua)) return "linux";
  return null;
}

/** Primary download button copy: OS-specific when known, otherwise just "Download". */
export function downloadCtaLabel(os: Os | null): string {
  return os ? `Download for ${osNames[os]}` : "Download";
}

/** The first "what's next" step after checkout: names the visitor's OS when known. */
export function downloadStepLabel(os: Os | null): string {
  return os ? `Download convt for ${osNames[os]}` : "Download convt";
}

/** The download page, preselecting the visitor's OS when known. */
export function downloadHref(os: Os | null): string {
  return os ? `${links.download}?os=${os}` : links.download;
}

export function isOs(value: unknown): value is Os {
  return value === "macos" || value === "windows" || value === "linux";
}
