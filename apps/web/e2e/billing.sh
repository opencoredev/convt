#!/usr/bin/env bash
# Billing end to end against `bun run dev:web` and the billing mock: a guest and a
# signed-in Desktop purchase with the key shown and Open in convt, the purchase
# email and its claim, the Pro trial and first payment, a declined and a
# successful switch to yearly, cancel and resume, the portal, a failed renewal, a
# refund and a dispute shown as revoked, API enrollment with a spend cap, the four
# emails, and account deletion. Screenshots at 1280 and 390 px, light and dark.
# Needs `bun run db:seed` and `bun tools/billing-mock/src/preload.ts`.
source "$(dirname "$0")/lib.sh"

# Every browser step is bounded, so a stuck page fails the check instead of hanging.
ab() { timeout 90 agent-browser --session "$E2E_SESSION" --profile "$HOME/.agent-browser/profiles/$E2E_SESSION" "$@"; }
SECOND=${E2E_SESSION}-second
ab2() { timeout 90 agent-browser --session "$SECOND" --profile "$HOME/.agent-browser/profiles/$SECOND" "$@"; }

MOCK=$BILLING_MOCK_URL
admin() { curl -fsS -m 30 -X POST "$MOCK/admin/$1" -H 'content-type: application/json' -d "${2:-{\}}"; }
cron() { curl -fsS -m 120 "$E2E_URL/__billing/scheduled?cron=$1" >/dev/null; }
drain() { cron '*+*+*+*+*'; }
wait_text() { ab wait --text "$1" --timeout 30000 >/dev/null 2>&1; }
js() { ab eval "$1" | tr -d '"'; }
dom_click() { # clicks a button by its text or aria-label through the DOM
  local q
  q=$(printf '%s' "$1" | python3 -c 'import json,sys; print(json.dumps(sys.stdin.read()))')
  js "(() => { const b = [...document.querySelectorAll('button, a')].find((x) => x.getAttribute('aria-label') === $q || x.textContent.trim() === $q); if (!b) return 'missing'; b.click(); return 'clicked'; })()"
}
stub_activate() {
  ab eval 'window.__opened = []; const click = HTMLAnchorElement.prototype.click; HTMLAnchorElement.prototype.click = function () { if (this.protocol === "convt:") { window.__opened.push(this.href); return; } return click.call(this); }; "stubbed"' >/dev/null
}
# The four views of the current page: 1280 and 390 px, light and dark.
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
until_sql() { # until_sql "query" expected [seconds]
  for _ in $(seq $((${3:-30} * 2))); do
    [[ $(owner_sql "$1") == "$2" ]] && return 0
    sleep 0.5
  done
  echo "wanted '$2', got '$(owner_sql "$1")'" >&2
  return 1
}
mail_to() { # the newest message to an address whose subject matches, as JSON {ID, Subject}
  curl -fsS -m 10 "$MAILPIT_URL/api/v1/search?query=to:$1" | python3 -c "
import json,sys,re
m=[x for x in json.load(sys.stdin)['messages'] if re.search(sys.argv[1], x['Subject'])]
print(m[0]['ID'] if m else '')" "$2"
}
wait_mail() { # wait_mail address subject-regex -> message id
  local id=""
  for _ in $(seq 40); do
    drain
    id=$(mail_to "$1" "$2")
    [[ -n $id ]] && break
    sleep 0.5
  done
  echo "$id"
}
email_shots() { # email_shots name message-id
  ab open "$MAILPIT_URL/view/$2.html" >/dev/null
  for t in light dark; do
    theme "$t"
    sleep 0.3
    shot "email-$1-desktop-$t"
  done
  theme light
}
pay() { # pay [card]: on the mock's hosted checkout
  ab wait 'select#card' >/dev/null
  ab select 'select#card' "${1:-4242}" >/dev/null
  ab click 'button[value=pay]' >/dev/null
}
mock_state() { curl -fsS -m 10 "$MOCK/admin/state"; }

width desktop
theme light
# Warm up: the first load after a fresh dev server re-optimizes dependencies.
open_page /checkout/success || true
open_page /sign-in || true
admin webhooks '{"mode":"auto","dropNext":0,"duplicate":0,"forgeNext":0}' >/dev/null

