#!/usr/bin/env bash
# bun run dev:web: the website with its local services.
#
#   1. scripts/db.sh up (this checkout's Postgres and Mailpit) and migrations
#   2. the OAuth mock (tools/oauth-mock) on OAUTH_MOCK_PORT from services.env
#   3. the billing mock (tools/billing-mock: Polar and Resend) on BILLING_MOCK_PORT,
#      delivering signed webhooks to the site's /webhooks/polar
#   4. apps/web/.dev.vars and apps/billing/.dev.vars from .convt-dev/services.env
#      (plus CONVT_API_URL and CONVT_WEB_TOKEN_SECRET when both are set);
#      convt-billing signs with the local dev key in .convt-dev/license.key
#   5. Vite on port 3000, or the next free one, with BETTER_AUTH_URL to match;
#      convt-billing runs inside it as an auxiliary Worker
#
# Seed the fixture accounts once with `bun run db:seed`, then mirror them into the
# billing mock with `bun tools/billing-mock/src/preload.ts`. Ctrl-C stops Vite and
# the mocks; the containers keep running (scripts/db.sh down removes them).
#
# PORT picks the first port to try. PUBLIC_ORIGIN (such as a tailnet URL) replaces
# http://localhost:<port> as the site origin and binds Vite to all interfaces;
# MOCK_PUBLIC_URL does the same for the OAuth mock's authorize page.

set -euo pipefail
ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)
export PATH=$HOME/.bun/bin:$PATH
cd "$ROOT"

bash scripts/db.sh up
bun packages/db/src/cli/migrate.ts
set -a
# shellcheck disable=SC1091
. .convt-dev/services.env
set +a

port_free() { ! (exec 3<>"/dev/tcp/127.0.0.1/$1") 2>/dev/null; }

MOCK_PID=
BILLING_MOCK_PID=
cleanup() {
  [[ -n $MOCK_PID ]] && kill "$MOCK_PID" 2>/dev/null || true
  [[ -n $BILLING_MOCK_PID ]] && kill "$BILLING_MOCK_PID" 2>/dev/null || true
}
trap cleanup EXIT INT TERM
if port_free "$OAUTH_MOCK_PORT"; then
  MOCK_HOST=${MOCK_HOST:-127.0.0.1} MOCK_PORT=$OAUTH_MOCK_PORT MOCK_PUBLIC_URL=${MOCK_PUBLIC_URL:-} \
    bun tools/oauth-mock/src/main.ts &
  MOCK_PID=$!
elif curl -fsS "http://127.0.0.1:$OAUTH_MOCK_PORT/health" >/dev/null 2>&1; then
  echo "dev-web: reusing the OAuth mock already on $OAUTH_MOCK_PORT"
else
  echo "dev-web: port $OAUTH_MOCK_PORT is taken by something else; remove OAUTH_MOCK_PORT from services.env and run again" >&2
  exit 1
fi

port=${PORT:-3000}
while ! port_free "$port"; do port=$((port + 1)); done
origin=${PUBLIC_ORIGIN:-http://localhost:$port}
host_args=()
[[ -n ${PUBLIC_ORIGIN:-} ]] && host_args=(--host 0.0.0.0)

# The billing mock gets a fresh token per run; it only ever talks to this checkout.
[[ -f .convt-dev/license.key ]] || bun -e 'await (await import("./packages/db/src/seed.ts")).devSigningKey()'
POLAR_MOCK_TOKEN=polar_oat_local_$(openssl rand -hex 12)
RESEND_MOCK_KEY=re_local_$(openssl rand -hex 12)
if ! port_free "$BILLING_MOCK_PORT"; then
  echo "dev-web: port $BILLING_MOCK_PORT is taken; stop the other billing mock or remove BILLING_MOCK_PORT from services.env" >&2
  exit 1
fi
MOCK_PORT=$BILLING_MOCK_PORT MOCK_PUBLIC_URL=${BILLING_MOCK_PUBLIC_URL:-} \
  POLAR_ACCESS_TOKEN=$POLAR_MOCK_TOKEN RESEND_API_KEY=$RESEND_MOCK_KEY \
  POLAR_WEBHOOK_SECRET=$POLAR_WEBHOOK_SECRET WEBHOOK_URL=http://127.0.0.1:$port/webhooks/polar \
  WEBHOOK_SCHEME=${WEBHOOK_SCHEME:-standard} MAILPIT_URL=$MAILPIT_URL \
  bun tools/billing-mock/src/main.ts &
BILLING_MOCK_PID=$!

umask 077
cat >apps/web/.dev.vars <<VARS
# Written by scripts/dev-web.sh from .convt-dev/services.env. Never commit.
ENV=development
SALES=${SALES:-all}
BETTER_AUTH_URL=$origin
BETTER_AUTH_SECRET=$BETTER_AUTH_SECRET
MAIL_TRANSPORT=mailpit
MAIL_FROM=convt <hello@convt.test>
MAILPIT_URL=$MAILPIT_URL
OAUTH_MOCK_URL=$OAUTH_MOCK_URL
OAUTH_MOCK_PUBLIC_URL=${MOCK_PUBLIC_URL:-$OAUTH_MOCK_URL}
VARS
# A local cloud API (test-convt-server): pass both to let the dashboard
# converter and the desktop app's cloud jobs reach it.
if [[ -n ${CONVT_API_URL:-} && -n ${CONVT_WEB_TOKEN_SECRET:-} ]]; then
  printf 'CONVT_API_URL=%s\nCONVT_WEB_TOKEN_SECRET=%s\n' "$CONVT_API_URL" "$CONVT_WEB_TOKEN_SECRET" >>apps/web/.dev.vars
fi
cat >apps/billing/.dev.vars <<VARS
# Written by scripts/dev-web.sh. Local billing mock and the dev signing key. Never commit.
ENV=development
SITE_URL=$origin
BILLING_CATALOG=local
POLAR_ACCESS_TOKEN=$POLAR_MOCK_TOKEN
POLAR_API_URL=$BILLING_MOCK_URL
POLAR_PORTAL_ORIGIN=${BILLING_MOCK_PUBLIC_URL:-$BILLING_MOCK_URL}
POLAR_WEBHOOK_SECRET=$POLAR_WEBHOOK_SECRET
MAIL_TRANSPORT=resend
RESEND_API_KEY=$RESEND_MOCK_KEY
RESEND_API_URL=$BILLING_MOCK_URL
MAIL_FROM=convt <hello@convt.test>
ALERT_EMAIL=alerts@convt.test
LICENSE_SIGNING_KEY=$(tr -d '\n' <.convt-dev/license.key)
VARS

for _ in $(seq 40); do curl -fsS "$BILLING_MOCK_URL/health" >/dev/null 2>&1 && break; sleep 0.25; done
bun tools/billing-mock/src/preload.ts >/dev/null 2>&1 || true

echo "dev-web: $origin (Mailpit $MAILPIT_URL, OAuth mock $OAUTH_MOCK_URL, billing mock $BILLING_MOCK_URL)"
cd apps/web
CLOUDFLARE_HYPERDRIVE_LOCAL_CONNECTION_STRING_HYPERDRIVE=$DATABASE_URL \
  CLOUDFLARE_HYPERDRIVE_LOCAL_CONNECTION_STRING_HYPERDRIVE_BILLING=$BILLING_DATABASE_URL \
  bunx vite dev --port "$port" --strictPort "${host_args[@]}"
