#!/usr/bin/env bash
# CNV-101 end to end against `bun run dev:web` and the billing mock: the pricing
# page, Desktop at $39, a $29 order sold before the change refunded afterwards, a
# Desktop + Cloud purchase and its billing page, a second bundle that stacks, a
# refund that moves it forward, a lost chargeback, deleting an account that holds
# a bundle, and the seeded Pro subscriber switching, canceling, resuming and
# renewing. Screenshots at 1280 and 390 px, light and dark.
# Needs `bun run db:seed` and `bun tools/billing-mock/src/preload.ts`. Run it once
# per fresh dev:web: the last step moves the mock's clock a year forward.
source "$(dirname "$0")/lib.sh"

ab() { timeout 90 agent-browser --session "$E2E_SESSION" --profile "$HOME/.agent-browser/profiles/$E2E_SESSION" "$@"; }
MOCK=$BILLING_MOCK_URL
admin() { curl -fsS -m 30 -X POST "$MOCK/admin/$1" -H 'content-type: application/json' -d "${2:-{\}}"; }
cron() { curl -fsS -m 120 "$E2E_URL/__billing/scheduled?cron=$1" >/dev/null; }
reconcile() { cron '*%2F15+*+*+*+*'; }
wait_text() { ab wait --text "$1" --timeout 30000 >/dev/null 2>&1; }
js() { ab eval "$1" | tr -d '"'; }
dom_click() {
  local q
  q=$(printf '%s' "$1" | python3 -c 'import json,sys; print(json.dumps(sys.stdin.read()))')
  js "(() => { const b = [...document.querySelectorAll('button, a')].find((x) => x.getAttribute('aria-label') === $q || x.textContent.trim() === $q); if (!b) return 'missing'; b.click(); return 'clicked'; })()"
}
shots() {
  local name=$1 w t
  for w in desktop phone; do
    width "$w"
    for t in light dark; do
      theme "$t"
      sleep 0.4
      shot "$name-$w-$t"
    done
  done
  width desktop
  theme light
}
until_sql() {
  for _ in $(seq $((${3:-30} * 2))); do
    [[ $(owner_sql "$1") == "$2" ]] && return 0
    sleep 0.5
  done
  echo "wanted '$2', got '$(owner_sql "$1")'" >&2
  return 1
}
until_eq() { # until_eq expected command...: polls the command's output
  local want=$1 got=""
  shift
  for _ in $(seq 60); do
    got=$("$@")
    [[ $got == "$want" ]] && return 0
    sleep 0.5
  done
  echo "wanted '$want', got '$got'" >&2
  return 1
}
bundle_state() { echo "$(key_state "$1")/$(years "$1")"; }
pay() {
  ab wait 'select#card' >/dev/null
  ab select 'select#card' "${1:-4242}" >/dev/null
  ab click 'button[value=pay]' >/dev/null
}
no_overflow() { test "$(js 'document.documentElement.scrollWidth <= window.innerWidth')" = true; }
key_state() { # key_state email -> revoke reasons of the account's keys, oldest first
  owner_sql "select string_agg(coalesce(l.revoke_reason, 'live'), ',' order by l.created_at) from licenses l join users u on u.id = l.user_id where u.email = '$1'"
}
years() { # years email -> the status of each cloud year, by purchase
  owner_sql "select string_agg(s.status, ',' order by o.billed_at) from subscriptions s join orders o on o.id = s.order_id where o.email = '$1'"
}

trial_route() { # trial_route email -> where the app's trial URL sends that account
  local before
  sign_in_code "$1"
  before=$(owner_sql "select count(*) from checkouts c join users u on u.id = c.user_id where u.email = '$1' and c.product like 'pro_%'")
  ab open "$E2E_URL/checkout/pro?interval=month&from=app" >/dev/null
  sleep 2
  local url
  url=$(url_now)
  local after
  after=$(owner_sql "select count(*) from checkouts c join users u on u.id = c.user_id where u.email = '$1' and c.product like 'pro_%'")
  echo "${url#"$E2E_URL"} +$((after - before))"
}

width desktop
theme light
open_page /sign-in || true
admin webhooks '{"mode":"auto","dropNext":0,"duplicate":0,"forgeNext":0}' >/dev/null

# 1. Pricing: two one-time cards, no Pro checkout, no trial.
ab cookies clear >/dev/null
open_page /
ab eval 'document.getElementById("pricing").scrollIntoView()' >/dev/null
pricing=$(js 'document.getElementById("pricing").innerText')
check "pricing shows Desktop at \$39 and Desktop + Cloud at \$69" bash -c '[[ $1 == *"\$39"* && $1 == *"\$69"* && $1 == *"Desktop + Cloud"* ]]' _ "$pricing"
check "pricing has no trial, monthly price or Pro checkout" bash -c '[[ $1 != *trial* && $1 != *"per month"* ]]' _ "$pricing"
check "no link on the landing page opens Pro checkout" test "$(js '[...document.querySelectorAll("a")].filter((a) => a.href.includes("/checkout/pro")).length')" = 0
for w in desktop phone; do
  width "$w"
  for t in light dark; do
    theme "$t"
    ab eval 'document.getElementById("pricing").scrollIntoView()' >/dev/null
    sleep 0.4
    ab screenshot "$E2E_SHOTS/pricing-$w-$t.png" >/dev/null
  done
  check "pricing fits the $w width" no_overflow
