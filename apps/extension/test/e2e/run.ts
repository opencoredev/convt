// End-to-end test: the real extension in headless Chrome for Testing, converting
// images served by local servers. Native context menus can't be clicked headless, so
// each case sends the test build's `test:run` message, which runs exactly what a menu
// click runs.
//
//   bun run --cwd apps/extension test:e2e
//
// Needs agent-browser (its Chrome for Testing, or CHROME_BIN) and ffmpeg for fixtures.
// Set KEEP=1 to leave the browser running afterwards for screenshots.

import { $ } from "bun";
import { createHash } from "node:crypto";
import { mkdtemp, readdir, readFile, mkdir, writeFile, cp } from "node:fs/promises";
import { homedir, tmpdir } from "node:os";
import { join } from "node:path";

import { sniff } from "../../src/shared/sniff.ts";
import { freePort } from "../free-port.ts";

const root = join(import.meta.dir, "../..");
const dist = join(root, "dist-e2e");
const work = await mkdtemp(join(tmpdir(), "convt-ext-e2e-"));
const downloads = join(work, "downloads");
const fixtures = join(work, "fixtures");
await mkdir(downloads);
await mkdir(fixtures);

const failures: string[] = [];
function check(name: string, ok: boolean, detail = "") {
  console.log(`${ok ? "PASS" : "FAIL"}  ${name}${detail ? `  (${detail})` : ""}`);
  if (!ok) failures.push(name);
}

// --- Fixtures -------------------------------------------------------------------------

await $`bun ${join(root, "scripts/build.ts")} --e2e`.quiet();
await cp(join(root, "public/images/miso.webp"), join(fixtures, "photo.webp"));
const ff = (args: string[]) => $`ffmpeg -loglevel error -y ${args}`.quiet();
await ff([
  "-f",
  "lavfi",
  "-i",
  // An opaque red square in the middle of a transparent 64x64 canvas.
  "color=c=red:s=32x32,format=rgba,pad=64:64:16:16:color=0x00000000",
  "-frames:v",
  "1",
  join(fixtures, "transparent.png"),
]);
await ff(["-f", "lavfi", "-i", "testsrc=size=80x60:rate=5", "-t", "1", join(fixtures, "anim.gif")]);
await ff([
  "-f",
  "lavfi",
  "-i",
  "testsrc2=size=6000x4000",
  "-frames:v",
  "1",
  join(fixtures, "huge.png"),
]);
// Camera-like noise: as a PNG this is over 64 MB, past Chrome's message cap.
await ff([
  "-f",
  "lavfi",
  "-i",
  "color=c=gray:s=6000x4000,noise=alls=100:allf=t",
  "-frames:v",
  "1",
  "-q:v",
  "3",
  join(fixtures, "noise.jpg"),
]);
// A PNG signature followed by 46 MB: over the extension's 45 MB source limit.
await writeFile(
  join(fixtures, "oversized.png"),
  Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    Buffer.alloc(46_000_000),
  ]),
);
await writeFile(
  join(fixtures, "icon.svg"),
  '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><circle cx="12" cy="12" r="10" fill="#127a47"/></svg>',
);
// HEIC brand header: Chrome can't decode HEIC, which is all this needs to prove.
await writeFile(
  join(fixtures, "photo.heic"),
  Buffer.from([0, 0, 0, 24, ...Buffer.from("ftypheic"), 0, 0, 0, 0, ...Buffer.from("mif1heic")]),
);

// --- Servers ----------------------------------------------------------------------------
// localhost: the page and same-site images, under a strict CSP.
// 127.0.0.2: an image host with no CORS headers (needs site access).
// 127.0.0.3: an image host that allows cross-origin reads.

const types: Record<string, string> = {
  webp: "image/webp",
  png: "image/png",
  gif: "image/gif",
  svg: "image/svg+xml",
  heic: "image/heic",
  jpg: "image/jpeg",
  html: "text/html",
  js: "text/javascript",
};
async function file(name: string, headers: Record<string, string> = {}) {
  const ext = name.split(".").at(-1) ?? "";
  return new Response(await readFile(join(fixtures, name)), {
    headers: { "content-type": types[ext] ?? "application/octet-stream", ...headers },
  });
}

