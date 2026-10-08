// Renders the Chrome Web Store images into store/ from the real extension:
// four 1280x800 screenshots and the 440x280 promo tile.
//
//   bun run --cwd apps/extension store-shots
//
// Needs agent-browser's Chrome for Testing (or CHROME_BIN). Builds the test build,
// because only it can start a conversion without a native right-click.

import { $ } from "bun";
import { createHash } from "node:crypto";
import { mkdir, mkdtemp, readdir, readFile, rm, writeFile } from "node:fs/promises";
import { homedir, tmpdir } from "node:os";
import { extname, join } from "node:path";

const root = join(import.meta.dir, "..");
const dist = join(root, "dist-e2e");
const src = join(root, "store/src");
const out = join(root, "store");
const captures = join(src, "captures");
const work = await mkdtemp(join(tmpdir(), "convt-store-"));
await rm(captures, { recursive: true, force: true });
await mkdir(captures, { recursive: true });

await $`bun ${join(root, "scripts/build.ts")} --e2e`.quiet();

const types: Record<string, string> = {
  ".html": "text/html",
  ".css": "text/css",
  ".png": "image/png",
  ".webp": "image/webp",
  ".woff2": "font/woff2",
  ".js": "text/javascript",
};
const server = Bun.serve({
  hostname: "127.0.0.1",
  port: 0,
  async fetch(request) {
    const path = decodeURIComponent(new URL(request.url).pathname);
    const file = path.startsWith("/ext/")
      ? join(dist, path.slice(5))
      : path === "/miso.webp"
        ? join(root, "public/images/miso.webp")
        : join(src, path.slice(1));
    if (!file.startsWith(root)) return new Response("no", { status: 404 });
    try {
      return new Response(await readFile(file), {
        headers: { "content-type": types[extname(file)] ?? "application/octet-stream" },
      });
    } catch {
      return new Response("no", { status: 404 });
    }
  },
});
const site = `http://localhost:${server.port}`;

async function chromeBinary(): Promise<string> {
  if (process.env.CHROME_BIN) return process.env.CHROME_BIN;
  const dir = join(homedir(), ".agent-browser/browsers");
  const newest = (await readdir(dir))
    .filter((d) => d.startsWith("chrome-"))
    .sort()
    .at(-1);
  if (!newest) throw new Error("No Chrome for Testing; set CHROME_BIN");
  return join(dir, newest, "chrome");
}

const profile = join(work, "profile");
await mkdir(join(profile, "Default"), { recursive: true });
await writeFile(
  join(profile, "Default/Preferences"),
  JSON.stringify({ download: { default_directory: work, prompt_for_download: false } }),
);
const port = 9400 + Math.floor(Math.random() * 500);
const chrome = Bun.spawn(
  [
    await chromeBinary(),
    "--headless=new",
    "--no-sandbox",
    "--no-first-run",
    "--hide-scrollbars",
    `--user-data-dir=${profile}`,
    `--remote-debugging-port=${port}`,
    "--disable-features=DisableLoadExtensionCommandLineSwitch",
    `--load-extension=${dist}`,
    "about:blank",
  ],
  { stdout: "ignore", stderr: "ignore" },
);
const id = createHash("sha256")
  .update(dist)
  .digest("hex")
  .slice(0, 32)
  .replace(/./g, (c) => String.fromCharCode(97 + Number.parseInt(c, 16)));
const ext = (page: string) => `chrome-extension://${id}/${page}`;

const session = `convt-store-${process.pid}`;
const ab = (...args: string[]) =>
  $`agent-browser --session ${session} --cdp ${String(port)} ${args}`.text();
const evaluate = async <T>(body: string): Promise<T> =>
  JSON.parse(
    JSON.parse(
      (
        await ab("eval", `(async () => JSON.stringify(await (async () => { ${body} })()))()`)
      ).trim(),
    ),
  ) as T;
const tab = async (urlPart: string) => {
  const line = (await ab("tab", "list")).split("\n").find((l) => l.includes(urlPart));
  const tabId = line ? /\bt\d+\b/.exec(line)?.[0] : undefined;
  if (!tabId) throw new Error(`no tab for ${urlPart}`);
  await ab("tab", tabId);
};

