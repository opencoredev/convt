import { describe, expect, test } from "bun:test";

import { parseMarkdown } from "#/components/site/markdown";
import { downloadCtaLabel, osFromUserAgent, releaseFromManifest } from "#/lib/platform";
import { parseReleaseManifest } from "#/lib/release-manifest";

const sha = "a".repeat(64);
const artifact = (platform: string, kind: string, name: string) => ({
  platform,
  kind,
  url: `https://downloads.convt.app/0.1.0/${name}`,
  size: 1234,
  sha256: sha,
});
const manifest = (ready: boolean) => ({
  schema_version: 1,
  sequence: 2,
  issued_at: 1790000000,
  expires_at: 1797776000,
  distribution_ready: ready,
  purchase_url: "https://convt.app/pricing",
  builds: [
    {
      version: "0.1.0",
      build_date: "2026-10-07",
      artifacts: [
        artifact("macos-arm64", "dmg", "convt-0.1.0-macos-arm64.dmg"),
        artifact("linux-x86_64", "deb", "convt_0.1.0_amd64.deb"),
        artifact("windows-x86_64", "zip", "convt-0.1.0-windows-x86_64.zip"),
      ],
      source: artifact("source", "tar.gz", "convt-0.1.0-source.tar.gz"),
    },
  ],
});

describe("release manifest", () => {
  test("links nothing until distribution_ready", () => {
    const release = releaseFromManifest(parseReleaseManifest(manifest(false)));
    expect(release.slots.every((s) => s.artifact === null)).toBe(true);
    expect(release.source).toBeNull();
    expect(release.version).toBe("0.1.0");
  });

  test("fills published slots and keeps the rest pending", () => {
    const release = releaseFromManifest(parseReleaseManifest(manifest(true)));
    const kinds = (os: string) => release.slots.filter((s) => s.os === os).map((s) => s.kind);
    expect(kinds("linux")).toEqual(["AppImage", "deb", "rpm", "tar.gz"]);
    expect(kinds("windows")).toEqual(["msi", "zip"]);
    expect(release.slots.find((s) => s.kind === "dmg")?.artifact?.size).toBe(1234);
    expect(release.slots.find((s) => s.kind === "rpm")?.artifact).toBeNull();
    expect(release.source?.url).toEndWith("convt-0.1.0-source.tar.gz");
  });

  test("no manifest means every download is pending", () => {
    const release = releaseFromManifest(null);
    expect(release.slots.length).toBe(6);
    expect(release.slots.some((s) => s.artifact)).toBe(false);
  });

  test("rejects bad checksums and http URLs", () => {
    const bad = manifest(true);
    bad.builds[0].artifacts[0].sha256 = "ABC";
    expect(() => parseReleaseManifest(bad)).toThrow("sha256");
    const http = manifest(true);
    http.builds[0].source.url = "http://example.com/x.tar.gz";
    expect(() => parseReleaseManifest(http)).toThrow("https");
  });
});

test("OS detection", () => {
  expect(osFromUserAgent("Mozilla/5.0 (Macintosh; Intel Mac OS X 14_5)")).toBe("macos");
  expect(osFromUserAgent("Mozilla/5.0 (Windows NT 10.0; Win64; x64)")).toBe("windows");
  expect(osFromUserAgent("Mozilla/5.0 (X11; Linux x86_64)")).toBe("linux");
  expect(osFromUserAgent("Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X)")).toBeNull();
  expect(osFromUserAgent("Mozilla/5.0 (Linux; Android 15; Pixel 9)")).toBeNull();
});

test("download CTA labels the detected OS and falls back when unknown", () => {
  expect(downloadCtaLabel("macos")).toBe("Download for macOS");
  expect(downloadCtaLabel("windows")).toBe("Download for Windows");
  expect(downloadCtaLabel("linux")).toBe("Download for Linux");
  expect(downloadCtaLabel(null)).toBe("Download");
});

test("markdown reader", () => {
  const blocks = parseMarkdown(
    "# Changelog\n\nIntro\n\n## 0.1.0\n\n### Added\n\n- One `x`\n  continued\n- Two\n",
  );
  expect(blocks).toEqual([
    { type: "heading", level: 1, text: "Changelog" },
    { type: "paragraph", text: "Intro" },
    { type: "heading", level: 2, text: "0.1.0" },
    { type: "heading", level: 3, text: "Added" },
    { type: "list", items: ["One `x` continued", "Two"] },
  ]);
});
