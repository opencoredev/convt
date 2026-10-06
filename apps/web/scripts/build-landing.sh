#!/usr/bin/env bash
# Build the coming-soon landing page as static files for Vercel.
#
# Prerenders only "/" (CONVT_LANDING_ONLY in vite.config.ts) and leaves the result in
# dist/client. The prerender boots the Worker locally, which insists on its bindings and
# auth settings even though "/" uses none of them, so this passes throwaway development
# values. scripts/landing-static.py then adds the agent files, the About, Privacy and
# 404 pages, and vercel.json.
#
# CLOUDFLARE_INCLUDE_PROCESS_ENV copies the process environment into dist/*/.dev.vars,
# so the build runs with an empty environment plus an allowlist, and those files are
# deleted afterwards. Only dist/client is deployed.
set -euo pipefail
cd "$(dirname "$0")/.."

(cd ../../packages/sdk && bun run build >/dev/null)

unused_db="postgres://prerender:unused@127.0.0.1:1/unused"
env -i \
  PATH="$PATH" HOME="$HOME" TMPDIR="${TMPDIR:-/tmp}" LANG="${LANG:-C.UTF-8}" \
  CONVT_LANDING_ONLY=1 \
  CLOUDFLARE_INCLUDE_PROCESS_ENV=true \
  CLOUDFLARE_HYPERDRIVE_LOCAL_CONNECTION_STRING_HYPERDRIVE="$unused_db" \
  CLOUDFLARE_HYPERDRIVE_LOCAL_CONNECTION_STRING_HYPERDRIVE_BILLING="$unused_db" \
  ENV=development \
  BETTER_AUTH_URL=http://localhost:3000 \
  BETTER_AUTH_SECRET="$(head -c 32 /dev/urandom | base64)" \
  MAIL_TRANSPORT=mailpit \
  MAIL_FROM="convt <hello@convt.test>" \
  MAILPIT_URL=http://127.0.0.1:8025 \
  bun run build
find dist -name '.dev.vars*' -delete

test -f dist/client/index.html
python3 scripts/landing-static.py dist/client
bun scripts/landing-pages.ts dist/client
echo "Static landing page: $(pwd)/dist/client"
