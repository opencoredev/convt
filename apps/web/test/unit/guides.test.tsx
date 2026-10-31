import { expect, test } from "bun:test";
import { existsSync } from "node:fs";
import { renderToStaticMarkup } from "react-dom/server";

import { guideDownloadUrl, guideHead, guidePath, guides, relatedGuides } from "#/lib/guides";
import { Route as BatchHeicToJpgWindows } from "../../src/routes/_site/guides/batch-heic-to-jpg-windows";
import { Route as HeicToJpgMac } from "../../src/routes/_site/guides/heic-to-jpg-mac";
import { Route as MovToMp4Mac } from "../../src/routes/_site/guides/mov-to-mp4-mac";
import { Route as PdfToPngMacWindows } from "../../src/routes/_site/guides/pdf-to-png-mac-windows";
import { Route as WebpToPngJpg } from "../../src/routes/_site/guides/webp-to-png-jpg";

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

function renderGuide(
  route:
    | typeof HeicToJpgMac
    | typeof BatchHeicToJpgWindows
    | typeof MovToMp4Mac
    | typeof WebpToPngJpg
    | typeof PdfToPngMacWindows,
) {
  const Page = route.options.component;
  if (!Page) throw new Error("guide route has no component");
  return renderToStaticMarkup(<Page />);
}

test("the MOV to MP4 guide prints the tested remux and folder commands", () => {
  expect(guideHead("mov-to-mp4-mac").links).toContainEqual({
    rel: "canonical",
    href: "https://convt.app/guides/mov-to-mp4-mac",
  });
  const html = renderGuide(MovToMp4Mac);
  const downloads = new Set([...html.matchAll(/href="(\/download[^"]*)"/g)].map((m) => m[1]));
  expect([...downloads]).toEqual([
    "/download?utm_source=convt_guides&amp;utm_medium=organic&amp;utm_campaign=mov-to-mp4-mac",
  ]);
  expect(html).toContain("ffmpeg -i input.mov -c copy -movflags +faststart output.mp4");
  expect(html).toContain("convt videos --to mp4 -r -o mp4");
  expect(html).toContain("convt clip.mov --to mp4 --video-height 720 -q 60");
  expect(html).toContain("-preset medium -pix_fmt yuv420p -c:a aac");
  expect(html).toContain(
    `find . -maxdepth 1 -iname &quot;*.mov&quot; -exec sh -c &#x27;ffmpeg -i &quot;$1&quot; -c copy &quot;\${1%.*}.mp4&quot;&#x27; _ {} \\;`,
  );
});

test("the WebP guide tags its download link and prints the tested folder commands", () => {
  expect(guideHead("webp-to-png-jpg").links).toContainEqual({
    rel: "canonical",
    href: "https://convt.app/guides/webp-to-png-jpg",
  });
  const html = renderGuide(WebpToPngJpg);
  const downloads = new Set([...html.matchAll(/href="(\/download[^"]*)"/g)].map((m) => m[1]));
  expect([...downloads]).toEqual([
    "/download?utm_source=convt_guides&amp;utm_medium=organic&amp;utm_campaign=webp-to-png-jpg",
  ]);
  expect(html).toContain("convt images --to png -r -o png");
  expect(html).toContain("convt images --to jpg -r -o jpg");
  expect(html).toContain('href="/guides/heic-to-jpg-mac"');
});

test("every guide links to every other guide, same kind of file first", () => {
  const routes = {
    "heic-to-jpg-mac": HeicToJpgMac,
    "batch-heic-to-jpg-windows": BatchHeicToJpgWindows,
    "mov-to-mp4-mac": MovToMp4Mac,
    "webp-to-png-jpg": WebpToPngJpg,
    "pdf-to-png-mac-windows": PdfToPngMacWindows,
  } satisfies Record<(typeof guides)[number]["slug"], unknown>;
  for (const guide of guides) {
    const html = renderGuide(routes[guide.slug]);
    expect(html).toContain("Related guides");
    for (const other of guides) {
      if (other.slug === guide.slug) continue;
      expect(html).toContain(`href="/guides/${other.slug}"`);
    }
  }
  // The Mac HEIC guide lists the other image guides before the video one, unlike catalog order.
  expect(relatedGuides(guides[0]).map((g) => g.slug)).toEqual([
    "batch-heic-to-jpg-windows",
    "webp-to-png-jpg",
    "mov-to-mp4-mac",
    "pdf-to-png-mac-windows",
  ]);
  const related = renderGuide(HeicToJpgMac).split("Related guides")[1] ?? "";
  expect([...related.matchAll(/href="\/guides\/([^"]+)"/g)].map((m) => m[1])).toEqual([
    "batch-heic-to-jpg-windows",
    "webp-to-png-jpg",
    "mov-to-mp4-mac",
    "pdf-to-png-mac-windows",
  ]);
  expect(relatedGuides(guides[2]).map((g) => g.slug)).toEqual([
    "heic-to-jpg-mac",
    "batch-heic-to-jpg-windows",
    "webp-to-png-jpg",
    "pdf-to-png-mac-windows",
  ]);
  expect(relatedGuides(guides[3]).map((g) => g.slug)).toEqual([
    "heic-to-jpg-mac",
    "batch-heic-to-jpg-windows",
    "mov-to-mp4-mac",
    "pdf-to-png-mac-windows",
  ]);
});

test("the PDF guide tags its download link and prints the tested page and folder commands", () => {
  const head = guideHead("pdf-to-png-mac-windows");
  expect(head.links).toContainEqual({
    rel: "canonical",
    href: "https://convt.app/guides/pdf-to-png-mac-windows",
  });
  const html = renderGuide(PdfToPngMacWindows);
  const downloads = new Set([...html.matchAll(/href="(\/download[^"]*)"/g)].map((m) => m[1]));
  expect([...downloads]).toEqual([
    "/download?utm_source=convt_guides&amp;utm_medium=organic&amp;utm_campaign=pdf-to-png-mac-windows",
  ]);
  expect(html).toContain("convt report.pdf --to png --pages 2 --dpi 300");
  expect(html).toContain("convt docs --to png -r -o png");
  expect(html).toContain(
    "&amp; &quot;$env:LOCALAPPDATA\\Programs\\convt\\convt.exe&quot; docs --to png -r -o png",
  );
  expect(html).toContain(
    "&amp; &quot;$env:LOCALAPPDATA\\Programs\\convt\\convt.exe&quot; report.pdf --to png",
  );
  // A documents guide lists the other guides after it, in catalog order.
  expect(relatedGuides(guides[4]).map((g) => g.slug)).toEqual([
    "heic-to-jpg-mac",
    "batch-heic-to-jpg-windows",
    "mov-to-mp4-mac",
    "webp-to-png-jpg",
  ]);
});
