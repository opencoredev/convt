import { expect, test } from "bun:test";
import { renderToStaticMarkup } from "react-dom/server";

import { CommandBlock, homebrewCommands } from "../../src/components/app/command-block";
import {
  Checksums,
  DownloadButton,
  DownloadStage,
  Homebrew,
  Platforms,
  primaryMeta,
} from "../../src/components/site/download";
import { releaseFromManifest } from "../../src/lib/platform";
import type { ReleaseManifest } from "../../src/lib/release-manifest";

const base = "https://github.com/opencoredev/convt/releases/download/v0.3.0";
const artifact = (platform: string, kind: string, name: string, size: number) => ({
  platform,
  kind,
  url: `${base}/${name}`,
  size,
  sha256: "a".repeat(64),
});
const manifest = {
  schema_version: 1,
  sequence: 3,
  issued_at: 0,
  expires_at: 0,
  distribution_ready: true,
  purchase_url: "https://convt.app/pricing",
  builds: [
    {
      version: "0.3.0",
      build_date: "2026-10-08",
      artifacts: [
        artifact("macos-arm64", "dmg", "convt-macos-arm64.dmg", 50_016_928),
        artifact("windows-x86_64", "msi", "convt-0.3.0-windows-x86_64.msi", 84_156_416),
        artifact("linux-x86_64", "AppImage", "convt-linux-x86_64.AppImage", 58_530_296),
        artifact("linux-x86_64", "deb", "convt_0.3.0-1_amd64.deb", 33_208_072),
      ],
      source: artifact("source", "tar.gz", "convt-0.3.0-source.tar.gz", 9_000_000),
    },
  ],
} as ReleaseManifest;
const release = releaseFromManifest(manifest);
function slot(os: "macos" | "windows") {
  const found = release.slots.find((s) => s.os === os);
  if (!found) throw new Error(`no ${os} slot`);
  return found;
}
const empty = releaseFromManifest(null);

test("the primary download names the system, never the file", () => {
  const html = renderToStaticMarkup(<DownloadStage os="linux" release={release} />);
  expect(html).toContain("Download for Linux");
  expect(html).not.toContain(">Download convt-linux");
  expect(html).toContain(`href="${base}/convt-linux-x86_64.AppImage"`);
  expect(html).toContain("AppImage · x86_64 · v0.3.0 · 58.5 MB");
  // The other Linux packages are links under it; the unpublished ones aren't.
  expect(html).toContain(">.deb</a>");
  expect(html).not.toContain(">.rpm</a>");
});

test("the meta line reads like Apple silicon · v0.3.0 · 50.0 MB", () => {
  expect(primaryMeta(slot("macos"), release.version)).toBe("Apple silicon · v0.3.0 · 50.0 MB");
  expect(primaryMeta(slot("windows"), release.version)).toBe("64-bit · v0.3.0 · 84.2 MB");
});

test("Homebrew has a plain heading and two one-line commands", () => {
  const html = renderToStaticMarkup(<Homebrew />);
  expect(html).toContain("Install with Homebrew");
  expect(html).toContain("whitespace-pre");
  for (const command of homebrewCommands) expect(html).toContain(command);
  const block = renderToStaticMarkup(<CommandBlock label="Homebrew" commands={homebrewCommands} />);
  expect(block.match(/class="block/g)?.length).toBe(2);
  expect(block).toContain('aria-label="Copy Homebrew commands"');
  // The stage itself stays one button and its meta line.
  expect(renderToStaticMarkup(<DownloadStage os="macos" release={release} />)).not.toContain(
    "brew",
  );
});

test("no eyebrows or monospace in the stage and platform list", () => {
  for (const html of [
    renderToStaticMarkup(<DownloadStage os="macos" release={release} />),
    renderToStaticMarkup(<Platforms os="linux" release={release} />),
  ]) {
    expect(html).not.toContain("font-mono");
    expect(html).not.toContain("uppercase");
  }
});

test("an unpublished build says Shipping today and links nothing", () => {
  const html = renderToStaticMarkup(<DownloadStage os="macos" release={empty} />);
  expect(html).toContain("Shipping today");
  expect(html).not.toContain("download=");
  expect(renderToStaticMarkup(<DownloadButton artifact={null} label="x" />)).toContain(
    "Shipping today",
  );
  expect(renderToStaticMarkup(<Checksums release={empty} />)).toBe("");
});

test("a phone or unknown system gets every platform, not a guess", () => {
  const html = renderToStaticMarkup(<DownloadStage os={null} release={release} />);
  expect(html).toContain("See all platforms");
  expect(html).not.toContain("Download for");
});

test("the platform list marks this computer and says Linux's architecture once", () => {
  const html = renderToStaticMarkup(<Platforms os="linux" release={release} />);
  expect(html).toContain("This computer");
  expect(html.match(/>x86_64</g)?.length).toBe(1);
  expect(html).not.toContain("· x86_64");
  expect(html).toContain('aria-label="Download Debian package (.deb) for Linux"');
});

test("checksums are folded away with the source for the build", () => {
  const html = renderToStaticMarkup(<Checksums release={release} />);
  expect(html).toContain("<details");
  expect(html).not.toContain("<details open");
  expect(html).toContain("a".repeat(64));
  expect(html).toContain(`href="${base}/convt-0.3.0-source.tar.gz"`);
});
