#!/usr/bin/env bash
# Run against local dev:web with SALES=desktop; direct calls must not create cloud checkouts.
source "$(dirname "$0")/lib.sh"

sign_in_code new@convt.test
before=$(owner_sql 'select count(*) from checkouts')
status=$(curl -sS -o /dev/null -w '%{http_code}' "$E2E_URL/checkout/pro?interval=month")
[[ $status == 403 ]] && pass 'direct guest Pro checkout refused' || fail "Pro returned $status"
ab eval 'fetch("/checkout/pro?interval=year").then(async r => ({status:r.status, body:await r.text()}))' | tee "$E2E_SHOTS/pro-refusal.json"
response=$(ab eval 'import("/src/server/billing-fns.ts").then(m => m.enrollApi({data:{cap:"20"}}))')
[[ $response == *'API billing is coming soon.'* ]] && pass 'direct authenticated API enrollment refused' || fail "$response"
[[ $(owner_sql 'select count(*) from checkouts') == "$before" ]] && pass 'refused requests created no checkouts' || fail 'cloud checkout was created'
for page in /dashboard/api /dashboard/billing; do
  open_page "$page"
  for w in desktop phone; do
    width "$w"
    for t in light dark; do
      theme "$t"
      shot "sales-$(basename "$page")-$w-$t"
    done
  done
  [[ $(page_text) == *'coming soon'* ]] && pass "$page shows coming soon" || fail "$page missing coming soon"
done
status=$(curl -sS -o /dev/null -w '%{http_code}' "$E2E_URL/checkout/desktop")
[[ $status == 303 ]] && pass 'Desktop still starts checkout' || fail "Desktop returned $status"
[[ $(owner_sql 'select count(*) from checkouts') == "$((before + 1))" ]] && pass 'only Desktop created a checkout' || fail 'wrong checkout count'
finish