# 1. A guest buys Desktop.
guest="guest-$RANDOM@convt.test"
ab cookies clear >/dev/null
ab open "$E2E_URL/checkout/desktop" >/dev/null
check "Desktop checkout starts signed out and lands on the provider's page" wait_url "$MOCK/checkout/"
shots checkout-provider-page
ab fill '#email' "$guest" >/dev/null
pay
check "paying returns to the success page" wait_url /checkout/success 30
wait_text "Here's your license key" || true
token=$(owner_sql "select token from licenses where email = '$guest'")
shown=$(js 'document.querySelector("[data-testid=license-key]")?.innerText ?? ""')
check "the key shown is the issued license" test -n "$token" -a "$shown" = "$token"
check "the email is masked" grep -q "We also emailed it to g\*\*\*@convt.test" <<<"$(page_text)"
success_url=$(url_now)
shots success-ready
stub_activate
dom_click "Open in convt" >/dev/null
ab wait --fn 'window.__opened.length > 0' >/dev/null
check "Open in convt opens convt://activate with the key" test "$(js 'window.__opened[0]')" = "convt://activate?key=$token"
cookies=$(ab cookies get --json | python3 -c 'import json,sys; print("; ".join(c["name"]+"="+c["value"] for c in json.load(sys.stdin)["data"]["cookies"]))')
html=$(curl -fsS -m 30 -H "cookie: $cookies" "$success_url")
check "no token in the server-rendered success page" bash -c '! grep -qF "$1" <<<"$2"' _ "${token:0:40}" "$html"
ab open "$success_url" >/dev/null
wait_text "Here's your license key" || true
check "a reload within 10 minutes shows the key again" grep -q "Here's your license key" <<<"$(page_text)"
ab2 cookies clear >/dev/null
ab2 open "$success_url" >/dev/null
ab2 wait --text "Nothing to show here" --timeout 30000 >/dev/null 2>&1
check "the success URL in a second session shows nothing" grep -q "Nothing to show here" <<<"$(ab2 eval 'document.body.innerText')"
ab2 close >/dev/null 2>&1 || true
mid=$(wait_mail "$guest" "license key")
check "the purchase email arrives with the key" grep -qF "$token" <<<"$(curl -fsS -m 10 "$MAILPIT_URL/api/v1/message/$mid" | python3 -c 'import json,sys; print(json.load(sys.stdin)["Text"])')"
email_shots license-issued "$mid"
# Signing up with that email claims the purchase.
sign_in_code "$guest"
open_page /dashboard/licenses
check "signing up with the email claims the key" grep -q "Desktop License, bought" <<<"$(page_text)"

# 2. A signed-in user buys Desktop: the owning session sees the key without the nonce.
buyer="buyer-$RANDOM@convt.test"
sign_in_code "$buyer"
ab open "$E2E_URL/checkout/desktop" >/dev/null
wait_url "$MOCK/checkout/"
check "the provider's checkout has the account's email" test "$(js 'document.querySelector("#email").value')" = "$buyer"
pay
wait_url /checkout/success 30
check "the buyer sees their key" wait_text "Here's your license key"
check "and it is theirs on the dashboard" until_sql "select count(*) from licenses l join users u on u.id = l.user_id where u.email = '$buyer'" 1

# 3. Pro with a trial, then the first payment issues the key.
pro="pro-$RANDOM@convt.test"
sign_in_code "$pro"
ab open "$E2E_URL/checkout/pro?interval=month" >/dev/null
wait_url "$MOCK/checkout/"
check "the Pro checkout offers the 7-day trial" grep -q "7-day free trial" <<<"$(page_text)"
pay
wait_url /checkout/success 30
check "the success page says the trial started" wait_text "Your trial has started"
shots success-trial
check "no key during the trial" test "$(owner_sql "select count(*) from licenses l join users u on u.id = l.user_id where u.email = '$pro'")" = 0
open_page /dashboard/billing
wait_text "TRIAL" || true
shots billing-trial
uid=$(owner_sql "select id from users where email = '$pro'")
# The trial-ending email: move the trial's end inside 48 hours and run the reconciler.
owner_sql "update subscriptions set trial_ends_at = now() + interval '30 hours' where user_id = '$uid'" >/dev/null
cron '*%2F15+*+*+*+*'
tid=$(wait_mail "$pro" "trial ends")
check "the trial-ending email arrives once" test -n "$tid"
email_shots trial-ending "$tid"
admin trial-end "{\"external_customer_id\":\"$uid\"}" >/dev/null
check "the first payment issues a Pro key" until_sql "select count(*) from licenses where user_id = '$uid' and plan = 'pro'" 1
first_until=$(owner_sql "select updates_until from licenses where user_id = '$uid' and plan = 'pro'")
check "and emails it" test -n "$(wait_mail "$pro" "Pro license key")"
open_page /dashboard/licenses
shots licenses-pro

