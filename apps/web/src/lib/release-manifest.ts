// The release manifest the download page reads. The release pipeline defines it in
// packaging/release/manifest.schema.json and writes it with
// `scripts/release/manifest.ts generate`; copy the unsigned release-manifest.json to
// apps/web/content/release-manifest.json to publish a release on the site. No file
// means no release yet. See apps/web/README.md.
// This module has no dependencies so release scripts can import it with Bun.

export type Platform = "linux-x86_64" | "macos-arm64" | "windows-x86_64" | "source";
export type ArtifactKind = "tar.gz" | "AppImage" | "deb" | "rpm" | "dmg" | "zip" | "msi" | "exe";

export type ManifestArtifact = {
  platform: Platform;
  kind: ArtifactKind;
  /** Absolute https URL. */
  url: string;
  /** Size in bytes. */
  size: number;
  /** Lowercase hex SHA-256. */
  sha256: string;
};

export type ManifestBuild = {
  version: string;
  /** YYYY-MM-DD */
  build_date: string;
  artifacts: ManifestArtifact[];
  /** convt's source for this exact build, which the AGPL requires next to the binaries. */
  source: ManifestArtifact;
};

export type ReleaseManifest = {
  schema_version: 1;
  sequence: number;
  /** Unix seconds. */
  issued_at: number;
  expires_at: number;
  /** False until the source and notice gates pass; the site then links nothing. */
  distribution_ready: boolean;
  purchase_url: string;
  builds: ManifestBuild[];
};

const platforms: Platform[] = ["linux-x86_64", "macos-arm64", "windows-x86_64", "source"];
const kinds: ArtifactKind[] = ["tar.gz", "AppImage", "deb", "rpm", "dmg", "zip", "msi", "exe"];

export function isArtifactKind(value: unknown): value is ArtifactKind {
  return typeof value === "string" && (kinds as readonly string[]).includes(value);
}

function fail(path: string, message: string): never {
  throw new Error(`release manifest: ${path} ${message}`);
}

function checkArtifact(path: string, a: Record<string, unknown>) {
  if (typeof a !== "object" || a === null) fail(path, "is not an object");
  if (!platforms.includes(a.platform as Platform))
    fail(path, `has unknown platform ${String(a.platform)}`);
  if (!kinds.includes(a.kind as ArtifactKind)) fail(path, `has unknown kind ${String(a.kind)}`);
  if (typeof a.url !== "string" || !a.url.startsWith("https://")) fail(path, "url must be https");
  if (!Number.isSafeInteger(a.size) || (a.size as number) < 1)
    fail(path, "size must be a positive integer");
  if (typeof a.sha256 !== "string" || !/^[a-f0-9]{64}$/.test(a.sha256))
    fail(path, "sha256 must be 64 lowercase hex characters");
}

/** Validates JSON against packaging/release/manifest.schema.json. Throws with the path. */
export function parseReleaseManifest(input: unknown): ReleaseManifest {
  if (typeof input !== "object" || input === null) fail("", "is not an object");
  const m = input as Record<string, unknown>;
  if (m.schema_version !== 1) fail("schema_version", "must be 1");
  if (!Number.isSafeInteger(m.sequence) || (m.sequence as number) < 1)
    fail("sequence", "must be a positive integer");
  if (!Number.isSafeInteger(m.issued_at)) fail("issued_at", "must be Unix seconds");
  if (!Number.isSafeInteger(m.expires_at)) fail("expires_at", "must be Unix seconds");
  if (typeof m.distribution_ready !== "boolean") fail("distribution_ready", "must be a boolean");
  if (typeof m.purchase_url !== "string" || !m.purchase_url.startsWith("https://"))
    fail("purchase_url", "must be https");
  if (!Array.isArray(m.builds) || m.builds.length === 0)
    fail("builds", "must be a non-empty array");
  m.builds.forEach((b: Record<string, unknown>, i: number) => {
    const path = `builds[${i}]`;
    if (typeof b.version !== "string" || !/^\d+\.\d+\.\d+$/.test(b.version))
      fail(path, "version must be x.y.z");
    if (typeof b.build_date !== "string" || !/^\d{4}-\d{2}-\d{2}$/.test(b.build_date))
      fail(path, "build_date must be YYYY-MM-DD");
    if (!Array.isArray(b.artifacts) || b.artifacts.length === 0) fail(path, "needs artifacts");
    b.artifacts.forEach((a: Record<string, unknown>, j: number) =>
      checkArtifact(`${path}.artifacts[${j}]`, a),
    );
    checkArtifact(`${path}.source`, b.source as Record<string, unknown>);
  });
  return m as unknown as ReleaseManifest;
}

const versionKey = (v: string) =>
  v
    .split(".")
    .map((n) => n.padStart(6, "0"))
    .join(".");

/** The newest build in the manifest. */
export function latestBuild(manifest: ReleaseManifest) {
  return [...manifest.builds].sort((a, b) =>
    versionKey(b.version).localeCompare(versionKey(a.version)),
  )[0];
}

export function fileName(url: string) {
  return decodeURIComponent(new URL(url).pathname.split("/").pop() ?? url);
}

/** "4.8 MB" style sizes, decimal units like the OS file managers use. */
export function formatBytes(bytes: number) {
  const units = ["B", "KB", "MB", "GB"];
  let value = bytes;
  let unit = 0;
  while (value >= 1000 && unit < units.length - 1) {
    value /= 1000;
    unit++;
  }
  return `${unit === 0 ? value : value.toFixed(1)} ${units[unit]}`;
}
