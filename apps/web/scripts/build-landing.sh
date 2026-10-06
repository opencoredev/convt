#!/usr/bin/env bash
# Build the coming-soon landing page as static files for Vercel.
#
# Prerenders only "/" (CONVT_LANDING_ONLY in vite.config.ts) and leaves the result in
# dist/client. The prerender boots the Worker locally, which insists on its bindings and
# auth settings even though "/" uses none of them, so this passes throwaway development
# values. None of them end up in the output.
set -euo pipefail
cd "$(dirname "$0")/.."

(cd ../../packages/sdk && bun run build >/dev/null)

unused_db="postgres://prerender:unused@127.0.0.1:1/unused"
export CONVT_LANDING_ONLY=1
export CLOUDFLARE_INCLUDE_PROCESS_ENV=true
export CLOUDFLARE_HYPERDRIVE_LOCAL_CONNECTION_STRING_HYPERDRIVE="$unused_db"
export CLOUDFLARE_HYPERDRIVE_LOCAL_CONNECTION_STRING_HYPERDRIVE_BILLING="$unused_db"
export ENV=development
export BETTER_AUTH_URL=http://localhost:3000
BETTER_AUTH_SECRET="$(head -c 32 /dev/urandom | base64)"
export BETTER_AUTH_SECRET
export MAIL_TRANSPORT=mailpit
export MAIL_FROM="convt <hello@convt.test>"
export MAILPIT_URL=http://127.0.0.1:8025

bun run build

test -f dist/client/index.html
echo "Static landing page: $(pwd)/dist/client"
