#!/usr/bin/env bash
# bun run db:ci: the schema drift check. Runs locally until the GitHub repository
# exists, then as a CI job. Everything happens in one disposable Postgres
# (scripts/test-db.sh) that is removed on exit.
#
#   1. disposable Postgres and roles
#   2. drizzle-kit check; drizzle-kit generate in a temporary copy adds no migration
#   3. database A from the migrations; database B from the Drizzle schema alone
#      (drizzle-kit export) plus sql/privileges.sql; their schema dumps match
#   4. on A: every down file leaves only an empty drizzle schema; migrating up
#      again gives the first dump
#   5. integration tests (packages/db, apps/web)
#   6. convt-server: sqlx prepare --check, an offline build, the database tests

set -euo pipefail
ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd -P)
export PATH=$HOME/.cargo/bin:$HOME/.bun/bin:$PATH

if [[ ${1:-} != --inner ]]; then
  # The template stays empty until step 5; steps 2 to 4 build their own databases.
  TEST_DB_SKIP_MIGRATE=1 exec bash "$ROOT/packages/db/scripts/test-db.sh" bash "$0" --inner
fi

DB_DIR=$ROOT/packages/db
WORK=$(mktemp -d /tmp/convt-db-ci.XXXXXX)
trap 'rm -rf "$WORK"' EXIT
step() { printf '\n== db:ci %s\n' "$*"; }
admin() { docker exec -i -e PGOPTIONS=-cclient_min_messages=warning "$TEST_PG_CONTAINER" psql -q -v ON_ERROR_STOP=1 -U postgres "$@"; }
owner_url() { echo "postgresql://convt_owner:$TEST_OWNER_PASSWORD@127.0.0.1:$TEST_PG_PORT/$1"; }

dump() {
  # pg_dump from the container, so the client matches the server version. The
  # drizzle and convt_dev schemas are bookkeeping, not schema.
  docker exec "$TEST_PG_CONTAINER" pg_dump -U postgres --schema-only --no-owner \
    --exclude-schema=drizzle --exclude-schema=convt_dev "$1" |
    python3 -c '
import re, sys
text = sys.stdin.read()
text = "\n".join(l for l in text.splitlines() if not l.startswith("--") and not l.startswith("\\restrict") and not l.startswith("\\unrestrict"))
stmts = [re.sub(r"\s+", " ", s).strip() for s in text.split(";\n")]
stmts = [s for s in stmts if s and not s.startswith("SET ") and not s.startswith("SELECT pg_catalog.set_config")]
print("\n".join(sorted(stmts)))
'
}

step "1: disposable Postgres on 127.0.0.1:$TEST_PG_PORT, roles created"

step "2: drizzle-kit check and generate"
(cd "$DB_DIR" && bunx drizzle-kit check)
COPY=$WORK/db-copy
mkdir -p "$COPY"
cp -r "$DB_DIR/src" "$DB_DIR/migrations" "$DB_DIR/drizzle.config.ts" "$DB_DIR/package.json" "$COPY/"
ln -s "$DB_DIR/node_modules" "$COPY/node_modules"
before=$(cd "$COPY/migrations" && find . -type f | sort | xargs sha256sum)
(cd "$COPY" && bunx drizzle-kit generate --name drift >"$WORK/generate.log" 2>&1) || { cat "$WORK/generate.log"; exit 1; }
after=$(cd "$COPY/migrations" && find . -type f | sort | xargs sha256sum)
if [[ $before != "$after" ]]; then
  echo "db:ci: the schema has changes without a migration:" >&2
  diff <(echo "$before") <(echo "$after") >&2 || true
  exit 1
fi
echo "no new migration"

