// Builds the extension into dist/ and zips it for the Chrome Web Store and Edge Add-ons.
//
//   bun scripts/build.ts            release build: dist/ and convt-extension-<version>.zip
//   bun scripts/build.ts --e2e      test build in dist-e2e/ that accepts `test:run` messages
//   bun scripts/build.ts --watch    rebuild dist/ on every change

import { $ } from "bun";
import { cp, mkdir, rm, watch } from "node:fs/promises";
import { join } from "node:path";

import pkg from "../package.json" with { type: "json" };

const root = join(import.meta.dir, "..");
const e2e = process.argv.includes("--e2e");
const watching = process.argv.includes("--watch");
const out = join(root, e2e ? "dist-e2e" : "dist");
const fonts = join(root, "node_modules/@fontsource-variable");

/** The Chrome Web Store shows `name` (45 chars max) and `description` (132 max). */
const manifest = {
  manifest_version: 3,
  name: "convt: Save Image as PNG, JPG or WebP",
  short_name: "convt",
  description:
    "Right-click any image to save it as PNG, JPG or WebP, or copy it as PNG. Converts in your browser; nothing is uploaded.",
  version: pkg.version,
  homepage_url: "https://convt.app",
  minimum_chrome_version: "120",
  icons: {
    "16": "icons/icon-16.png",
    "32": "icons/icon-32.png",
    "48": "icons/icon-48.png",
    "128": "icons/icon-128.png",
  },
  action: {
    default_title: "convt",
    default_popup: "popup.html",
    default_icon: { "16": "icons/icon-16.png", "32": "icons/icon-32.png" },
  },
  background: { service_worker: "background.js", type: "module" },
  // Each one's warning, if any, is in README.md.
  permissions: [
    "contextMenus",
    "downloads",
    "storage",
    "scripting",
    "activeTab",
    "offscreen",
    "clipboardWrite",
  ],
  optional_host_permissions: ["<all_urls>"],
};

async function build(): Promise<void> {
  await rm(out, { recursive: true, force: true });
  await mkdir(join(out, "fonts"), { recursive: true });

  const define = { __E2E__: JSON.stringify(e2e) };
  const modules = await Bun.build({
    entrypoints: [
      join(root, "src/background/index.ts"),
      join(root, "src/offscreen/index.ts"),
      join(root, "src/popup/index.ts"),
      join(root, "src/welcome/index.ts"),
    ],
    naming: "[dir].[ext]",
    root: join(root, "src"),
    outdir: out,
    target: "browser",
    format: "esm",
    minify: !e2e,
    define,
  });
  // Injected with chrome.scripting, which runs classic scripts only.
  const content = await Bun.build({
    entrypoints: [join(root, "src/content/toast.ts")],
    naming: "toast.js",
    outdir: out,
    target: "browser",
    format: "iife",
    minify: !e2e,
    define,
  });
  const gallery = e2e
    ? await Bun.build({
        entrypoints: [join(root, "test/gallery/gallery.ts")],
        naming: "gallery.js",
        outdir: out,
        target: "browser",
        format: "esm",
        define,
      })
    : null;
  for (const result of [modules, content, ...(gallery ? [gallery] : [])]) {
    if (!result.success) {
      for (const log of result.logs) console.error(log);
      throw new Error("build failed");
    }
  }

  const copies: [string, string][] = [
    ["src/popup/popup.html", "popup.html"],
    ["src/popup/popup.css", "popup.css"],
    ["src/welcome/welcome.html", "welcome.html"],
    ["src/welcome/welcome.css", "welcome.css"],
    ["src/offscreen/offscreen.html", "offscreen.html"],
    ["src/ui/tokens.css", "tokens.css"],
    ["src/ui/page.css", "page.css"],
    ["src/ui/toast.css", "toast.css"],
    ["src/ui/menu-demo.css", "menu-demo.css"],
    ["public/icons", "icons"],
    ["public/images", "images"],
  ];
  if (e2e) copies.push(["test/gallery/gallery.html", "gallery.html"]);
  for (const [from, to] of copies) await cp(join(root, from), join(out, to), { recursive: true });
  await cp(
    join(fonts, "geist/files/geist-latin-wght-normal.woff2"),
    join(out, "fonts/geist.woff2"),
  );
  await cp(
    join(fonts, "geist-mono/files/geist-mono-latin-wght-normal.woff2"),
    join(out, "fonts/geist-mono.woff2"),
  );
  // The test build can script its own fixture page (the real build gets that from the
  // right-click, via activeTab) and read the clipboard to check Copy as PNG.
  const written = e2e
    ? {
        ...manifest,
        permissions: [...manifest.permissions, "clipboardRead", "webNavigation"],
        host_permissions: ["*://localhost/*"],
      }
    : manifest;
  await Bun.write(join(out, "manifest.json"), `${JSON.stringify(written, null, 2)}\n`);
}

await build();
console.log(`built ${out}`);

if (!e2e && !watching) {
  const zip = join(root, `convt-extension-${pkg.version}.zip`);
  await rm(zip, { force: true });
  await $`zip -qrX ${zip} .`.cwd(out);
  console.log(`zipped ${zip}`);
}

if (watching) {
  for await (const _event of watch(join(root, "src"), { recursive: true })) {
    await build().catch((error: unknown) => console.error(error));
    console.log(`rebuilt ${new Date().toLocaleTimeString()}`);
  }
}
