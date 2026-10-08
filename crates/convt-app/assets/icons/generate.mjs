// Writes the Hugeicons the desktop app draws into ./hugeicons, as plain SVGs
// GPUI can load. Run it after adding a name to ICONS:
//
//   node crates/convt-app/assets/icons/generate.mjs
//
// It downloads the pinned free package (MIT, stroke-rounded style) into a
// temporary directory, so nothing is added to the workspace's dependencies.
import { execFileSync } from "node:child_process";
import { mkdtempSync, mkdirSync, readdirSync, rmSync, writeFileSync, copyFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, dirname } from "node:path";
import { createRequire } from "node:module";
import { fileURLToPath } from "node:url";

const VERSION = "4.3.5";

// The file name the app loads, and the Hugeicons export it comes from.
const ICONS = {
  "add": "Add01Icon",
  "alert-circle": "AlertCircleIcon",
  "alert-triangle": "Alert02Icon",
  "arrow-down": "ArrowDown02Icon",
  "arrow-right": "ArrowRight02Icon",
  "calendar": "Calendar03Icon",
  "cancel": "Cancel01Icon",
  "cancel-circle": "CancelCircleIcon",
  "check": "Tick02Icon",
  "check-circle": "CheckmarkCircle02Icon",
  "chevron-down": "ArrowDown01Icon",
  "chevron-right": "ArrowRight01Icon",
  "chevrons-up-down": "UnfoldMoreIcon",
  "cloud": "CloudIcon",
  "computer": "ComputerIcon",
  "document": "File02Icon",
  "download": "Download04Icon",
  "edit": "PencilEdit02Icon",
  "external-link": "LinkSquare02Icon",
  "folder": "Folder01Icon",
  "folder-open": "Folder02Icon",
  "google": "GoogleIcon",
  "hard-drive": "HardDriveIcon",
  "inbox": "InboxIcon",
  "info": "InformationCircleIcon",
  "key": "Key01Icon",
  "loading": "Loading03Icon",
  "mail": "Mail01Icon",
  "magic-wand": "MagicWand01Icon",
  "minus": "MinusSignIcon",
  "refresh": "RefreshIcon",
  "restore": "Copy02Icon",
  "rotate": "RotateClockwiseIcon",
  "settings": "Settings02Icon",
  "sparkles": "SparklesIcon",
  "square": "SquareIcon",
  "star": "StarIcon",
  "user-circle": "UserCircleIcon",
};

const here = dirname(fileURLToPath(import.meta.url));
const out = join(here, "hugeicons");
const work = mkdtempSync(join(tmpdir(), "hugeicons-"));
try {
  execFileSync("npm", ["pack", `@hugeicons/core-free-icons@${VERSION}`, "--silent"], { cwd: work });
  const tarball = readdirSync(work).find((f) => f.endsWith(".tgz"));
  execFileSync("tar", ["xzf", tarball], { cwd: work });
  const pkg = createRequire(join(work, "package", "package.json"))(join(work, "package", "dist", "cjs", "index.js"));
  rmSync(out, { recursive: true, force: true });
  mkdirSync(out);
  const attr = (k) => k.replace(/[A-Z]/g, (c) => `-${c.toLowerCase()}`);
  for (const [file, name] of Object.entries(ICONS)) {
    const nodes = pkg[name];
    if (!nodes) throw new Error(`no icon ${name} in @hugeicons/core-free-icons@${VERSION}`);
    const body = nodes
      .map(([tag, attrs]) => {
        const a = Object.entries(attrs)
          .filter(([k]) => k !== "key")
          .map(([k, v]) => `${attr(k)}="${v}"`)
          .join(" ");
        return `  <${tag} ${a} />`;
      })
      .join("\n");
    writeFileSync(
      join(out, `${file}.svg`),
      `<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none">\n${body}\n</svg>\n`,
    );
  }
  copyFileSync(join(work, "package", "LICENSE.md"), join(out, "LICENSE"));
  console.log(`wrote ${Object.keys(ICONS).length} icons to ${out}`);
} finally {
  rmSync(work, { recursive: true, force: true });
}
