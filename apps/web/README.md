# @convt/web

The convt.app website: TanStack Start on Cloudflare Workers.

```sh
bun run dev      # http://localhost:3000
bun run build
bun run deploy   # wrangler deploy (needs Cloudflare auth)
```

## Public page content

The public pages read files in `content/` and `src/generated/` at build time. `bun run build` runs `scripts/generate-content.ts` first.

| File                            | Page         | Written by                                                                                                                                                          |
| ------------------------------- | ------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `content/release-manifest.json` | `/download`  | The release pipeline. Copy the unsigned `release-manifest.json` from `scripts/release/manifest.ts generate`; its shape is `packaging/release/manifest.schema.json`. |
| `content/formats.json`          | `/formats`   | `scripts/generate-content.ts`, from `convt formats --json`, `convt engines` and `convt targets`.                                                                    |
| `content/changelog.md`          | `/changelog` | By hand. Each `## ` heading is a release.                                                                                                                           |

The release manifest is optional. Without it, or while its `distribution_ready` is false, download buttons show "Shipping today" and nothing is linked. With it, the page lists the newest build's artifacts by platform and kind, and keeps the expected slots (macOS `dmg`, Windows `msi`, Linux `AppImage`, `deb`, `rpm`, `tar.gz`) that the build lacks as "Shipping today".

The formats list comes from the registry on the building machine, and `default_registry()` registers only engines that run there. If any engine is unavailable, the script keeps the committed `content/formats.json` and warns. Set `CONVT_FORMATS_STRICT=1` in release builds to fail instead. When `CONVT_LIBHEIF_DIR` is unset and a Linux bundle exists in `packaging/out/convt/lib`, the script uses its libheif. `CONVT_BIN` picks a prebuilt `convt`; `CONVT_FORMATS_SKIP=1` skips regeneration. Turbo caches the build by this package's files, so a crate change alone does not rerun it; run `bun run generate-content` after changing formats or routes.

Placeholder facts (support and privacy addresses, the legal entity, address, jurisdiction and effective date) live in `src/lib/site.ts`.

## Public pages and their content

The landing page is `src/routes/index.tsx` (dark only). The other public pages live under `src/routes/_site/` and follow the visitor's light or dark setting. Their links, the support addresses and the legal placeholders are in `src/lib/site.ts`.

`bun run build` runs `scripts/generate-content.ts` before Vite. It reads these files:

- `content/formats.json`: regenerated from `convt formats --json`, `convt engines` and `convt targets` for each format. The script builds `convt-cli` (or uses `CONVT_BIN`) and loads libheif from `packaging/out/convt/lib` when `CONVT_LIBHEIF_DIR` is unset. If any engine is unavailable, it keeps the committed file rather than publish a shorter list. Set `CONVT_FORMATS_STRICT=1` in release builds to fail instead, or `CONVT_FORMATS_SKIP=1` to leave the file alone.
- `content/release-manifest.json`: optional. Copy the unsigned `release-manifest.json` that `scripts/release/manifest.ts generate` writes (schema in `packaging/release/manifest.schema.json`). Without the file, or while `distribution_ready` is false, download buttons show "Shipping today". The download page lists the newest build.
- `content/changelog.md`: the changelog page. Each `## ` heading is a release.

Everything under `/docs`, including the API reference at `/docs/api`, is the Blume site in `apps/docs`, deployed as its own Worker. This app has no `/docs` routes.

## Sign-in providers

Email sign-in always works. Google and GitHub show on `/sign-in` and the desktop sign-in page (`/device`) only when the Worker has both the client ID and the secret for that provider (`availableProviders` in `src/server/env.ts`). With one missing, the button stays hidden and sign-in falls back to email. Locally, `bun run dev:web` uses the OAuth mock in `tools/oauth-mock` for both.

To turn on Google for a deployed Worker:

1. In Google Cloud Console, under Google Auth Platform, set up the consent screen (app name convt, support email, the `convt.app` domain, links to `https://convt.app/privacy` and `https://convt.app/terms`) and publish it to production. The default scopes `openid`, `email` and `profile` need no verification.
2. Create an OAuth client of type Web application:
   - Authorized JavaScript origin: `https://convt.app`
   - Authorized redirect URI: `https://convt.app/api/auth/callback/google`
   - For staging, a second client (or the same one) with `https://convt-web-staging.leoisadev.workers.dev` and `https://convt-web-staging.leoisadev.workers.dev/api/auth/callback/google`.
3. Store the pair on the Worker. Both may be secrets; the Worker reads vars and secrets the same way.

   ```sh
   cd apps/web
   bunx wrangler secret put GOOGLE_CLIENT_ID
   bunx wrangler secret put GOOGLE_CLIENT_SECRET
   # staging: add --env staging to both
   ```

   `wrangler secret put` deploys a new version, so "Continue with Google" appears on the next page load. Check with `bunx wrangler secret list`, which prints names only.

GitHub works the same way with `GITHUB_CLIENT_ID` and `GITHUB_CLIENT_SECRET` and the callback `https://convt.app/api/auth/callback/github`.