const page = Bun.serve({
  hostname: "127.0.0.1",
  port: 0,
  async fetch(request) {
    const path = new URL(request.url).pathname;
    if (path === "/") {
      return new Response(
        `<!doctype html><title>fixture</title><h1>Fixture page</h1><img src="/photo.webp" width="320"><iframe src="http://127.0.0.3:${cors.port}/frame.html"></iframe><script src="/page.js"></script>`,
        {
          headers: {
            "content-type": "text/html",
            // The private image below needs this cookie; the extension's own fetch omits it.
            "set-cookie": "session=1; Path=/; SameSite=Lax",
            // Strict, like many real sites: no inline styles or scripts.
            "content-security-policy":
              "default-src 'none'; script-src 'self'; img-src 'self' http://127.0.0.2:* http://127.0.0.3:* data: blob:; connect-src 'self' blob:; frame-src http://127.0.0.3:*",
          },
        },
      );
    }
    if (path === "/page.js") {
      return new Response(
        `addEventListener("message", (e) => { globalThis.frameBlobUrl = e.data; });
        fetch("/transparent.png").then(r => r.blob()).then(b => { globalThis.blobUrl = URL.createObjectURL(b); document.title = "ready"; });`,
        { headers: { "content-type": "text/javascript" } },
      );
    }
    if (path === "/not-image.png")
      return new Response("<html>Sign in</html>", { headers: { "content-type": "text/html" } });
    if (path === "/missing.png") return new Response("no", { status: 404 });
    if (path === "/private-big.png") {
      const signedIn = request.headers.get("cookie")?.includes("session=1") ?? false;
      return signedIn ? file("oversized.png") : new Response("sign in", { status: 403 });
    }
    if (path === "/private.webp") {
      const signedIn = request.headers.get("cookie")?.includes("session=1") ?? false;
      return signedIn ? file("photo.webp") : new Response("sign in", { status: 403 });
    }
    return file(path.slice(1)).catch(() => new Response("no", { status: 404 }));
  },
});
const noCors = Bun.serve({
  hostname: "127.0.0.2",
  port: 0,
  fetch: (request) => file(new URL(request.url).pathname.slice(1)),
});
const cors = Bun.serve({
  hostname: "127.0.0.3",
  port: 0,
  fetch(request) {
    const path = new URL(request.url).pathname;
    // An embedded frame from another site that makes its own blob: image.
    if (path === "/frame.html") {
      return new Response('<!doctype html><img id="i"><script src="/frame.js"></script>', {
        headers: { "content-type": "text/html" },
      });
    }
    if (path === "/frame.js") {
      return new Response(
        `fetch("/photo.webp").then(r => r.blob()).then(b => { const u = URL.createObjectURL(b); document.getElementById("i").src = u; parent.postMessage(u, "*"); });`,
        { headers: { "content-type": "text/javascript" } },
      );
    }
    return file(path.slice(1), { "access-control-allow-origin": "*" });
  },
});
const pageOrigin = `http://localhost:${page.port}`;

// --- Chrome -------------------------------------------------------------------------------

async function chromeBinary(): Promise<string> {
  if (process.env.CHROME_BIN) return process.env.CHROME_BIN;
  const dir = join(homedir(), ".agent-browser/browsers");
  const versions = (await readdir(dir)).filter((d) => d.startsWith("chrome-")).sort();
  const newest = versions.at(-1);
  if (!newest) throw new Error("No Chrome for Testing; set CHROME_BIN");
  return join(dir, newest, "chrome");
}

const profile = join(work, "profile");
await mkdir(join(profile, "Default"), { recursive: true });
await writeFile(
  join(profile, "Default/Preferences"),
  JSON.stringify({ download: { default_directory: downloads, prompt_for_download: false } }),
);
const port = await freePort();
const chrome = Bun.spawn(
  [
    await chromeBinary(),
    "--headless=new",
    "--no-sandbox",
    "--no-first-run",
    `--user-data-dir=${profile}`,
    `--remote-debugging-port=${port}`,
    // Chrome 137+ ignores --load-extension unless this feature is off.
    "--disable-features=DisableLoadExtensionCommandLineSwitch",
    `--load-extension=${dist}`,
    "about:blank",
  ],
  { stdout: "ignore", stderr: "ignore" },
);

