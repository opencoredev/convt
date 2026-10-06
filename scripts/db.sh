#!/usr/bin/env bash
# Local Postgres and Mailpit for one checkout of convt.
#
#   scripts/db.sh up        start (or reuse) this checkout's containers; idempotent
#   scripts/db.sh down      stop and remove this checkout's containers and volume
#   scripts/db.sh status    show the containers, ports and owner marker
#   scripts/db.sh url [web|server|billing|owner]   print a connection URL (default web)
#   scripts/db.sh psql [web|server|billing|owner]  open psql as that role
#   scripts/db.sh prune     remove containers and volumes of checkouts that no longer exist
#   scripts/db.sh guard URL exit 0 only if URL is this checkout's own database
#
# Containers are named for a hash of the checkout path and labeled with the path,
# so two checkouts never share a database. Ports are published on 127.0.0.1 with a
# random host port. `up` writes .convt-dev/services.env (mode 0600), which the dev
# server, the seed and the tests read. Nothing here touches a container or volume
# without the convt.checkout label.

set -euo pipefail

ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)
HASH=$(printf '%s' "$ROOT" | sha256sum | cut -c1-8)
PG=convt-pg-$HASH
MAIL=convt-mail-$HASH
VOLUME=$PG-data
LABEL=convt.checkout
DEV_DIR=$ROOT/.convt-dev
ENV_FILE=$DEV_DIR/services.env
DB=convt

# Pinned by digest. Match Postgres's major version to Railway's when the
# production database exists.
PG_IMAGE=postgres:17-alpine@sha256:b0f9560a2de083e2cc7382e75f808c7381a32852a7ec49117deedb300e552b24
MAIL_IMAGE=axllent/mailpit@sha256:b68349e3a014b90c5610bfb26b2ae36f3892d7b8cf25ee140c6c71c98d2fcf48

die() {
  echo "db.sh: $*" >&2
  exit 1
}

need() { command -v "$1" >/dev/null || die "$1 is not installed"; }

exists() { docker inspect "$1" >/dev/null 2>&1; }

label_of() { docker inspect --format "{{ index .Config.Labels \"$LABEL\" }}" "$1" 2>/dev/null; }

volume_label_of() { docker volume inspect --format "{{ index .Labels \"$LABEL\" }}" "$1" 2>/dev/null; }

host_port() { docker port "$1" "$2" 2>/dev/null | sed -n 's/^127\.0\.0\.1:\([0-9]*\)$/\1/p' | head -1; }

# A container with our name but another checkout's label is someone else's; stop.
check_ours() {
  local name=$1 label
  if exists "$name"; then
    label=$(label_of "$name")
    [[ $label == "$ROOT" ]] || die "$name exists but belongs to '${label:-<unlabeled>}', not $ROOT"
  fi
}

load_env() {
  [[ -f $ENV_FILE ]] || die "no $ENV_FILE; run scripts/db.sh up"
  # shellcheck disable=SC1090
  set -a && . "$ENV_FILE" && set +a
}

secret() { openssl rand -hex 32; }

free_port() { python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1])'; }

wait_ready() {
  for _ in $(seq 120); do
    docker exec "$PG" pg_isready -q -U postgres -d "$DB" 2>/dev/null && return 0
    sleep 0.5
  done
  die "Postgres in $PG did not become ready"
}

su_psql() { docker exec -i -e PGOPTIONS=-cclient_min_messages=warning "$PG" psql -v ON_ERROR_STOP=1 -q -U postgres -d "$DB" "$@"; }

