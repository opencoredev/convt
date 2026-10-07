import { expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { renderToStaticMarkup } from "react-dom/server";

import { DownloadButton } from "../../src/components/site/download";
import { InstallGuide } from "../../src/components/site/install-guide";
import { installSteps, primarySlot } from "../../src/lib/install-guide";
import { releaseFromManifest } from "../../src/lib/platform";
import { parseReleaseManifest } from "../../src/lib/release-manifest";

const page = readFileSync(new URL("../../src/routes/_site/download.tsx", import.meta.url), "utf8");

test("install steps cover each OS without checksum or source copy", () => {
  const mac = installSteps("macos", "dmg");
  expect(mac).toHaveLength(3);
  expect(mac[0].title).toContain("Downloads");
  expect(mac[1].title).toContain("Applications");
  expect(installSteps("linux", "AppImage")[1].title.toLowerCase()).toContain("run");
  expect(installSteps("windows", "msi")[2].title).toContain("Start");
  const html = renderToStaticMarkup(<InstallGuide os="macos" kind="dmg" />);
  expect(html).toContain("almost there");
  expect(html).toContain("Open convt.dmg from your Downloads folder");
  expect(html).not.toContain("SHA-256");
  expect(html).not.toContain("AGPL");
  expect(html).not.toContain("Check your download");
});

test("unpublished primary download is Shipping today, not a fake URL", () => {
  const html = renderToStaticMarkup(
    <DownloadButton artifact={null} large label="Download for Linux" />,
  );
  expect(html).toContain("Shipping today");
  expect(html).not.toContain("Coming soon");
  expect(html).not.toContain("href=");
});

test("published Mac artifact is a real download button", () => {
  const html = renderToStaticMarkup(
    <DownloadButton
      artifact={{
        platform: "macos-arm64",
        kind: "dmg",
        url: "https://downloads.convt.app/0.1.0/convt-0.1.0-macos-arm64.dmg",
        size: 1234,
        sha256: "a".repeat(64),
      }}
      large
      label="Download for macOS"
    />,
  );
  expect(html).toContain('href="https://downloads.convt.app/0.1.0/convt-0.1.0-macos-arm64.dmg"');
  expect(html).toContain("Download for macOS");
  expect(html).not.toContain("Shipping today");
  expect(html).not.toContain("Coming soon");
});

test("/download has no checksum or source Coming soon sections", () => {
  expect(page).not.toContain("Check your download");
  expect(page).not.toContain("Source code");
  expect(page).not.toContain("SourceSection");
  expect(page).not.toContain("verify-title");
  expect(page).not.toContain("Sha");
  expect(page).toContain("DownloadButton");
  expect(page).toContain("InstallGuide");
});

test("a published Linux AppImage is the primary download even if listed after empties", () => {
  const sha = "a".repeat(64);
  const release = releaseFromManifest(
    parseReleaseManifest({
      schema_version: 1,
      sequence: 1,
      issued_at: 1_790_000_000,
      expires_at: 1_890_000_000,
      distribution_ready: true,
      purchase_url: "https://convt.app/pricing",
      builds: [
        {
          version: "0.1.0",
          build_date: "2026-10-07",
          artifacts: [
            {
              platform: "linux-x86_64",
              kind: "AppImage",
              url: "https://github.com/opencoredev/convt/releases/download/v0.1.0/convt.AppImage",
              size: 42,
              sha256: sha,
            },
          ],
          source: {
            platform: "source",
            kind: "tar.gz",
            url: "https://github.com/opencoredev/convt/releases/download/v0.1.0/src.tar.gz",
            size: 1,
            sha256: sha,
          },
        },
      ],
    }),
  );
  const linux = primarySlot(release.slots, "linux");
  expect(linux?.kind).toBe("AppImage");
  expect(linux?.artifact?.url).toContain("convt.AppImage");
  expect(primarySlot(release.slots, "macos")?.artifact).toBeNull();
});