const session = `convt-ext-e2e-${process.pid}`;
const ab = (...args: string[]) =>
  $`agent-browser --session ${session} --cdp ${String(port)} ${args}`.text();

async function focusExtensionTab() {
  const list = await ab("tab", "list");
  const line = list
    .split("\n")
    .find((l) => l.includes(`chrome-extension://${extensionId}/popup.html`));
  // Lines look like "  t2  title  url"; tab ids are stable labels such as t2.
  const id = line ? /\bt\d+\b/.exec(line)?.[0] : undefined;
  if (id === undefined) throw new Error(`extension tab not found:\n${list}`);
  await ab("tab", id);
}

/**
 * Makes one tab report focus, as it would after a real click. Headless Chrome has no
 * window focus, and the clipboard refuses unfocused documents. Holds a DevTools
 * session open until the returned function is called.
 */
async function emulateFocus(urlPart: string): Promise<() => void> {
  const targets = (await (await fetch(`http://127.0.0.1:${port}/json/list`)).json()) as {
    url: string;
    webSocketDebuggerUrl: string;
  }[];
  const target = targets.find((t) => t.url.includes(urlPart));
  if (!target) throw new Error(`no tab matching ${urlPart}`);
  const socket = new WebSocket(target.webSocketDebuggerUrl);
  await new Promise((resolve, reject) => {
    socket.onopen = resolve;
    socket.onerror = reject;
  });
  const done = new Promise((resolve) => (socket.onmessage = resolve));
  socket.send(
    JSON.stringify({
      id: 1,
      method: "Emulation.setFocusEmulationEnabled",
      params: { enabled: true },
    }),
  );
  await done;
  return () => socket.close();
}

/** Evaluates `body` (an async function body) in the extension tab and returns its JSON result. */
async function inExtension<T>(body: string): Promise<T> {
  const out = await ab("eval", `(async () => JSON.stringify(await (async () => { ${body} })()))()`);
  return JSON.parse(JSON.parse(out.trim())) as T;
}

const extensionId = createHash("sha256")
  .update(dist)
  .digest("hex")
  .slice(0, 32)
  .replace(/./g, (c) => String.fromCharCode(97 + Number.parseInt(c, 16)));

