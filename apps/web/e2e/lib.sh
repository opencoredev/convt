#!/usr/bin/env bash
# Shared helpers for the e2e scripts. They drive a headless browser with
# agent-browser against a running `bun run dev:web` (or `wrangler dev` on the
# build), read sign-in codes from Mailpit's API, and save screenshots.
#
#   E2E_URL     site origin (default: the port in apps/web/.dev.vars)
#   E2E_SHOTS   screenshot directory (default: a mktemp directory)
#   E2E_SESSION agent-browser session name (default: convt-e2e)
#
# Sends are limited per address and per IP, and every local request comes from
# the same IP, so `reset_limits` clears the buckets in this checkout's own
# database (through the ownership guard) before each sign-in.

set -euo pipefail
ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd -P)
export PATH=$HOME/.bun/bin:$PATH
set -a
# shellcheck disable=SC1091
. "$ROOT/.convt-dev/services.env"
set +a

if [[ -z ${E2E_URL:-} ]]; then
  E2E_URL=$(sed -n 's/^BETTER_AUTH_URL=//p' "$ROOT/apps/web/.dev.vars")
fi
E2E_SHOTS=${E2E_SHOTS:-$(mktemp -d /tmp/convt-e2e.XXXXXX)}
E2E_SESSION=${E2E_SESSION:-convt-e2e}
mkdir -p "$E2E_SHOTS"
FAILURES=0

ab() { agent-browser --session "$E2E_SESSION" --profile "$HOME/.agent-browser/profiles/$E2E_SESSION" "$@"; }

pass() { printf 'PASS %s\n' "$*"; }
fail() {
  printf 'FAIL %s\n' "$*"
  FAILURES=$((FAILURES + 1))
}
check() { # check "description" command...
  local what=$1
  shift
  if "$@"; then pass "$what"; else fail "$what"; fi
}
finish() {
  echo "screenshots: $E2E_SHOTS"
  if ((FAILURES)); then
    echo "$FAILURES check(s) failed"
    exit 1
  fi
  echo "all checks passed"
}

owner_sql() {
  bash "$ROOT/scripts/db.sh" guard "$OWNER_DATABASE_URL"
  psql "$OWNER_DATABASE_URL" -qAtc "$1"
}

reset_limits() { owner_sql "delete from otp_send_limits; delete from rate_limits" >/dev/null; }

clear_mail() { curl -fsS -X DELETE "$MAILPIT_URL/api/v1/messages" >/dev/null; }

# The newest code mailed to an address (waits up to 10 s).
mail_code() {
  local to=$1 code=""
  for _ in $(seq 40); do
    code=$(curl -fsS "$MAILPIT_URL/api/v1/search?query=to:$to" |
      python3 -c 'import json,re,sys; m=json.load(sys.stdin)["messages"]; print(re.search(r"\d{6}", m[0]["Subject"]).group() if m else "")')
    [[ -n $code ]] && break
    sleep 0.25
  done
  [[ -n $code ]] || { echo "no code mailed to $to" >&2; return 1; }
  echo "$code"
}

# The full text of the newest message to an address.
mail_text() {
  local id
  id=$(curl -fsS "$MAILPIT_URL/api/v1/search?query=to:$1" | python3 -c 'import json,sys; print(json.load(sys.stdin)["messages"][0]["ID"])')
  curl -fsS "$MAILPIT_URL/api/v1/message/$id" | python3 -c 'import json,sys; print(json.load(sys.stdin)["Text"])'
}

url_now() { ab get url; }

hydrated() {
  ab wait --fn 'Object.keys(document.querySelector("main") ?? {}).some((k) => k.startsWith("__reactFiber"))' >/dev/null
}

wait_url() { # wait_url substring [seconds]; then waits for the page to hydrate
  for _ in $(seq $((${2:-15} * 4))); do
    if [[ $(url_now) == *"$1"* ]]; then
      [[ $(url_now) == "$E2E_URL"* ]] && hydrated
      return 0
    fi
    sleep 0.25
  done
  echo "still at $(url_now), wanted $1" >&2
  return 1
}

open_page() {
  ab open "$E2E_URL$1" >/dev/null
  # Hydrated once React has attached its fiber to the page's main element.
  hydrated
}

type_code() {
  ab click 'input[aria-label="Digit 1 of 6"]' >/dev/null
  ab keyboard type "$1" >/dev/null
}

sign_out_all_cookies() { ab cookies clear >/dev/null; }

# Signs in through the sign-in form and the code from Mailpit; ends on the dashboard.
sign_in_code() {
  local email=$1
  reset_limits
  sign_out_all_cookies
  open_page "/sign-in"
  ab fill 'input[type=email]' "$email" >/dev/null
  ab click 'button[type=submit]' >/dev/null
  wait_url /sign-in/check-email
  type_code "$(mail_code "$email")"
  wait_url /dashboard
}

shot() { ab screenshot --full "$E2E_SHOTS/$1.png" >/dev/null; }

theme() { ab set media "$1" >/dev/null; }

width() { # desktop | phone
  if [[ $1 == phone ]]; then ab set viewport 390 844 >/dev/null; else ab set viewport 1280 900 >/dev/null; fi
}

text_of() { ab get text "$1" 2>/dev/null; }

page_text() { ab eval 'document.body.innerText'; }
