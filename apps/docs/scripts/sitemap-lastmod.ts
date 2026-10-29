// Adds <lastmod> to the sitemap URLs Blume cannot date. `lastModified: "git"` dates every
// page written under content/, but the API reference is generated from the OpenAPI spec and
// content/reference/formats.mdx is generated and gitignored, so Blume lists them undated.
// Each one gets the date of the newest commit to the files its page is built from, never the
// build time. Runs after `blume build`. scripts/deploy.sh sets SITEMAP_LASTMOD_STRICT=1 so a
// deploy fails instead of shipping undated URLs, or dates from a shallow clone, where git
// reports the newest commit for every file.
type GeneratedPages = {
  matches: (path: string) => boolean;
  sources: readonly string[];
};

const generated: readonly GeneratedPages[] = [
  {
    matches: (path) => path === "/docs/api" || path.startsWith("/docs/api/"),
    sources: [
      "../../crates/convt-server/openapi.json",
      "openapi/public.yaml",
      "api-host.ts",
      "scripts/generate.ts",
    ],
  },
  {
    matches: (path) => path === "/docs/reference/formats",
    sources: ["../../crates/convt-server/cloud-formats.json", "scripts/generate-formats.ts"],
  },
];

const docsRoot = new URL("..", import.meta.url).pathname;
const sitemap = new URL("../dist/sitemap.xml", import.meta.url);

const git = (args: readonly string[]) =>
  Bun.spawnSync(["git", ...args], { cwd: docsRoot, stdout: "pipe", stderr: "ignore" });

// YYYY-MM-DD (UTC, like Blume's own <lastmod>) of the newest commit touching `sources`,
// or null outside a git checkout.
const lastCommitDay = (sources: readonly string[]): string | null => {
  const log = git(["log", "-1", "--format=%cI", "--", ...sources]);
  const iso = log.exitCode === 0 ? log.stdout.toString().trim() : "";
  return iso ? new Date(iso).toISOString().slice(0, 10) : null;
};

const days = generated.map((pages) => ({ ...pages, day: lastCommitDay(pages.sources) }));
const undated: string[] = [];
const xml = (await Bun.file(sitemap).text()).replace(
  /<url><loc>([^<]+)<\/loc><\/url>/g,
  (entry, loc: string) => {
    const path = new URL(loc).pathname;
    const day = days.find((pages) => pages.matches(path))?.day;
    if (!day) {
      undated.push(path);
      return entry;
    }
    return `<url><loc>${loc}</loc><lastmod>${day}</lastmod></url>`;
  },
);

await Bun.write(sitemap, xml);
const shallow = git(["rev-parse", "--is-shallow-repository"]).stdout.toString().trim() === "true";
const problems = [
  ...(shallow ? ["shallow git clone, so every date is the newest commit's"] : []),
  ...(undated.length > 0 ? [`no lastmod for ${undated.join(", ")}`] : []),
];
if (problems.length === 0) console.log("sitemap.xml: every URL has a lastmod");
else if (process.env.SITEMAP_LASTMOD_STRICT === "1") {
  console.error(`sitemap.xml: ${problems.join("; ")}`);
  process.exit(1);
} else console.warn(`sitemap.xml: ${problems.join("; ")}`);
