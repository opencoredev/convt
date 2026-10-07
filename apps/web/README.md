# @convt/web

The convt.app website: TanStack Start on Cloudflare Workers.

```sh
bun run dev      # http://localhost:3000
bun run build
bun run deploy   # wrangler deploy (needs Cloudflare auth)
```

## Public page content

The public pages read files in `content/` and `src/generated/` at build time. `bun run build` runs `scripts/generate-content.ts` first.

| File                               | Page         | Written by                                                                                                                                                          |
| ---------------------------------- | ------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `content/release-manifest.json`    | `/download`  | The release pipeline. Copy the unsigned `release-manifest.json` from `scripts/release/manifest.ts generate`; its shape is `packaging/release/manifest.schema.json`. |
| `content/formats.json`             | `/formats`   | `scripts/generate-content.ts`, from `convt formats --json`, `convt engines` and `convt targets`.                                                                    |
| `content/changelog.md`             | `/changelog` | By hand. Each `## ` heading is a release.                                                                                                                           |
| `src/generated/openapi.json`       | `/docs/api`  | The API work (P9), generated from convt-server.                                                                                                                     |
| `content/openapi.placeholder.json` | `/docs/api`  | Used only while `src/generated/openapi.json` is missing; the page then says the API is a preview.                                                                   |

The release manifest is optional. Without it, or while its `distribution_ready` is false, download buttons show "Coming soon" and nothing is linked. With it, the page lists the newest build's artifacts by platform and kind, and keeps the expected slots (macOS `dmg`, Windows `msi`, Linux `AppImage`, `deb`, `rpm`, `tar.gz`) that the build lacks as "Coming soon".

The formats list comes from the registry on the building machine, and `default_registry()` registers only engines that run there. If any engine is unavailable, the script keeps the committed `content/formats.json` and warns. Set `CONVT_FORMATS_STRICT=1` in release builds to fail instead. When `CONVT_LIBHEIF_DIR` is unset and a Linux bundle exists in `packaging/out/convt/lib`, the script uses its libheif. `CONVT_BIN` picks a prebuilt `convt`; `CONVT_FORMATS_SKIP=1` skips regeneration. Turbo caches the build by this package's files, so a crate change alone does not rerun it; run `bun run generate-content` after changing formats or routes.

Placeholder facts (support and privacy addresses, the legal entity, address, jurisdiction and effective date) live in `src/lib/site.ts`.

## Public pages and their content

The landing page is `src/routes/index.tsx` (dark only). The other public pages live under `src/routes/_site/` and follow the visitor's light or dark setting. Their links, the support addresses and the legal placeholders are in `src/lib/site.ts`.

`bun run build` runs `scripts/generate-content.ts` before Vite. It reads these files:

- `content/formats.json`: regenerated from `convt formats --json`, `convt engines` and `convt targets` for each format. The script builds `convt-cli` (or uses `CONVT_BIN`) and loads libheif from `packaging/out/convt/lib` when `CONVT_LIBHEIF_DIR` is unset. If any engine is unavailable, it keeps the committed file rather than publish a shorter list. Set `CONVT_FORMATS_STRICT=1` in release builds to fail instead, or `CONVT_FORMATS_SKIP=1` to leave the file alone.
- `content/release-manifest.json`: optional. Copy the unsigned `release-manifest.json` that `scripts/release/manifest.ts generate` writes (schema in `packaging/release/manifest.schema.json`). Without the file, or while `distribution_ready` is false, download buttons show "Coming soon". The download page lists the newest build.
- `content/changelog.md`: the changelog page. Each `## ` heading is a release.

The API reference at `/docs/api` renders `src/generated/openapi.json`, which the API work generates from convt-server. When that file is missing it falls back to `content/openapi.placeholder.json` and shows a preview notice.
