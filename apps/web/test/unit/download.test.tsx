import { expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { renderToStaticMarkup } from "react-dom/server";

import { DownloadButton, HomebrewInstall } from "../../src/components/site/download";

const page = readFileSync(new URL("../../src/routes/_site/download.tsx", import.meta.url), "utf8");

test("unpublished primary download is Shipping today, not a fake URL", () => {
  const html = renderToStaticMarkup(<DownloadButton artifact={null} large />);
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
    />,
  );
  expect(html).toContain('href="https://downloads.convt.app/0.1.0/convt-0.1.0-macos-arm64.dmg"');
  expect(html).toContain("Download convt-0.1.0-macos-arm64.dmg");
  expect(html).not.toContain("Shipping today");
  expect(html).not.toContain("Coming soon");
});

test("/download has no checksum or source Coming soon sections", () => {
  expect(page).not.toContain("Check your download");
  expect(page).not.toContain("Source code");
  expect(page).not.toContain("SourceSection");
  expect(page).not.toContain("verify-title");
  expect(page).toContain("DownloadButton");
  expect(page).toContain("For your computer");
  expect(page).toContain("HomebrewInstall");
});

test("Homebrew install block renders the tap commands", () => {
  const html = renderToStaticMarkup(<HomebrewInstall />);
  expect(html).toContain("brew tap opencoredev/convt");
  expect(html).toContain("brew install --cask convt");
});
