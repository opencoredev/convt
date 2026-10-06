#!/usr/bin/env bash
# Runs a command against a disposable Postgres:
#
#   packages/db/scripts/test-db.sh <command> [args...]
#
# Starts a postgres:17-alpine container on tmpfs with a random 127.0.0.1 port and
# the label convt.test-run=<random id>, creates the roles, migrates one template
# database (convt_template) as convt_owner, runs the command, and removes exactly
# that container on exit. The command gets:
#
#   TEST_PG_ADMIN_URL   superuser URL to the `postgres` database
#   TEST_PG_HOST, TEST_PG_PORT, TEST_TEMPLATE_DB
#   TEST_OWNER_PASSWORD, TEST_WEB_PASSWORD, TEST_SERVER_PASSWORD, TEST_BILLING_PASSWORD
#   CONVT_TEST_DATABASE_URL  same as TEST_PG_ADMIN_URL, for convt-server's tests
#
# Each test file or Rust test then creates its own database from the template
# (see src/testing.ts and convt-server's test helper).
#
# Set TEST_DB_SKIP_MIGRATE=1 to leave the template empty (db:ci builds its own).

set -euo pipefail

ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd -P)
IMAGE=postgres:17-alpine@sha256:b0f9560a2de083e2cc7382e75f808c7381a32852a7ec49117deedb300e552b24
RUN_ID=p6-$(openssl rand -hex 6)
ADMIN_PW=$(openssl rand -hex 24)
export TEST_OWNER_PASSWORD=$(openssl rand -hex 24)
export TEST_WEB_PASSWORD=$(openssl rand -hex 24)
export TEST_SERVER_PASSWORD=$(openssl rand -hex 24)
export TEST_BILLING_PASSWORD=$(openssl rand -hex 24)

[[ $# -gt 0 ]] || { sed -n '2,20p' "$0" | sed 's/^# \{0,1\}//'; exit 2; }

CID=$(docker run -d --rm --label "convt.test-run=$RUN_ID" \
  --tmpfs /var/lib/postgresql/data:rw,size=1g \
  -e POSTGRES_PASSWORD="$ADMIN_PW" -e POSTGRES_DB=convt_template \
  -p 127.0.0.1::5432 "$IMAGE" \
  -c fsync=off -c synchronous_commit=off -c full_page_writes=off -c max_connections=300)
cleanup() { docker rm -f "$CID" >/dev/null 2>&1 || true; }
trap cleanup EXIT INT TERM

for _ in $(seq 120); do
  # pg_isready over TCP: the entrypoint's temporary server listens on the socket only.
  docker exec "$CID" pg_isready -q -h 127.0.0.1 -U postgres -d convt_template 2>/dev/null && break
  sleep 0.25
done
docker exec "$CID" pg_isready -q -h 127.0.0.1 -U postgres -d convt_template || { echo "test-db: Postgres did not start" >&2; exit 1; }

export TEST_PG_HOST=127.0.0.1
TEST_PG_PORT=$(docker port "$CID" 5432/tcp | sed -n 's/^127\.0\.0\.1:\([0-9]*\)$/\1/p' | head -1)
export TEST_PG_PORT
export TEST_TEMPLATE_DB=convt_template
export TEST_PG_ADMIN_URL="postgresql://postgres:$ADMIN_PW@127.0.0.1:$TEST_PG_PORT/postgres"
export CONVT_TEST_DATABASE_URL=$TEST_PG_ADMIN_URL
export TEST_PG_CONTAINER=$CID

docker exec -i -e PGOPTIONS=-cclient_min_messages=warning "$CID" psql -q -v ON_ERROR_STOP=1 -U postgres -d convt_template \
  -v owner_password="$TEST_OWNER_PASSWORD" -v web_password="$TEST_WEB_PASSWORD" \
  -v server_password="$TEST_SERVER_PASSWORD" -v billing_password="$TEST_BILLING_PASSWORD" \
  <"$ROOT/packages/db/roles.sql"

if [[ ${TEST_DB_SKIP_MIGRATE:-} != 1 ]]; then
  MIGRATE_DATABASE_URL="postgresql://convt_owner:$TEST_OWNER_PASSWORD@127.0.0.1:$TEST_PG_PORT/convt_template" \
    bun "$ROOT/packages/db/src/cli/migrate.ts" >/dev/null
fi

set +e
"$@"
status=$?
exit $status