cmd_up() {
  need docker
  need openssl
  check_ours "$PG"
  check_ours "$MAIL"
  if exists "$VOLUME"; then
    [[ $(volume_label_of "$VOLUME") == "$ROOT" ]] || die "volume $VOLUME belongs to another checkout"
  fi
  mkdir -p "$DEV_DIR"
  chmod 700 "$DEV_DIR"

  # Secrets survive restarts: reuse the ones already in services.env.
  local owner_pw web_pw server_pw billing_pw auth_secret mock_port billing_mock_port webhook_secret fresh=0
  if [[ -f $ENV_FILE ]]; then
    owner_pw=$(sed -n 's/^OWNER_PASSWORD=//p' "$ENV_FILE")
    web_pw=$(sed -n 's/^WEB_PASSWORD=//p' "$ENV_FILE")
    server_pw=$(sed -n 's/^SERVER_PASSWORD=//p' "$ENV_FILE")
    billing_pw=$(sed -n 's/^BILLING_PASSWORD=//p' "$ENV_FILE")
    billing_mock_port=$(sed -n 's/^BILLING_MOCK_PORT=//p' "$ENV_FILE")
    webhook_secret=$(sed -n 's/^POLAR_WEBHOOK_SECRET=//p' "$ENV_FILE")
    auth_secret=$(sed -n 's/^BETTER_AUTH_SECRET=//p' "$ENV_FILE")
    mock_port=$(sed -n 's/^OAUTH_MOCK_PORT=//p' "$ENV_FILE")
  fi
  owner_pw=${owner_pw:-$(secret)}
  web_pw=${web_pw:-$(secret)}
  server_pw=${server_pw:-$(secret)}
  billing_pw=${billing_pw:-$(secret)}
  billing_mock_port=${billing_mock_port:-$(free_port)}
  # A Standard Webhooks secret for the local billing mock, never a real Polar one.
  webhook_secret=${webhook_secret:-whsec_$(openssl rand -base64 32)}
  auth_secret=${auth_secret:-$(secret)}
  mock_port=${mock_port:-$(free_port)}

  if ! exists "$PG"; then
    docker volume create --label "$LABEL=$ROOT" "$VOLUME" >/dev/null
    # The superuser password is never used: admin work goes through docker exec,
    # which the image trusts over the local socket.
    docker run -d --name "$PG" --label "$LABEL=$ROOT" \
      -e POSTGRES_PASSWORD="$(secret)" -e POSTGRES_DB="$DB" \
      -v "$VOLUME:/var/lib/postgresql/data" \
      -p 127.0.0.1::5432 "$PG_IMAGE" >/dev/null
    fresh=1
  elif [[ $(docker inspect --format '{{.State.Running}}' "$PG") != true ]]; then
    docker start "$PG" >/dev/null
  fi
  if ! exists "$MAIL"; then
    docker run -d --name "$MAIL" --label "$LABEL=$ROOT" \
      -p 127.0.0.1::8025 "$MAIL_IMAGE" >/dev/null
  elif [[ $(docker inspect --format '{{.State.Running}}' "$MAIL") != true ]]; then
    docker start "$MAIL" >/dev/null
  fi
  wait_ready

  local id pg_port mail_port
  id=$(docker inspect --format '{{.Id}}' "$PG")
  pg_port=$(host_port "$PG" 5432/tcp)
  mail_port=$(host_port "$MAIL" 8025/tcp)
  [[ -n $pg_port && -n $mail_port ]] || die "could not read the published ports"

  # Roles and the owner marker. Both are idempotent, so an `up` on an existing
  # database only refreshes passwords from services.env.
  su_psql -v owner_password="$owner_pw" -v web_password="$web_pw" \
    -v server_password="$server_pw" -v billing_password="$billing_pw" <"$ROOT/packages/db/roles.sql"
  su_psql <<SQL
create schema if not exists convt_dev;
create table if not exists convt_dev.owner (
  checkout text not null,
  container_id text not null,
  created_at timestamptz not null default now()
);
insert into convt_dev.owner (checkout, container_id)
select '$ROOT', '$id'
where not exists (select 1 from convt_dev.owner);
grant usage on schema convt_dev to convt_owner, convt_web, convt_server, convt_billing;
grant select on convt_dev.owner to convt_owner, convt_web, convt_server, convt_billing;
SQL

  local tmp
  tmp=$(mktemp "$DEV_DIR/services.env.XXXXXX")
  chmod 600 "$tmp"
  cat >"$tmp" <<ENV
# Written by scripts/db.sh up for $ROOT. Local development only; never commit.
CONTAINER_ID=$id
PG_CONTAINER=$PG
MAIL_CONTAINER=$MAIL
PG_PORT=$pg_port
OWNER_PASSWORD=$owner_pw
WEB_PASSWORD=$web_pw
SERVER_PASSWORD=$server_pw
BILLING_PASSWORD=$billing_pw
DATABASE_URL=postgresql://convt_web:$web_pw@127.0.0.1:$pg_port/$DB
SERVER_DATABASE_URL=postgresql://convt_server:$server_pw@127.0.0.1:$pg_port/$DB
BILLING_DATABASE_URL=postgresql://convt_billing:$billing_pw@127.0.0.1:$pg_port/$DB
OWNER_DATABASE_URL=postgresql://convt_owner:$owner_pw@127.0.0.1:$pg_port/$DB
MAILPIT_URL=http://127.0.0.1:$mail_port
OAUTH_MOCK_PORT=$mock_port
OAUTH_MOCK_URL=http://127.0.0.1:$mock_port
BETTER_AUTH_SECRET=$auth_secret
BILLING_MOCK_PORT=$billing_mock_port
BILLING_MOCK_URL=http://127.0.0.1:$billing_mock_port
POLAR_WEBHOOK_SECRET=$webhook_secret
ENV
  mv "$tmp" "$ENV_FILE"
  if ((fresh)); then echo "db.sh: created $PG and $MAIL"; else echo "db.sh: $PG and $MAIL are up"; fi
  echo "db.sh: postgres 127.0.0.1:$pg_port, mailpit http://127.0.0.1:$mail_port"
}