# 4. Monthly to yearly: a declined card changes nothing, then the switch.
admin card "{\"external_customer_id\":\"$uid\",\"decline\":true}" >/dev/null
open_page /dashboard/billing
dom_click "Switch to yearly" >/dev/null
wait_text "Switch to yearly now?" || true
shots billing-switch-confirm
dom_click "Switch to yearly" >/dev/null
check "a declined switch says so" wait_text "Your card was declined. Nothing changed."
shots billing-switch-declined
check "and nothing changed" test "$(owner_sql "select interval from subscriptions where user_id = '$uid' and kind = 'pro'")" = month
admin card "{\"external_customer_id\":\"$uid\",\"decline\":false}" >/dev/null
dom_click "Switch to yearly" >/dev/null
check "the switch goes through" wait_text "Switched to yearly."
check "the subscription is yearly" until_sql "select interval from subscriptions where user_id = '$uid' and kind = 'pro'" year
check "a key with the later end is issued" until_sql "select count(*) from licenses where user_id = '$uid' and plan = 'pro' and updates_until > '$first_until'" 1
open_page /dashboard/billing
shots billing-yearly

# 5. Cancel and resume.
dom_click "Cancel plan" >/dev/null
wait_text "Cancel Pro?" || true
shots billing-cancel-confirm
dom_click "Cancel Pro" >/dev/null
check "cancelling shows when Pro ends, with Resume" wait_text "Resume Pro"
shots billing-cancels
dom_click "Resume Pro" >/dev/null
check "resuming renews again" wait_text "Pro will renew."
check "the flag is stored" until_sql "select cancel_at_period_end from subscriptions where user_id = '$uid' and kind = 'pro'" f

# 6. The portal.
open_page /dashboard/billing
dom_click "Manage billing" >/dev/null
check "Manage billing opens the provider's portal" wait_url "$MOCK/portal/"
shots portal

# 7. A failed renewal is past due, with one email.
admin card "{\"external_customer_id\":\"$uid\",\"decline\":true}" >/dev/null
admin renew "{\"external_customer_id\":\"$uid\"}" >/dev/null
check "a failed renewal is past due" until_sql "select status from subscriptions where user_id = '$uid' and kind = 'pro'" past_due
open_page /dashboard/billing
wait_text "PAST DUE" || true
shots billing-past-due
rid=$(wait_mail "$pro" "couldn't charge")
check "the renewal-failed email arrives" test -n "$rid"
email_shots renewal-failed "$rid"
admin card "{\"external_customer_id\":\"$uid\",\"decline\":false}" >/dev/null
admin retry-payment "{\"external_customer_id\":\"$uid\"}" >/dev/null

# 8. A refund shows as revoked; the disputed fixture shows DISPUTED.
order=$(mock_state | python3 -c "import json,sys; print([o['id'] for o in json.load(sys.stdin)['orders'] if o['customer']['email']=='$guest'][0])")
admin refund "{\"order_id\":\"$order\"}" >/dev/null
check "a full refund revokes the guest's key" until_sql "select revoke_reason from licenses where email = '$guest'" refunded
sign_in_code "$guest"
open_page /dashboard/licenses
check "the card says REFUNDED with the date" grep -q "It won't be renewed or reissued." <<<"$(page_text)"
check "and hides Copy and Activate" test "$(js '[...document.querySelectorAll("button")].filter((b) => /Copy key|Activate/.test(b.textContent)).length')" = 0
shots licenses-refunded
sign_in_code disputed@convt.test
open_page /dashboard/licenses
check "a lost dispute shows DISPUTED" grep -q "DISPUTED" <<<"$(page_text)"
shots licenses-disputed