let exitCode = 0;
try {
  for (let i = 0; i < 50; i++) {
    const ready = await fetch(`http://127.0.0.1:${port}/json/version`)
      .then((r) => r.ok)
      .catch(() => false);
    if (ready) break;
    await Bun.sleep(200);
  }
  await ab("open", `chrome-extension://${extensionId}/popup.html`);

  const tabId = await inExtension<number>(`
    const tab = await chrome.tabs.create({ url: ${JSON.stringify(`${pageOrigin}/`)}, active: true });
    for (let i = 0; i < 100; i++) {
      const t = await chrome.tabs.get(tab.id);
      if (t.status === "complete" && t.title === "ready") return tab.id;
      await new Promise((r) => setTimeout(r, 100));
    }
    throw new Error("fixture page didn't load");
  `).catch(async () => {
    throw new Error(`fixture page didn't load: ${await ab("tab", "list")}`);
  });
  // Creating the fixture tab moves agent-browser to it; come back to the extension.
  await focusExtensionTab();

  type Logged = {
    jobId: string;
    phase: string;
    tone: string;
    title: string;
    detail: { text: string };
    note: { text: string } | null;
    action: { kind: string } | null;
  };
  /** Runs one conversion and waits for its toast to settle. */
  async function run(
    srcUrl: string,
    action: object,
  ): Promise<{ jobId: string; last: Logged | null; log: Logged[] }> {
    return inExtension(`
      const jobId = crypto.randomUUID();
      const job = { id: jobId, srcUrl: ${JSON.stringify(srcUrl)}, pageUrl: ${JSON.stringify(`${pageOrigin}/`)}, tabId: ${tabId}, frameId: 0, action: ${JSON.stringify(action)} };
      await chrome.runtime.sendMessage({ kind: "test:run", job });
      for (let i = 0; i < 300; i++) {
        const [r] = await chrome.scripting.executeScript({
          target: { tabId: ${tabId} },
          func: (id) => (globalThis.__convtLog ?? []).filter((e) => e.jobId === id),
          args: [jobId],
        });
        const log = r?.result ?? [];
        const last = log.at(-1) ?? null;
        if (last && last.phase !== "working") return { jobId, last, log };
        await new Promise((r) => setTimeout(r, 100));
      }
      return { jobId, last: null, log: [] };
    `);
  }

  async function downloaded(name: string): Promise<Uint8Array | null> {
    for (let i = 0; i < 50; i++) {
      const files = await readdir(downloads);
      if (files.includes(name)) return new Uint8Array(await readFile(join(downloads, name)));
      await Bun.sleep(100);
    }
    return null;
  }

  const pngSize = (b: Uint8Array) => {
    const v = new DataView(b.buffer, b.byteOffset);
    return { width: v.getUint32(16), height: v.getUint32(20) };
  };

  // 1. Same-site WebP to PNG: worker fetch via site access to the page's origin.
  {
    const r = await run(`${pageOrigin}/photo.webp`, { kind: "save", target: "png" });
    const out = await downloaded("photo.png");
    check(
      "same-site WebP saves as PNG",
      r.last?.phase === "saved" && out !== null && sniff(out).kind === "png",
    );
    check(
      "toast shows working then saved",
      r.log[0]?.phase === "working" && r.last?.title === "photo.png",
      r.log.map((e) => e.phase).join(" → "),
    );
    check(
      "toast detail names the conversion",
      r.last?.detail.text.startsWith("WebP → PNG · ") === true,
      r.last?.detail.text,
    );
    check(
      "PNG keeps the photo's size",
      out !== null && JSON.stringify(pngSize(out)) === '{"width":640,"height":440}',
    );
  }

  // 2. A host that allows cross-origin reads, to JPG.
  {
    const r = await run(`http://127.0.0.3:${cors.port}/photo.webp`, {
      kind: "save",
      target: "jpg",
    });
    const out = await downloaded("photo.jpg");
    check(
      "CORS host WebP saves as JPG",
      r.last?.phase === "saved" && out !== null && sniff(out).kind === "jpeg",
    );
  }

  // 3. Transparent PNG to JPG gets a white background, not black.
  {
    await run(`${pageOrigin}/transparent.png`, { kind: "save", target: "jpg" });
    const out = await downloaded("transparent.jpg");
    const pixelAt = async (x: number, y: number) =>
      out
        ? new Uint8Array(
            await $`ffmpeg -loglevel error -i ${join(downloads, "transparent.jpg")} -vf crop=2:2:${x}:${y} -f rawvideo -pix_fmt rgb24 -`.arrayBuffer(),
          )
        : new Uint8Array(3);
    const [r, g, b] = await pixelAt(0, 0);
    check(
      "JPG fills transparency with white",
      (r ?? 0) > 245 && (g ?? 0) > 245 && (b ?? 0) > 245,
      `rgb(${r},${g},${b})`,
    );
    const [cr, cg, cb] = await pixelAt(31, 31);
    check(
      "JPG keeps the opaque pixels",
      (cr ?? 0) > 200 && (cg ?? 255) < 60 && (cb ?? 255) < 60,
      `rgb(${cr},${cg},${cb})`,
    );
  }

  // 4. Animated GIF: first frame, and the desktop app's MP4 offer.
  {
    const r = await run(`${pageOrigin}/anim.gif`, { kind: "save", target: "png" });
    const out = await downloaded("anim.png");
    check(
      "animated GIF saves one PNG frame",
      out !== null && sniff(out).kind === "png" && !sniff(out).animated,
    );
    check(
      "animated GIF toast offers MP4",
      r.last?.note?.text.includes("MP4") === true,
      r.last?.note?.text,
    );
  }

  // 5. SVG without a size: rendered from its viewBox at 2x.
  {
    await run(`${pageOrigin}/icon.svg`, { kind: "save", target: "png" });
    const out = await downloaded("icon.png");
    check(
      "SVG rasterizes at 2x its viewBox",
      out !== null && JSON.stringify(pngSize(out)) === '{"width":48,"height":48}',
      out ? JSON.stringify(pngSize(out)) : "missing",
    );
  }

  // 6. data: URL to PNG, big enough that the stored job drops the URL. It's built in
  // the extension tab: a 380 KB string is too long for a command-line argument.
  {
    const r = await inExtension<{ last: { phase: string; title: string } | null }>(`
      const blob = await (await fetch(chrome.runtime.getURL("images/miso.webp"))).blob();
      const srcUrl = await new Promise((resolve) => {
        const reader = new FileReader();
        reader.onload = () => resolve(reader.result);
        reader.readAsDataURL(blob);
      });
      const jobId = crypto.randomUUID();
      await chrome.runtime.sendMessage({ kind: "test:run", job: { id: jobId, srcUrl, pageUrl: ${JSON.stringify(`${pageOrigin}/`)}, tabId: ${tabId}, frameId: 0, action: { kind: "save", target: "png" } } });
      for (let i = 0; i < 100; i++) {
        const [r] = await chrome.scripting.executeScript({ target: { tabId: ${tabId} }, func: (id) => (globalThis.__convtLog ?? []).filter((e) => e.jobId === id).at(-1) ?? null, args: [jobId] });
        if (r?.result && r.result.phase !== "working") return { last: r.result };
        await new Promise((r) => setTimeout(r, 100));
      }
      return { last: null };
    `);
    const out = await downloaded("image.png");
    check(
      "a 280 KB data: URL saves as PNG",
      r.last?.phase === "saved" && out !== null && sniff(out).kind === "png",
      r.last?.title,
    );
  }

  // 7. blob: URL: only the page can read it.
  {
    const blobUrl = await inExtension<string>(`
      const [r] = await chrome.scripting.executeScript({ target: { tabId: ${tabId} }, world: "MAIN", func: () => globalThis.blobUrl });
      return r.result;
    `);
    const r = await run(blobUrl, { kind: "save", target: "png" });
    check("blob: URL converts through the page", r.last?.phase === "saved", r.last?.title);
  }

  // 8. Failures say what happened.
  {
    const heic = await run(`${pageOrigin}/photo.heic`, { kind: "save", target: "png" });
    check(
      "HEIC explains Chrome can't read it",
      heic.last?.title === "Chrome can't read HEIC images" && heic.last?.action?.kind === "link",
      heic.last?.title,
    );
    const html = await run(`${pageOrigin}/not-image.png`, { kind: "save", target: "png" });
    check(
      "a web page at an image URL is reported",
      html.last?.title === "That isn't an image convt can read",
      html.last?.title,
    );
    const missing = await run(`${pageOrigin}/missing.png`, { kind: "save", target: "png" });
    check(
      "a 404 is reported with its status",
      missing.last?.title === "The image didn't load (error 404)",
      missing.last?.title,
    );
  }

  // 8b. The extension's fetch is refused (no cookie); the page's own fetch succeeds.
  {
    const r = await run(`${pageOrigin}/private.webp`, { kind: "save", target: "png" });
    const out = await downloaded("private.png");
    check(
      "a cookie-protected image converts through the page",
      r.last?.phase === "saved" && out !== null && sniff(out).kind === "png",
      r.last?.title,
    );
  }

  // 8c. A blob: image inside another site's frame is out of reach; say so plainly.
  {
    const frame = await inExtension<{ frameId: number; src: string } | null>(`
      const frames = await chrome.webNavigation.getAllFrames({ tabId: ${tabId} });
      const f = frames?.find((x) => x.url.includes("/frame.html"));
      const [r] = await chrome.scripting.executeScript({ target: { tabId: ${tabId} }, world: "MAIN", func: () => globalThis.frameBlobUrl ?? null });
      return f && r?.result ? { frameId: f.frameId, src: r.result } : null;
    `);
    const r = frame
      ? await inExtension<{ last: { title: string } | null }>(`
          const jobId = crypto.randomUUID();
          await chrome.runtime.sendMessage({ kind: "test:run", job: { id: jobId, srcUrl: ${JSON.stringify(frame?.src ?? "")}, pageUrl: ${JSON.stringify(`${pageOrigin}/`)}, tabId: ${tabId}, frameId: ${frame?.frameId ?? 0}, action: { kind: "save", target: "png" } } });
          for (let i = 0; i < 100; i++) {
            const [r] = await chrome.scripting.executeScript({ target: { tabId: ${tabId} }, func: (id) => (globalThis.__convtLog ?? []).filter((e) => e.jobId === id).at(-1) ?? null, args: [jobId] });
            if (r?.result && r.result.phase !== "working") return { last: r.result };
            await new Promise((r) => setTimeout(r, 100));
          }
          return { last: null };
        `)
      : { last: null };
    check(
      "a blob: image in another site's frame explains why",
      r.last?.title === "This image is inside an embedded frame",
      frame ? r.last?.title : "frame not found",
    );
  }

  // 8d. Sources past 45 MB stop early with the desktop offer, not a broken message.
  {
    const r = await run(`${pageOrigin}/oversized.png`, { kind: "save", target: "webp" });
    check(
      "a 46 MB source is refused as too large",
      r.last?.title === "This image is too big to convert in the browser",
      r.last?.title,
    );
  }

  // 8d'. The same limit holds when only the page can fetch the image.
  {
    const r = await run(`${pageOrigin}/private-big.png`, { kind: "save", target: "webp" });
    check(
      "a 46 MB image only the page can fetch is refused as too large",
      r.last?.title === "This image is too big to convert in the browser",
      r.last?.title,
    );
  }

  // 8e. Output past Chrome's 64 MiB message cap downloads through a blob: URL.
  {
    const r = await run(`${pageOrigin}/noise.jpg`, { kind: "save", target: "png" });
    const out = await downloaded("noise.png");
    const size = out?.length ?? 0;
    check(
      "a PNG over 64 MB saves",
      r.last?.phase === "saved" &&
        out !== null &&
        sniff(out).kind === "png" &&
        size > 64 * 1024 * 1024,
      `${(size / 1e6).toFixed(1)} MB, ${r.last?.title}`,
    );
  }

  // 9. A big image still downloads (data: URL path, ~24 megapixels).
  {
    const r = await run(`${pageOrigin}/huge.png`, { kind: "save", target: "webp" });
    const out = await downloaded("huge.webp");
    check(
      "a 6000x4000 PNG converts to WebP",
      r.last?.phase === "saved" && out !== null && sniff(out).kind === "webp",
      r.last?.title,
    );
  }

  // 10. Copy as PNG puts a PNG on the clipboard.
  {
    // After a real right-click the page has focus; the clipboard requires it.
    await inExtension(`await chrome.tabs.update(${tabId}, { active: true }); return true;`);
    const r = await run(`${pageOrigin}/photo.webp`, { kind: "copy" });
    const clip = await inExtension<{ types: string[]; size: number } | string>(`
      const self = await chrome.tabs.getCurrent();
      if (self?.id !== undefined) await chrome.tabs.update(self.id, { active: true });
      await new Promise((r) => setTimeout(r, 200));
      try {
        const [item] = await navigator.clipboard.read();
        const blob = item ? await item.getType("image/png") : null;
        return { types: item ? [...item.types] : [], size: blob ? blob.size : 0 };
      } catch (e) { return String(e); }
    `);
    const ok = r.last?.phase === "copied";
    const copyErrors = await inExtension<unknown[]>(`
      const [res] = await chrome.scripting.executeScript({ target: { tabId: ${tabId} }, func: () => (globalThis.__convtLog ?? []).filter((e) => "copyError" in e) });
      return res?.result ?? [];
    `);
    check("Copy as PNG reports success", ok, `${r.last?.title} ${JSON.stringify(copyErrors)}`);
    check(
      "clipboard holds the PNG",
      typeof clip === "object" && clip.types.includes("image/png") && clip.size > 0,
      JSON.stringify(clip),
    );
  }

  // 11. A host without CORS: the toast asks for access instead of failing silently.
  {
    const src = `http://127.0.0.2:${noCors.port}/photo.webp`;
    const r = await run(src, { kind: "save", target: "png" });
    check(
      "no-CORS host asks for access",
      r.last?.title === "convt needs access to this image" &&
        r.last?.action?.kind === "open-access",
      r.last?.title,
    );
    const pending = await inExtension<boolean>(`
      const all = await chrome.storage.session.get(null);
      return Object.keys(all).includes("pending:${r.jobId}");
    `);
    check("the job waits for access", pending);
    console.log(`ACCESS_JOB=${r.jobId} TAB=${tabId}`);
  }

  // 11b. Two conversions finishing together both land in the recent list.
  {
    const both = await inExtension<boolean>(`
      const ids = [crypto.randomUUID(), crypto.randomUUID()];
      await Promise.all(ids.map((id, i) => chrome.runtime.sendMessage({ kind: "test:run", job: { id, srcUrl: ${JSON.stringify(`${pageOrigin}/photo.webp`)} + "?n=" + i, pageUrl: ${JSON.stringify(`${pageOrigin}/`)}, tabId: ${tabId}, frameId: 0, action: { kind: "save", target: i ? "jpg" : "webp" } } })));
      for (let i = 0; i < 60; i++) {
        await new Promise((r) => setTimeout(r, 200));
        const { recent } = await chrome.storage.local.get("recent");
        const have = (recent ?? []).map((r) => r.jobId);
        if (ids.every((id) => have.includes(id))) return true;
      }
      return false;
    `);
    check("two conversions finishing together both appear in recent files", both);
  }

  // 12. After access is granted, the job resumes on its own. Chrome's permission
  // dialog can't be clicked headless, so this queues a job for a host the test build
  // can already read: the access page sees the access and hands straight back.
  {
    const resumed = await inExtension<{
      phase: string | null;
      accessTabOpen: boolean;
      activeIsPage: boolean;
    }>(`
      const jobId = crypto.randomUUID();
      const job = { id: jobId, srcUrl: ${JSON.stringify(`${pageOrigin}/photo.webp`)}, pageUrl: ${JSON.stringify(`${pageOrigin}/`)}, tabId: ${tabId}, frameId: 0, action: { kind: "save", target: "webp" } };
      await chrome.storage.session.set({ ["pending:" + jobId]: job });
      const access = await chrome.tabs.create({ url: chrome.runtime.getURL("welcome.html?access=" + jobId), active: true });
      let phase = null;
      // This tab is in the background now, where Chrome throttles timers to ~1s.
      for (let i = 0; i < 15 && phase !== "saved"; i++) {
        await new Promise((r) => setTimeout(r, 1000));
        const [r] = await chrome.scripting.executeScript({ target: { tabId: ${tabId} }, func: (id) => (globalThis.__convtLog ?? []).filter((e) => e.jobId === id).at(-1)?.phase ?? null, args: [jobId] });
        phase = r?.result ?? null;
      }
      const accessTabOpen = await chrome.tabs.get(access.id).then(() => true, () => false);
      const page = await chrome.tabs.get(${tabId});
      return { phase, accessTabOpen, activeIsPage: page.active };
    `);
    await focusExtensionTab();
    check(
      "granted access finishes the pending job",
      resumed.phase === "saved",
      String(resumed.phase),
    );
    check(
      "the access tab closes and the user is back on their page",
      !resumed.accessTabOpen && resumed.activeIsPage,
      JSON.stringify(resumed),
    );
  }

  // 12b. Two "access granted" messages for one job save it once.
  {
    const jobId = crypto.randomUUID();
    const started = Date.now();
    await inExtension(`
      const job = { id: ${JSON.stringify(jobId)}, srcUrl: ${JSON.stringify(`${pageOrigin}/photo.webp?once`)}, pageUrl: ${JSON.stringify(`${pageOrigin}/`)}, tabId: ${tabId}, frameId: 0, action: { kind: "save", target: "png" } };
      await chrome.storage.session.set({ ["pending:" + job.id]: job });
      await Promise.all([1, 2].map(() => chrome.runtime.sendMessage({ kind: "access-granted", jobId: job.id })));
      return true;
    `);
    await focusExtensionTab();
    // Give a second save, if there were one, time to land too.
    await Bun.sleep(5000);
    const counts = await inExtension<{ recent: number; downloads: number }>(`
      const { recent } = await chrome.storage.local.get("recent");
      const downloads = await chrome.downloads.search({ startedAfter: new Date(${started}).toISOString() });
      return {
        recent: (recent ?? []).filter((r) => r.jobId === ${JSON.stringify(jobId)}).length,
        downloads: downloads.filter((d) => d.filename.includes("photo")).length,
      };
    `);
    check(
      "a doubled access grant saves the file once",
      counts.recent === 1 && counts.downloads === 1,
      JSON.stringify(counts),
    );
  }

  // 13. Copy as PNG on the welcome page's demo photo (a page the toast can't run on).
  // The clipboard needs a focused document, which a real right-click gives.
  {
    await ab("tab", "new", `chrome-extension://${extensionId}/welcome.html`);
    await Bun.sleep(1000);
    const unfocus = await emulateFocus("/welcome.html");
    const out = await ab(
      "eval",
      `(async () => {
        const tab = await chrome.tabs.getCurrent();
        const before = Date.now();
        await chrome.runtime.sendMessage({ kind: "test:run", job: { id: crypto.randomUUID(), srcUrl: chrome.runtime.getURL("images/miso.webp"), pageUrl: location.href, tabId: tab.id, frameId: 0, action: { kind: "copy" } } });
        let kind = null;
        for (let i = 0; i < 50 && kind === null; i++) {
          await new Promise((r) => setTimeout(r, 100));
          const { recent } = await chrome.storage.local.get("recent");
          if (recent?.[0]?.at >= before) kind = recent[0].kind;
        }
        const types = await navigator.clipboard.read().then((items) => [...(items[0]?.types ?? [])], (e) => [String(e)]);
        const shown = document.querySelector(".below")?.innerText ?? "";
        const { lastFailure } = await chrome.storage.session.get("lastFailure");
        const failure = lastFailure?.at >= before ? lastFailure.view.problem.kind : null;
        return JSON.stringify({ kind, types, shown, failure, focused: document.hasFocus() });
      })()`,
    );
    const copied = JSON.parse(JSON.parse(out.trim())) as {
      kind: string | null;
      types: string[];
      shown: string;
    };
    unfocus();
    await ab("tab", "close");
    await focusExtensionTab();
    check(
      "Copy as PNG works on the welcome page's demo photo",
      copied.kind === "copied" && copied.types.includes("image/png"),
      JSON.stringify(copied),
    );
    check(
      "the welcome page shows the copied result",
      copied.shown.includes("Copied as PNG"),
      copied.shown,
    );
  }

  // 14. Recent list in the popup.
  {
    const recent = await inExtension<number>(`
      const { recent } = await chrome.storage.local.get("recent");
      return Array.isArray(recent) ? recent.length : 0;
    `);
    check("popup's recent list holds the last 8", recent === 8, String(recent));
  }
} catch (error) {
  console.error(error);
  failures.push("harness error");
} finally {
  if (process.env.KEEP) {
    console.log(
      `KEEP: chrome on port ${port}, session ${session}, extension ${extensionId}, page ${pageOrigin}, downloads ${downloads}`,
    );
    console.log(`PIDS chrome=${chrome.pid}`);
  } else {
    await ab("close").catch(() => {});
    chrome.kill();
    page.stop(true);
    noCors.stop(true);
    cors.stop(true);
  }
  exitCode = failures.length > 0 ? 1 : 0;
  console.log(
    failures.length > 0 ? `\n${failures.length} failed: ${failures.join("; ")}` : "\nall passed",
  );
}
if (process.env.KEEP) {
  // Hold the browser and servers until this process is told to stop.
  await new Promise((resolve) => {
    process.on("SIGTERM", resolve);
    process.on("SIGINT", resolve);
  });
  await ab("close").catch(() => {});
  chrome.kill();
}
process.exit(exitCode);