step "3: migrations (A) against the schema alone (B)"
admin -d postgres -c 'create database ci_a owner convt_owner' -c 'create database ci_b owner convt_owner'
MIGRATE_DATABASE_URL=$(owner_url ci_a) bun "$DB_DIR/src/cli/migrate.ts"
(cd "$DB_DIR" && bunx drizzle-kit export --sql 2>/dev/null) >"$WORK/export.sql"
# drizzle-kit export ignores `casing: "snake_case"`; apply the same conversion to
# quoted identifiers. Any mismatch it caused would show up in the comparison below.
python3 - "$WORK/export.sql" <<'PY'
import re, sys
p = sys.argv[1]
s = open(p).read()
s = re.sub(r'"([A-Za-z0-9_]*[A-Z][A-Za-z0-9_]*)"', lambda m: '"' + re.sub(r"([a-z0-9])([A-Z])", r"\1_\2", m.group(1)).lower() + '"', s)
open(p, "w").write(s)
PY
{
  cat "$WORK/export.sql"
  printf '\n--> statement-breakpoint\n'
  cat "$DB_DIR/sql/privileges.sql"
} | sed 's/--> statement-breakpoint/;/' >"$WORK/b.sql"
psql "$(owner_url ci_b)" -q -v ON_ERROR_STOP=1 -f "$WORK/b.sql" >/dev/null
dump ci_a >"$WORK/a.dump"
dump ci_b >"$WORK/b.dump"
if ! diff -u "$WORK/a.dump" "$WORK/b.dump" >"$WORK/ab.diff"; then
  echo "db:ci: the migrations and the schema disagree:" >&2
  cat "$WORK/ab.diff" >&2
  exit 1
fi
echo "A and B match ($(wc -l <"$WORK/a.dump") statements)"

step "4: down files reverse everything; up again reproduces A"
(cd "$DB_DIR" && OWNER=$(owner_url ci_a) bun -e '
import pg from "pg";
import { rollbackOne } from "./src/migrations";
const c = new pg.Client({ connectionString: process.env.OWNER });
await c.connect();
let tag;
while ((tag = await rollbackOne(c))) console.log("reversed", tag);
await c.end();
')
left=$(psql "$(owner_url ci_a)" -Atc "
  select string_agg(n, ',') from (
    select nspname || '.' || relname as n from pg_class c join pg_namespace s on s.oid = c.relnamespace
    where nspname not in ('pg_catalog', 'information_schema', 'pg_toast', 'drizzle')
    union all
    select nspname || '.' || proname from pg_proc p join pg_namespace s on s.oid = p.pronamespace
    where nspname not in ('pg_catalog', 'information_schema', 'drizzle')
  ) x")
rows=$(psql "$(owner_url ci_a)" -Atc 'select count(*) from drizzle.__drizzle_migrations')
[[ -z $left && $rows == 0 ]] || { echo "db:ci: after the down files: objects '$left', migration rows $rows" >&2; exit 1; }
echo "only an empty drizzle schema remains"
MIGRATE_DATABASE_URL=$(owner_url ci_a) bun "$DB_DIR/src/cli/migrate.ts"
dump ci_a >"$WORK/a2.dump"
diff -u "$WORK/a.dump" "$WORK/a2.dump" || { echo "db:ci: up after down differs from the first build" >&2; exit 1; }
echo "up after down matches"

step "5: integration tests"
MIGRATE_DATABASE_URL=$(owner_url "$TEST_TEMPLATE_DB") bun "$DB_DIR/src/cli/migrate.ts"
(cd "$DB_DIR" && bun test test/integration)
if [[ -d $ROOT/apps/web/test/integration ]]; then
  (cd "$ROOT/apps/web" && bun test test/integration)
fi

if [[ -f $ROOT/crates/convt-server/.sqlx/.gitkeep || -d $ROOT/crates/convt-server/.sqlx ]]; then
  step "6: convt-server against database A"
  (
    cd "$ROOT/crates/convt-server"
    DATABASE_URL=$(owner_url ci_a) cargo sqlx prepare --check -- --all-targets
  )
  (cd "$ROOT" && env -u DATABASE_URL SQLX_OFFLINE=true cargo build -p convt-server --all-targets)
  (cd "$ROOT" && CONVT_REQUIRE_DB=1 cargo test -p convt-server)
fi

printf '\n== db:ci passed\n'
