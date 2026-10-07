#!/usr/bin/env bash
# Every seeded fixture on all four dashboard pages and settings, at 1280 and 390
# px, in light and dark, with a text check per state. Needs `bun run db:seed`.
source "$(dirname "$0")/lib.sh"

pages=(dashboard dashboard/licenses dashboard/billing dashboard/api account)
name_of() { local p=${1//\//-}; echo "${p/dashboard-/}"; }

# fixture | page | text the page must show
expectations=(
  "new|dashboard|No plan" "new|dashboard|No key yet" "new|dashboard|Not set up"
  "new|dashboard/licenses|No licenses on this account yet." "new|dashboard/billing|No plan"
  "new|dashboard/billing|No invoices yet." "new|dashboard/api|No keys yet."
  "trial|dashboard|Pro trial" "trial|dashboard|Trial ends" "trial|dashboard/licenses|Pro, free trial"
  "trial|dashboard/billing|TRIAL" "trial|dashboard/billing|Free until"
  "desktop|dashboard|Desktop" "desktop|dashboard|Active on 1 Mac" "desktop|dashboard|Dana's MacBook Air"
  "desktop|dashboard/licenses|Desktop License, bought" "desktop|dashboard/billing|No plan"
  "desktop|dashboard/billing|Desktop License, 12 months of updates"
  "pro|dashboard|Renews" "pro|dashboard|Updates included" "pro|dashboard|API this month"
  "pro|dashboard/licenses|Pro, yearly" "pro|dashboard/licenses|Desktop License, bought"
  "pro|dashboard/billing|ACTIVE" "pro|dashboard/billing|API, pay per conversion" "pro|dashboard/billing|\$96.00"
  "pro|dashboard/api|9,412" "pro|dashboard/api|Production" "pro|dashboard/api|cvt_live_"
  "pro|account|ID 100200300" "pro|account|Leo's MacBook Pro"
  "lapsed|dashboard|Ended" "lapsed|dashboard|Builds up to" "lapsed|dashboard/billing|CANCELED"
  "lapsed|dashboard/billing|Pro, monthly (payment failed)"
  "api|dashboard|API this month" "api|dashboard|No plan" "api|dashboard/api|Backend" "api|dashboard/billing|Spend cap"
  "api|dashboard/billing|API, pay per conversion" "api|dashboard/billing|Billed per conversion"
)
expected_for() { for e in "${expectations[@]}"; do IFS='|' read -r f p t <<<"$e"; [[ $f == "$1" && $p == "$2" ]] && echo "$t"; done; }

for fixture in new trial desktop pro lapsed api; do
  width desktop
  theme light
  sign_in_code "$fixture@convt.test"
  for page in "${pages[@]}"; do
    open_page "/$page"
    text=$(page_text)
    while IFS= read -r want; do
      [[ -z $want ]] && continue
      check "$fixture /$page shows \"$want\"" grep -qF "$want" <<<"$text"
    done < <(expected_for "$fixture" "$page")
    check "$fixture /$page has no sample-data badge or preview note" bash -c '! grep -qE "Sample data|Preview only" <<<"$1"' _ "$text"
    for w in desktop phone; do
      width "$w"
      for th in light dark; do
        theme "$th"
        ab wait 300 >/dev/null
        shot "$fixture-$(name_of "$page")-$w-$th"
      done
    done
    width desktop
    theme light
  done
  # Licenses never put a token in the HTML the server renders.
  cookie=$(ab cookies get --json | python3 -c 'import json,sys; print("; ".join(c["name"]+"="+c["value"] for c in json.load(sys.stdin)["data"]["cookies"]))')
  html=$(curl -fsS -H "cookie: $cookie" "$E2E_URL/dashboard/licenses")
  check "$fixture: no license token in the server-rendered licenses page" bash -c '! grep -qE "eyJ[A-Za-z0-9_-]{20,}\.[A-Za-z0-9_-]{40,}" <<<"$1"' _ "$html"
  headers=$(curl -fsS -o /dev/null -D - -H "cookie: $cookie" "$E2E_URL/dashboard")
done

finish