done
width desktop
theme light

# 2. Desktop at $39, bought signed in.
d39="desktop39-$RANDOM@convt.test"
sign_in_code "$d39"
ab open "$E2E_URL/checkout/desktop" >/dev/null
wait_url "$MOCK/checkout/"
check "the provider's Desktop checkout charges \$39.00" grep -q '\$39.00' <<<"$(page_text)"
pay
wait_url /checkout/success 30
check "the \$39 buyer sees their key" wait_text "Here's your license key"
check "the order is \$39" until_sql "select amount_cents from orders where email = '$d39'" 3900
check "a Desktop order makes no cloud year" test -z "$(years "$d39")"

# 3. A $29 order sold before the change, refunded after it.
admin product '{"key":"desktop","price_id":"price_local_desktop","amount":2900}' >/dev/null
d29="desktop29-$RANDOM@convt.test"
sign_in_code "$d29"
ab open "$E2E_URL/checkout/desktop" >/dev/null
wait_url "$MOCK/checkout/"
check "with the old price the provider charges \$29.00" grep -q '\$29.00' <<<"$(page_text)"
pay
wait_url /checkout/success 30
wait_text "Here's your license key" || true
admin product '{"key":"desktop","price_id":"price_local_desktop_39","amount":3900}' >/dev/null
check "the \$29 order granted a key" until_sql "select count(*) from licenses l join orders o on o.id = l.order_id where o.email = '$d29' and o.amount_cents = 2900" 1
reconcile
order29=$(owner_sql "select provider_order_id from orders where email = '$d29'")
admin refund "{\"order_id\":\"$order29\"}" >/dev/null
reconcile
check "refunding the \$29 order after the change revokes its key" until_sql "select revoke_reason from licenses l join orders o on o.id = l.order_id where o.email = '$d29'" refunded
check "no refetch of the \$29 order was rejected" test "$(owner_sql "select count(*) from billing_alerts where kind = 'rejected_fact' and detail like '%Desktop is%'")" = 0

# 4. Desktop + Cloud: needs an account, then a key and a year.
ab cookies clear >/dev/null
guest_bundle=$(curl -sS -o /dev/null -w '%{http_code} %{redirect_url}' "$E2E_URL/checkout/bundle")
check "a signed-out Desktop + Cloud checkout signs in first" bash -c '[[ $1 == "303 "*"/sign-in?redirect=%2Fcheckout%2Fbundle" ]]' _ "$guest_bundle"
bundle="bundle-$RANDOM@convt.test"
sign_in_code "$bundle"
ab open "$E2E_URL/checkout/bundle" >/dev/null
wait_url "$MOCK/checkout/"
check "the provider's bundle checkout charges \$69.00, once" bash -c '[[ $1 == *"\$69.00"* && $1 != *" / "* ]]' _ "$(page_text)"
pay
wait_url /checkout/success 30
check "the bundle success page shows its key" wait_text "Here's your license key"
check "and names Desktop + Cloud" grep -q "CONVT DESKTOP + CLOUD" <<<"$(page_text)"
shots success-bundle
check "the bundle granted one Desktop key and no Pro key" test "$(owner_sql "select string_agg(plan, ',') from licenses l join users u on u.id = l.user_id where u.email = '$bundle'")" = desktop
check "and one active cloud year of 365 days" test "$(owner_sql "select s.status || ':' || extract(day from s.current_period_end - s.current_period_start) from subscriptions s join orders o on o.id = s.order_id where o.email = '$bundle'")" = "active:365"
open_page /dashboard/billing
check "billing shows the cloud year apart from the plan" wait_text "Buy another year"
shots billing-bundle
for w in desktop phone; do
  width "$w"
  check "billing fits the $w width" no_overflow
done
width desktop
open_page /dashboard
shots overview-bundle
check "the overview names Desktop + Cloud" grep -q "Desktop + Cloud" <<<"$(page_text)"
open_page /dashboard/api/convert
check "the browser converter lets the bundle owner in" bash -c '[[ $1 != *"needs Desktop + Cloud"* ]]' _ "$(page_text)"

# 4b. The app's trial URL starts only a trial: an account that can't start one gets
# no Pro checkout. A cloud-year owner goes to billing, a lapsed one to pricing.
check "a Desktop + Cloud owner's trial URL goes to billing, with no Pro checkout" test "$(trial_route "$bundle")" = "/dashboard/billing +0"
check "a lapsed Pro account's trial URL goes to pricing, with no Pro checkout" test "$(trial_route lapsed@convt.test)" = "/#pricing +0"
fresh_trial=$(trial_route "trial-$RANDOM@convt.test")
check "a new account's trial URL still opens the trial checkout" bash -c '[[ $1 == "$2/checkout/"*" +1" ]]' _ "$fresh_trial" "$MOCK"
sign_in_code "$bundle"