# Refuse unless URL points at this checkout's container and database. Used before
# anything destructive: seed, rollback, reset and down.
cmd_guard() {
  local url=${1:-} host port id expected_id label count
  [[ -n $url ]] || die "guard needs a URL"
  load_env
  # One plain shape only: postgresql://user:password@127.0.0.1:<port>/convt. Query
  # parameters (dbname=, host=, service=) are read differently by libpq and
  # node-postgres, so the marker check could see another database than the tool
  # that runs next.
  read -r host port < <(python3 - "$url" "$DB" <<'PY'
import re, sys, urllib.parse
url, db = sys.argv[1], sys.argv[2]
u = urllib.parse.urlsplit(url)
ok = (
    u.scheme in ("postgresql", "postgres")
    and not u.query and not u.fragment
    and u.path == "/" + db
    and re.fullmatch(r"[A-Za-z0-9_]+:[A-Za-z0-9]+@127\.0\.0\.1:[0-9]+", u.netloc or "") is not None
)
print((u.hostname or "") if ok else "invalid", u.port or 0 if ok else 0)
PY
)
  [[ $host != invalid ]] || die "guard: only postgresql://user:password@127.0.0.1:<port>/$DB is accepted"
  [[ $host == 127.0.0.1 ]] || die "guard: host '$host' is not 127.0.0.1"
  exists "$PG" || die "guard: no container $PG"
  [[ $port == "$(host_port "$PG" 5432/tcp)" ]] || die "guard: port $port is not $PG's published port"
  id=$(docker inspect --format '{{.Id}}' "$PG")
  expected_id=$CONTAINER_ID
  [[ $id == "$expected_id" ]] || die "guard: $PG is ${id:0:12}, services.env names ${expected_id:0:12}"
  label=$(label_of "$PG")
  [[ $label == "$ROOT" ]] || die "guard: $PG is labeled '$label', not $ROOT"
  # The marker is read through the URL itself, so it proves what the URL reaches.
  count=$(PGCONNECT_TIMEOUT=5 psql "$url" -Atc \
    "select count(*) from convt_dev.owner where checkout = '$ROOT' and container_id = '$id'" 2>/dev/null) ||
    die "guard: the database at that URL has no convt_dev.owner marker"
  [[ $count == 1 ]] || die "guard: convt_dev.owner does not name this checkout and container"
}

cmd_down() {
  if exists "$PG"; then
    load_env
    cmd_guard "$OWNER_DATABASE_URL"
  fi
  local name
  for name in "$PG" "$MAIL"; do
    if exists "$name"; then
      [[ $(label_of "$name") == "$ROOT" ]] || die "$name is not labeled for $ROOT; leaving it"
      docker rm -f "$name" >/dev/null && echo "db.sh: removed $name"
    fi
  done
  if exists "$VOLUME"; then
    [[ $(volume_label_of "$VOLUME") == "$ROOT" ]] || die "$VOLUME is not labeled for $ROOT; leaving it"
    docker volume rm "$VOLUME" >/dev/null && echo "db.sh: removed $VOLUME"
  fi
  rm -f "$ENV_FILE"
}

cmd_status() {
  echo "checkout: $ROOT"
  local name
  for name in "$PG" "$MAIL"; do
    if exists "$name"; then
      echo "$name: $(docker inspect --format '{{.State.Status}}' "$name") $(docker port "$name" | tr '\n' ' ')"
    else
      echo "$name: absent"
    fi
  done
  if exists "$PG" && [[ -f $ENV_FILE ]]; then
    load_env
    echo "owner marker: $(psql "$OWNER_DATABASE_URL" -Atc "select checkout || ' ' || left(container_id, 12) from convt_dev.owner" 2>/dev/null || echo missing)"
  fi
}

cmd_url() {
  load_env
  case ${1:-web} in
    web) echo "$DATABASE_URL" ;;
    server) echo "$SERVER_DATABASE_URL" ;;
    billing) echo "$BILLING_DATABASE_URL" ;;
    owner) echo "$OWNER_DATABASE_URL" ;;
    *) die "unknown role $1 (web, server, billing or owner)" ;;
  esac
}

cmd_prune() {
  local kind name label
  local -a doomed=()
  while read -r kind name; do
    [[ -n $name ]] || continue
    if [[ $kind == container ]]; then label=$(label_of "$name"); else label=$(volume_label_of "$name"); fi
    [[ -n $label && ! -d $label ]] && doomed+=("$kind $name $label")
  done < <(
    docker ps -a --filter "label=$LABEL" --format 'container {{.Names}}'
    docker volume ls --filter "label=$LABEL" --format 'volume {{.Name}}'
  )
  if ((${#doomed[@]} == 0)); then
    echo "db.sh: nothing to prune"
    return
  fi
  echo "db.sh: these belong to checkouts that no longer exist:"
  printf '  %s\n' "${doomed[@]}"
  local entry
  for entry in "${doomed[@]}"; do
    read -r kind name _ <<<"$entry"
    # Containers come first in the list, so a volume is free by the time it is removed.
    if [[ $kind == container ]]; then docker rm -f "$name" >/dev/null; else docker volume rm "$name" >/dev/null; fi
    echo "db.sh: removed $kind $name"
  done
}

case ${1:-} in
  up) cmd_up ;;
  down) cmd_down ;;
  status) cmd_status ;;
  url) cmd_url "${2:-web}" ;;
  psql) shift && psql "$(cmd_url "${1:-web}")" "${@:2}" ;;
  prune) cmd_prune ;;
  guard) cmd_guard "${2:-}" ;;
  *) sed -n '2,13p' "$0" | sed 's/^# \{0,1\}//' && exit 2 ;;
esac
