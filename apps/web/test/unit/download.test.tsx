import { expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { renderToStaticMarkup } from "react-dom/server";

import { DownloadButton } from "../../src/components/site/download";

const page = readFileSync(new URL("../../src/routes/_site/download.tsx", import.meta.url), "utf8");

test("unpublished primary download is Coming soon, not a fake URL", () => {
  const html = renderToStaticMarkup(<DownloadButton artifact={null} large />);
  expect(html).toContain("Coming soon");
  expect(html).not.toContain("href=");
});

test("/download has no checksum or source Coming soon sections", () => {
  expect(page).not.toContain("Check your download");
  expect(page).not.toContain("Source code");
  expect(page).not.toContain("SourceSection");
  expect(page).not.toContain("verify-title");
  expect(page).toContain("DownloadButton");
  expect(page).toContain("For your computer");
});