# 5. A second bundle stacks; refunding the first moves it to the refund.
ab open "$E2E_URL/checkout/bundle" >/dev/null
wait_url "$MOCK/checkout/"
pay
wait_url /checkout/success 30
check "a second bundle makes a second year" until_sql "select count(*) from subscriptions s join orders o on o.id = s.order_id where o.email = '$bundle'" 2
check "that starts when the first ends" test "$(owner_sql "select (select current_period_start from subscriptions s join orders o on o.id = s.order_id where o.email = '$bundle' order by o.billed_at desc limit 1) = (select current_period_end from subscriptions s join orders o on o.id = s.order_id where o.email = '$bundle' order by o.billed_at limit 1)")" = t
first=$(owner_sql "select provider_order_id from orders where email = '$bundle' order by billed_at limit 1")
admin refund "{\"order_id\":\"$first\"}" >/dev/null
reconcile
check "refunding the first bundle revokes its key and ends its year" until_eq "refunded,live/canceled,active" bundle_state "$bundle"
check "the second year now starts at the refund" test "$(owner_sql "select abs(extract(epoch from (select current_period_start from subscriptions s join orders o on o.id = s.order_id where o.email = '$bundle' order by o.billed_at desc limit 1) - now())) < 120")" = t

# 6. A lost chargeback on the second bundle.
second=$(owner_sql "select provider_order_id from orders where email = '$bundle' order by billed_at desc limit 1")
did=$(admin dispute "{\"action\":\"open\",\"order_id\":\"$second\"}" | python3 -c 'import json,sys; print(json.load(sys.stdin)["id"])')
admin dispute "{\"action\":\"lost\",\"dispute_id\":\"$did\"}" >/dev/null
reconcile
check "a lost chargeback revokes the key and ends the cloud year" until_eq "refunded,dispute_lost/canceled,canceled" bundle_state "$bundle"
open_page /dashboard/billing
shots billing-bundle-ended
open_page /dashboard/licenses
check "the licenses page shows both keys revoked" bash -c '[[ $1 == *REFUNDED* && $1 == *DISPUTED* ]]' _ "$(page_text)"

# 7. Deleting an account that holds a bundle.
gone="bundle-delete-$RANDOM@convt.test"
sign_in_code "$gone"
ab open "$E2E_URL/checkout/bundle" >/dev/null
wait_url "$MOCK/checkout/"
pay
wait_url /checkout/success 30
uid=$(owner_sql "select id from users where email = '$gone'")
until_sql "select count(*) from subscriptions where user_id = '$uid' and order_id is not null and status = 'active'" 1 >/dev/null
open_page /account
dom_click "Delete account" >/dev/null
wait_text "to confirm" || true
ab fill "input[spellcheck=false]" "$gone" >/dev/null
dom_click "Delete my account" >/dev/null
wait_text "Deleting your account" || true
cron '*+*+*+*+*'
check "the account holding a bundle is deleted" until_sql "select count(*) from users where id = '$uid'" 0 60
check "its order stays paid and its year is ended, keeping the email" test "$(owner_sql "select o.status || '/' || s.status from orders o join subscriptions s on s.order_id = o.id where o.email = '$gone'")" = "paid/canceled"

# 8. The seeded Pro subscriber keeps every control. Last: renewing moves the mock clock.
sign_in_code pro@convt.test
open_page /dashboard/billing
check "Pro yearly still shows its plan" wait_text "Pro, yearly"
check "and no link to buy Pro" test "$(js '[...document.querySelectorAll("a")].filter((a) => a.href.includes("/checkout/pro")).length')" = 0
shots billing-pro
dom_click "Switch to monthly" >/dev/null
wait_text "Switch to monthly now?" || true
dom_click "Switch to monthly" >/dev/null
check "Pro switches yearly to monthly" until_sql "select interval from subscriptions where email = 'pro@convt.test' and kind = 'pro' and order_id is null" month
open_page /dashboard/billing
dom_click "Switch to yearly" >/dev/null
wait_text "Switch to yearly now?" || true
dom_click "Switch to yearly" >/dev/null
check "and back to yearly" until_sql "select interval from subscriptions where email = 'pro@convt.test' and kind = 'pro' and order_id is null" year
open_page /dashboard/billing
dom_click "Cancel plan" >/dev/null
wait_text "Cancel Pro?" || true
dom_click "Cancel Pro" >/dev/null
check "Pro cancels at the period end" wait_text "Resume Pro"
dom_click "Resume Pro" >/dev/null
check "and resumes" until_sql "select cancel_at_period_end from subscriptions where email = 'pro@convt.test' and kind = 'pro' and order_id is null" f
puid=$(owner_sql "select id from users where email = 'pro@convt.test'")
before=$(owner_sql "select max(updates_until) from licenses where user_id = '$puid' and plan = 'pro' and revoked_at is null")
admin renew "{\"external_customer_id\":\"$puid\"}" >/dev/null
check "a renewal issues a later Pro key" until_sql "select max(updates_until) > '$before' from licenses where user_id = '$puid' and plan = 'pro' and revoked_at is null" t

finish
