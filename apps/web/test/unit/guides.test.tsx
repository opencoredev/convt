import { expect, test } from "bun:test";
import { existsSync } from "node:fs";
import { renderToStaticMarkup } from "react-dom/server";

import { guideDownloadUrl, guideHead, guidePath, guides } from "#/lib/guides";
import { Route as BatchHeicToJpgWindows } from "../../src/routes/_site/guides/batch-heic-to-jpg-windows";
import { Route as HeicToJpgMac } from "../../src/routes/_site/guides/heic-to-jpg-mac";

test("every listed guide has a route file", () => {
  for (const guide of guides) {
    expect(
      existsSync(new URL(`../../src/routes/_site/guides/${guide.slug}.tsx`, import.meta.url)),
    ).toBe(true);
  }
});

test("a guide's head is an article with its canonical URL and Article JSON-LD", () => {
  const head = guideHead("heic-to-jpg-mac");
  expect(head.links).toContainEqual({
    rel: "canonical",
    href: "https://convt.app/guides/heic-to-jpg-mac",
  });
  expect(head.meta).toContainEqual({ property: "og:type", content: "article" });
  const data: unknown = JSON.parse(head.scripts[0].children);
  expect(data).toMatchObject({
    "@graph": [
      { "@type": "TechArticle", headline: "How to convert HEIC to JPG on a Mac" },
      { "@type": "BreadcrumbList" },
      { "@type": "Organization" },
    ],
  });
});

test("the HEIC guide's download link carries the guide campaign", () => {
  expect(guidePath("heic-to-jpg-mac")).toBe("/guides/heic-to-jpg-mac");
  expect(guideDownloadUrl("heic-to-jpg-mac")).toBe(
    "/download?utm_source=convt_guides&utm_medium=organic&utm_campaign=heic-to-jpg-mac",
  );
  const Page = HeicToJpgMac.options.component;
  if (!Page) throw new Error("guide route has no component");
  const html = renderToStaticMarkup(<Page />);
  const downloads = [...html.matchAll(/href="(\/download[^"]*)"/g)].map((m) => m[1]);
  expect(downloads).toEqual([
    "/download?utm_source=convt_guides&amp;utm_medium=organic&amp;utm_campaign=heic-to-jpg-mac",
  ]);
});

test("the Windows HEIC guide tags its download links and prints the tested command", () => {
  const head = guideHead("batch-heic-to-jpg-windows");
  expect(head.links).toContainEqual({
    rel: "canonical",
    href: "https://convt.app/guides/batch-heic-to-jpg-windows",
  });
  const Page = BatchHeicToJpgWindows.options.component;
  if (!Page) throw new Error("guide route has no component");
  const html = renderToStaticMarkup(<Page />);
  const downloads = new Set([...html.matchAll(/href="(\/download[^"]*)"/g)].map((m) => m[1]));
  expect([...downloads]).toEqual([
    "/download?utm_source=convt_guides&amp;utm_medium=organic&amp;utm_campaign=batch-heic-to-jpg-windows",
  ]);
  expect(html).toContain(
    "&amp; &quot;$env:LOCALAPPDATA\\Programs\\convt\\convt.exe&quot; iPhone --to jpg -r -o jpg",
  );
  expect(html).toContain("cd $HOME\\Pictures");
  expect(html).toContain('href="/guides/heic-to-jpg-mac"');
});
