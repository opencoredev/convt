import { expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { renderToStaticMarkup } from "react-dom/server";

import { DownloadButton } from "../../src/components/site/download";
import { InstallGuide } from "../../src/components/site/install-guide";
import {
  installSteps,
  primarySlot,
  processorLabel,
  selectedSlot,
  slotCaption,
} from "../../src/lib/install-guide";
import { releaseFromManifest } from "../../src/lib/platform";
import { parseReleaseManifest } from "../../src/lib/release-manifest";

const page = readFileSync(new URL("../../src/routes/_site/download.tsx", import.meta.url), "utf8");
const sha = "a".repeat(64);

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
  expect(html).toContain(".dmg");
  expect(html).not.toContain("SHA-256");
  expect(html).not.toContain("AGPL");
  expect(html).not.toContain("Check your download");
});

test("zip downloads get archive steps and pictures, not installer steps", () => {
  const macZip = installSteps("macos", "zip");
  expect(macZip[0].title.toLowerCase()).toContain("zip");
  expect(macZip.join(" ")).not.toContain("convt.dmg");
  const winZip = installSteps("windows", "zip");
  expect(winZip[2].title).toContain("convt-app.exe");
  expect(winZip.map((s) => s.title).join(" ")).not.toMatch(/installer/i);
  const macHtml = renderToStaticMarkup(<InstallGuide os="macos" kind="zip" />);
  expect(macHtml).toContain(".zip");
  expect(macHtml).not.toContain(".dmg");
  const winHtml = renderToStaticMarkup(<InstallGuide os="windows" kind="zip" />);
  expect(winHtml).toContain(".zip");
  expect(winHtml).toContain(".exe");
  expect(winHtml).not.toContain(".msi");
});

test("Windows installer copy promises a Start menu shortcut, not Explorer", () => {
  const body = installSteps("windows", "msi")[1].body;
  expect(body).toContain("Start menu shortcut");
  expect(body.toLowerCase()).not.toContain("right-click");
});

test("archive guide launches convt-app and does not invent a menu helper", () => {
  const tar = installSteps("linux", "tar.gz");
  expect(tar[1].title).toContain("./convt-app");
  expect(tar[1].body).toContain("./convt-app");
  expect(tar.map((s) => s.body).join(" ")).not.toMatch(/menu helper/i);
  const html = renderToStaticMarkup(<InstallGuide os="linux" kind="tar.gz" />);
  expect(html).toContain("./convt-app");
  expect(html).toContain(".tar");
  expect(html).not.toContain("convt.AppImage");
});

test("Linux package kinds update steps and pictures", () => {
  expect(installSteps("linux", "deb")[0].title).toContain(".deb");
  expect(installSteps("linux", "rpm")[0].title).toContain(".rpm");
  const deb = renderToStaticMarkup(<InstallGuide os="linux" kind="deb" />);
  expect(deb).toContain(".deb");
  expect(deb).toContain("Open the .deb from your Downloads folder");
  expect(deb).not.toContain("convt.AppImage");
  const rpm = renderToStaticMarkup(<InstallGuide os="linux" kind="rpm" />);
  expect(rpm).toContain(".rpm");
  expect(rpm).not.toContain("convt.AppImage");
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
        sha256: sha,
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

test("/download tracks the selected format and has no checksum sections", () => {
  expect(page).not.toContain("Check your download");
  expect(page).not.toContain("Source code");
  expect(page).not.toContain("SourceSection");
  expect(page).not.toContain("verify-title");
  expect(page).not.toContain("Sha");
  expect(page).toContain("DownloadButton");
  expect(page).toContain("InstallGuide");
  expect(page).toContain("kind");
  expect(page).toContain("selectedSlot");
  expect(page).toContain("slotCaption");
});

test("a published Linux AppImage is the primary download even if listed after empties", () => {
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
            {
              platform: "linux-x86_64",
              kind: "deb",
              url: "https://github.com/opencoredev/convt/releases/download/v0.1.0/convt.deb",
              size: 40,
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
  expect(selectedSlot(release.slots, "linux", "deb")?.kind).toBe("deb");
  expect(selectedSlot(release.slots, "linux", "deb")?.artifact?.url).toContain("convt.deb");
  expect(selectedSlot(release.slots, "linux", "dmg")?.kind).toBe("AppImage");
  expect(primarySlot(release.slots, "macos")?.artifact).toBeNull();
});

test("published downloads show the processor next to the file kind", () => {
  expect(processorLabel({ os: "macos", arch: "arm64" })).toBe("Apple silicon");
  expect(processorLabel({ os: "windows", arch: "x86_64" })).toBe("64-bit");
  expect(
    slotCaption({
      os: "macos",
      kind: "dmg",
      arch: "arm64",
      artifact: {
        platform: "macos-arm64",
        kind: "dmg",
        url: "https://downloads.convt.app/0.1.0/convt.dmg",
        size: 12_582_912,
        sha256: sha,
      },
    }),
  ).toContain("Apple silicon");
});