# 9. API enrollment with a spend cap; pending and the multiple-subscriptions notice.
sign_in_code apipending@convt.test
open_page /dashboard/api
check "an enrollment without a card is pending" wait_text "waiting for the payment provider"
shots api-pending
api="api-$RANDOM@convt.test"
sign_in_code "$api"
admin settings '{"allow_multiple_subscriptions":false}' >/dev/null
open_page /dashboard/api
check "with multiple subscriptions off, enrollment is refused with a notice" wait_text "can't start yet"
shots api-blocked
admin settings '{"allow_multiple_subscriptions":true}' >/dev/null
open_page /dashboard/api
wait_text "Monthly spend cap" || true
shots api-not-enrolled
ab fill 'input[inputmode=decimal]' "25" >/dev/null
dom_click "Add card" >/dev/null
wait_url "$MOCK/checkout/"
check "the API checkout saves a card" grep -q "No charge today" <<<"$(page_text)"
pay
wait_url /checkout/success 30
check "the success page says API billing is on" wait_text "API billing is on"
shots success-api
api_uid=$(owner_sql "select id from users where email = '$api'")
check "the cap reached the subscription" until_sql "select spend_cap_cents from subscriptions where user_id = '$api_uid' and kind = 'api'" 2500
open_page /dashboard/api
check "the API page shows enrolled with the cap" wait_text "\$25.00"
shots api-enrolled
dom_click "Change cap" >/dev/null
ab fill 'input[inputmode=decimal]' "40" >/dev/null
dom_click "Save cap" >/dev/null
check "the cap edit saves" until_sql "select spend_cap_cents from subscriptions where user_id = '$api_uid' and kind = 'api'" 4000
admin card "{\"external_customer_id\":\"$api_uid\",\"decline\":true}" >/dev/null
api_sub=$(mock_state | python3 -c "import json,sys; print([s['id'] for s in json.load(sys.stdin)['subscriptions'] if s['customer'].get('external_id')=='$api_uid'][0])")
admin renew "{\"subscription_id\":\"$api_sub\",\"usage_cents\":512}" >/dev/null
check "a failed API payment shows" until_sql "select status from subscriptions where user_id = '$api_uid' and kind = 'api'" past_due
open_page /dashboard/api
wait_text "PAYMENT FAILED" || true
shots api-payment-failed
admin card "{\"external_customer_id\":\"$api_uid\",\"decline\":false}" >/dev/null
admin retry-payment "{\"subscription_id\":\"$api_sub\"}" >/dev/null

# 10. The daily digest email.
cron '17+3+*+*+*'
did=$(wait_mail "alerts@convt.test" "convt billing")
check "the daily digest reaches ALERT_EMAIL" test -n "$did"
email_shots alert-digest "$did"

# 11. Account deletion with live Pro and API subscriptions.
sign_in_code "$pro"
open_page /dashboard/api
ab fill 'input[inputmode=decimal]' "20" >/dev/null
dom_click "Add card" >/dev/null
wait_url "$MOCK/checkout/"
pay
wait_url /checkout/success 30
until_sql "select count(*) from subscriptions where user_id = '$uid' and kind = 'api' and status = 'active'" 1 >/dev/null
open_page /account
dom_click "Delete account" >/dev/null
wait_text "to confirm" || true
ab fill "input[spellcheck=false]" "$pro" >/dev/null
shots account-delete-confirm
dom_click "Delete my account" >/dev/null
check "deletion shows its progress" wait_text "Deleting your account"
shots account-deleting
check "the account is deleted" until_sql "select count(*) from users where id = '$uid'" 0 60
check "both subscriptions ended and keep the email" test "$(owner_sql "select string_agg(status, ',' order by kind) from subscriptions where email = '$pro'")" = "canceled,canceled"
check "the provider shows them canceled" test "$(mock_state | python3 -c "import json,sys; print(sorted({s['status'] for s in json.load(sys.stdin)['subscriptions'] if s['customer']['email']=='$pro'}))")" = "['canceled']"
check "financial rows keep the email and lose the user" test "$(owner_sql "select count(*) from licenses where email = '$pro' and user_id is null")" -ge 1

ab close >/dev/null 2>&1 || true
finish