try {
  for (let i = 0; i < 50; i++) {
    if (
      await fetch(`http://127.0.0.1:${port}/json/version`).then(
        (r) => r.ok,
        () => false,
      )
    )
      break;
    await Bun.sleep(200);
  }
  await ab("set", "media", "light");
  await ab("open", ext("popup.html"));

  /** Runs a conversion of `srcUrl` in the tab showing `pageUrl`, as a menu click would. */
  const convert = (tabId: number, pageUrl: string, srcUrl: string, action: string) =>
    evaluate<boolean>(`
      const id = crypto.randomUUID();
      await chrome.runtime.sendMessage({ kind: "test:run", job: { id, srcUrl: ${JSON.stringify(srcUrl)}, pageUrl: ${JSON.stringify(pageUrl)}, tabId: ${tabId}, frameId: 0, action: ${action} } });
      for (let i = 0; i < 80; i++) {
        await new Promise((r) => setTimeout(r, 100));
        const { recent } = await chrome.storage.local.get("recent");
        if (recent?.[0]?.jobId === id) return true;
      }
      return false;
    `);

  // 1. The article page: the menu drawing at the pointer and a real toast.
  const welcome = ext("welcome.html");
  const welcomeTab = await evaluate<number>(
    `return (await chrome.tabs.create({ url: ${JSON.stringify(welcome)}, active: false })).id;`,
  );
  await Bun.sleep(1200);
  // The popup's empty state draws the menu; borrow its markup.
  const menu = await evaluate<string>(`return document.querySelector(".md")?.outerHTML ?? "";`);
  // Earlier conversions, on the welcome tab, so the popup has something to list.
  await convert(welcomeTab, welcome, `${site}/miso.webp`, '{ kind: "save", target: "jpg" }');
  await convert(
    welcomeTab,
    welcome,
    `${site}/ext/icons/icon-128.png`,
    '{ kind: "save", target: "webp" }',
  );
  // A fourth, so the demo conversion isn't the third (which shows the desktop tip).
  await convert(welcomeTab, welcome, `${site}/miso.webp`, '{ kind: "save", target: "webp" }');
  const demo = `${site}/demo.html`;
  await ab("tab", "new", demo);
  await Bun.sleep(1000);
  await tab("/popup.html");
  const demoTab = await evaluate<number>(
    `return (await chrome.tabs.query({ url: "http://localhost/*" }))[0].id;`,
  );
  await convert(demoTab, demo, `${site}/miso.webp`, '{ kind: "save", target: "png" }');
  await tab("/demo.html");
  await ab("set", "viewport", "1280", "800");
  await ab("eval", `document.getElementById("menu").innerHTML = ${JSON.stringify(menu)}; 1`);
  await Bun.sleep(600);
  await ab("screenshot", join(out, "screenshot-1-right-click.png"));

  // Captures the framed screenshots use.
  await tab("/popup.html");
  await ab("set", "media", "dark");
  // Each view is captured at its own height, like the real popup sizes itself.
  const fit = async () => {
    const height = await evaluate<number>(`return document.getElementById("app").offsetHeight;`);
    await ab("set", "viewport", "360", String(height), "2");
  };
  await ab("set", "viewport", "360", "600", "2");
  await ab("open", ext("popup.html"));
  await Bun.sleep(800);
  await fit();
  await ab("screenshot", join(captures, "popup.png"));
  await ab("set", "viewport", "360", "600", "2");
  await ab("click", "button[aria-label=Settings]");
  await Bun.sleep(400);
  await fit();
  await ab("screenshot", join(captures, "settings.png"));

  await ab("set", "viewport", "820", "1100", "2");
  await ab("open", ext("gallery.html"));
  await Bun.sleep(800);
  await ab("screenshot", "#grid > .toast:nth-child(2)", join(captures, "toast-saved.png"));
  await ab("screenshot", "#grid > .toast:nth-child(3)", join(captures, "toast-gif.png"));
  await ab("screenshot", "#grid > .toast:nth-child(4)", join(captures, "toast-copied.png"));

  // 2 and 3. Framed screenshots.
  await ab("set", "media", "light");
  await ab("set", "viewport", "1280", "800");
  await ab("open", `${site}/popup.html`);
  await Bun.sleep(800);
  await ab("screenshot", join(out, "screenshot-2-recent-files.png"));
  await ab("open", `${site}/private.html`);
  await Bun.sleep(800);
  await ab("screenshot", join(out, "screenshot-3-private.png"));

  // 4. The welcome page after its first conversion.
  await ab("open", ext("welcome.html"));
  await Bun.sleep(1000);
  await evaluate(`
    const t = await chrome.tabs.getCurrent();
    await chrome.runtime.sendMessage({ kind: "test:run", job: { id: crypto.randomUUID(), srcUrl: chrome.runtime.getURL("images/miso.webp"), pageUrl: location.href, tabId: t.id, frameId: 0, action: { kind: "save", target: "png" } } });
    await new Promise((r) => setTimeout(r, 2500));
    return true;
  `);
  await Bun.sleep(500);
  await ab("screenshot", join(out, "screenshot-4-welcome.png"));

  // Promo tile.
  await ab("set", "viewport", "440", "280");
  await ab("open", `${site}/promo.html`);
  await Bun.sleep(800);
  await ab("screenshot", join(out, "promo-small-440x280.png"));
  console.log(`wrote store images to ${out}`);
} finally {
  await ab("close").catch(() => {});
  chrome.kill();
  server.stop(true);
}
