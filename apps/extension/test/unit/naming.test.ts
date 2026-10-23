import { describe, expect, test } from "bun:test";

import { baseName, formatBytes, outputName, stemFromUrl } from "../../src/shared/naming.ts";

describe("outputName", () => {
  test("swaps the extension and drops the query", () => {
    expect(outputName("https://cdn.example.com/photos/Miso%20asleep.webp?w=800", "png")).toBe(
      "Miso asleep.png",
    );
    expect(outputName("https://example.com/a/b/cat.JPEG#x", "webp")).toBe("cat.webp");
    expect(outputName("https://example.com/logo.svg", "jpg")).toBe("logo.jpg");
  });

  test("keeps dots that aren't extensions", () => {
    expect(stemFromUrl("https://example.com/v1.2.3-release.png")).toBe("v1.2.3-release");
    expect(stemFromUrl("https://example.com/render.php")).toBe("render");
  });

  test("names extensionless and odd URLs sensibly", () => {
    expect(stemFromUrl("https://pbs.twimg.com/media/GZx81aXbQAA?format=jpg&name=large")).toBe(
      "GZx81aXbQAA",
    );
    expect(stemFromUrl("https://example.com/")).toBe("image");
    expect(stemFromUrl("data:image/png;base64,AAAA")).toBe("image");
    expect(stemFromUrl("blob:https://example.com/0b1c")).toBe("image");
    expect(stemFromUrl("not a url")).toBe("image");
    expect(stemFromUrl("file:///home/me/Pictures/scan%201.webp")).toBe("scan 1");
    expect(stemFromUrl("chrome-extension://abc/images/miso.webp")).toBe("miso");
  });

  test("uses the real file behind an image proxy", () => {
    expect(
      stemFromUrl("https://site.dev/_next/image?url=%2Fimages%2Fhero-shot.png&w=1080&q=75"),
    ).toBe("hero-shot");
    expect(stemFromUrl("https://img.example.com/resize?src=https://a.com/x/pic.webp?v=2")).toBe(
      "pic",
    );
    // A `url` parameter that doesn't name an image is ignored.
    expect(stemFromUrl("https://example.com/thumb.jpg?url=https://example.com/page")).toBe("thumb");
  });

  test("removes characters file systems reject", () => {
    expect(stemFromUrl("https://example.com/a%3Ab%22c%7Cd%3F.png")).toBe("a b c d");
    expect(stemFromUrl("https://example.com/..%2F..%2Fetc.png")).toBe("etc");
    expect(stemFromUrl("https://example.com/%E2%80%8B.png")).toBe("\u200b");
    expect(stemFromUrl("https://example.com/con.png")).toBe("con-image");
    expect(stemFromUrl("https://example.com/%2E%2E.png")).toBe("image");
  });

  test("caps long names", () => {
    const long = "x".repeat(300);
    expect(stemFromUrl(`https://example.com/${long}.png`)).toHaveLength(100);
  });

  test("survives malformed escapes", () => {
    expect(stemFromUrl("https://example.com/100%25-real%E0%A4%A.png")).toBe("100%25-real%E0%A4%A");
  });
});

describe("baseName", () => {
  test("handles both path separators", () => {
    expect(baseName("/home/me/Downloads/cat (1).png")).toBe("cat (1).png");
    expect(baseName("C:\\Users\\me\\Downloads\\cat.png")).toBe("cat.png");
    expect(baseName("cat.png")).toBe("cat.png");
  });
});

describe("formatBytes", () => {
  test("uses decimal units like the site", () => {
    expect(formatBytes(512)).toBe("512 B");
    expect(formatBytes(1500)).toBe("1.5 KB");
    expect(formatBytes(612_000)).toBe("612 KB");
    expect(formatBytes(4_800_000)).toBe("4.8 MB");
    expect(formatBytes(999_999)).toBe("1000 KB");
  });
});
