#!/usr/bin/env bash
# Licenses and settings actions: activate and copy a key, another user's key is
# refused, rename, connect and remove GitHub, a stale session is asked to sign in
# again, change email, sign out one session from a second browser, and sign out
# everywhere else. Needs `bun run db:seed`.
source "$(dirname "$0")/lib.sh"

width desktop
theme light

# A second browser with its own cookies.
SECOND=${E2E_SESSION}-second
ab2() { agent-browser --session "$SECOND" --profile "$HOME/.agent-browser/profiles/$SECOND" "$@"; }

dom_click() { # clicks through the DOM, which runs the same React handler
  ab eval "(() => { const b = [...document.querySelectorAll('button')].find((x) => x.getAttribute('aria-label') === $(printf '%s' "$1" | python3 -c 'import json,sys; print(json.dumps(sys.stdin.read()))') || x.textContent.trim() === $(printf '%s' "$1" | python3 -c 'import json,sys; print(json.dumps(sys.stdin.read()))')); if (!b) return 'missing'; b.click(); return 'clicked'; })()" | tr -d '"'
}
notice_text() { ab eval 'document.querySelector("[role=status]")?.innerText ?? ""' | tr -d '"'; }
wait_text() { ab wait --text "$1" >/dev/null 2>&1; }

# Activate and copy, as pro@.
sign_in_code pro@convt.test
open_page /dashboard/licenses
token=$(owner_sql "select token from licenses where user_id = (select id from users where email = 'pro@convt.test') and plan = 'pro'")
ab eval 'window.__opened = []; const click = HTMLAnchorElement.prototype.click; HTMLAnchorElement.prototype.click = function () { if (this.protocol === "convt:") { window.__opened.push(this.href); return; } return click.call(this); }; "stubbed"' >/dev/null
ab click 'button[aria-label="Activate Pro license on this computer"]' >/dev/null
ab wait --fn 'window.__opened.length > 0' >/dev/null
opened=$(ab eval 'window.__opened[0]' | tr -d '"')
check "Activate opens convt://activate?key=<that license's token>" test "$opened" = "convt://activate?key=$token"
html=$(curl -fsS -H "cookie: $(ab cookies get --json | python3 -c 'import json,sys; print("; ".join(c["name"]+"="+c["value"] for c in json.load(sys.stdin)["data"]["cookies"]))')" "$E2E_URL/dashboard/licenses" | tr -d "\\0")
check "the token is not in the server-rendered page" bash -c '! grep -qF "$1" <<<"$2"' _ "${token:0:40}" "$html"
ab clipboard write "empty" >/dev/null
ab click 'button[aria-label="Copy Pro license key"]' >/dev/null
wait_text "License key copied"
check "Copy key confirms" grep -q "License key copied" <<<"$(notice_text)"
# Headless Chrome will not let the test read the clipboard back, so record writes.
ab eval 'const write = navigator.clipboard.writeText.bind(navigator.clipboard); navigator.clipboard.writeText = (t) => { window.__copied = t; return write(t); }; "ok"' >/dev/null
ab click 'button[aria-label="Copy Pro license key"]' >/dev/null
ab wait --fn 'typeof window.__copied === "string"' >/dev/null
check "and copies that license's token" test "$(ab eval 'window.__copied' | tr -d '"')" = "$token"
shot licenses-copy-notice-desktop-light

# Another user's license is refused by getLicenseKey. Development serves source
# modules, so the page can call the client function directly.
pro_license=$(owner_sql "select id from licenses where user_id = (select id from users where email = 'pro@convt.test') and plan = 'pro'")
sign_in_code desktop@convt.test
result=$(ab eval "import('/src/lib/account.ts').then((m) => m.getLicenseKey('$pro_license')).then((t) => 'GOT ' + t.slice(0, 12), (e) => 'REFUSED ' + e.message)")
check "getLicenseKey refuses another user's license" grep -q "REFUSED" <<<"$result"
own=$(owner_sql "select id from licenses where user_id = (select id from users where email = 'desktop@convt.test')")
result=$(ab eval "import('/src/lib/account.ts').then((m) => m.getLicenseKey('$own')).then((t) => 'GOT ' + t.slice(0, 3), (e) => 'REFUSED ' + e.message)")
check "and returns the user's own" grep -q "GOT eyJ" <<<"$result"

# Settings, on a fresh account.
user="settings-$RANDOM@convt.test"
moved="moved-$RANDOM@convt.test"
sign_in_code "$user"
open_page /account
ab fill 'input[name=name]' "Sam Settings" >/dev/null
ab press Enter >/dev/null
wait_text "Name saved"
check "rename saves and the header follows" wait_text "Sam Settings's account"

dom_click "Connect GitHub" >/dev/null
wait_url /github/authorize
ab open "$(url_now)&identity=github-verified&email=gh-$RANDOM@convt.test" >/dev/null
check "Connect GitHub returns to settings" wait_url /account
check "GitHub shows as connected" grep -q "Connected" <<<"$(page_text | grep -A2 GitHub)"
shot settings-github-connected-desktop-light
dom_click "Remove GitHub" >/dev/null
wait_text "GitHub removed"
check "Remove GitHub disconnects it" test "$(owner_sql "select count(*) from accounts a join users u on u.id = a.user_id where u.email = '$user'")" = 0

owner_sql "update sessions set created_at = now() - interval '2 hours' where user_id = (select id from users where email = '$user')" >/dev/null
dom_click "Connect GitHub" >/dev/null
wait_text "Sign in again to continue"
check "a stale session is asked to sign in again before connecting" grep -q "Sign in again" <<<"$(page_text)"
shot settings-stale-session-desktop-light
theme dark
shot settings-stale-session-desktop-dark
theme light
owner_sql "update sessions set created_at = now() where user_id = (select id from users where email = '$user')" >/dev/null

open_page /account
dom_click "Change email" >/dev/null
ab fill 'input[type=email]' "$moved" >/dev/null
reset_limits
dom_click "Send code" >/dev/null
wait_text "Code sent to"
shot settings-change-email-code-desktop-light
type_code "$(mail_code "$moved")"
wait_text "Email changed"
wait_text "$moved"
check "change email confirms by code and shows the new address" grep -qF "$moved" <<<"$(page_text)"
check "the old address got no code" test "$(curl -fsS "$MAILPIT_URL/api/v1/search?query=to:$user" | python3 -c 'import json,sys; print(sum("confirmation" in m["Subject"] for m in json.load(sys.stdin)["messages"]))')" = 0

# One session signed out from another browser.
reset_limits
ab2 cookies clear >/dev/null
ab2 open "$E2E_URL/sign-in" >/dev/null
ab2 wait --fn 'Object.keys(document.querySelector("main") ?? {}).some((k) => k.startsWith("__reactFiber"))' >/dev/null
ab2 fill 'input[type=email]' "$moved" >/dev/null
ab2 click 'button[type=submit]' >/dev/null
ab2 wait --url "**/sign-in/check-email**" >/dev/null
ab2 click 'input[aria-label="Digit 1 of 6"]' >/dev/null
ab2 keyboard type "$(mail_code "$moved")" >/dev/null
ab2 wait --url "**/dashboard" >/dev/null
open_page /account
shot settings-two-sessions-desktop-light
other=$(owner_sql "select count(*) from sessions where user_id = (select id from users where email = '$moved')")
check "settings lists both browser sessions" test "$other" = 2
ab eval '(() => { const rows = [...document.querySelectorAll("section[aria-labelledby=sessions-title] tbody tr")]; const row = rows.find((r) => !r.innerText.includes("THIS BROWSER")); row.querySelector("button").click(); return "ok"; })()' >/dev/null
wait_text "Signed out"
ab2 open "$E2E_URL/dashboard" >/dev/null
check "the other browser is signed out" bash -c 'for _ in $(seq 40); do [[ $(agent-browser --session "$1" --profile "$HOME/.agent-browser/profiles/$1" get url) == *"/sign-in?redirect"* ]] && exit 0; sleep 0.25; done; exit 1' _ "$SECOND"
open_page /dashboard
check "this browser stays signed in" wait_url /dashboard

# Sign out everywhere else: other sessions and devices.
reset_limits
ab2 open "$E2E_URL/sign-in" >/dev/null
ab2 wait --fn 'Object.keys(document.querySelector("main") ?? {}).some((k) => k.startsWith("__reactFiber"))' >/dev/null
ab2 fill 'input[type=email]' "$moved" >/dev/null
ab2 click 'button[type=submit]' >/dev/null
ab2 wait --url "**/sign-in/check-email**" >/dev/null
ab2 click 'input[aria-label="Digit 1 of 6"]' >/dev/null
ab2 keyboard type "$(mail_code "$moved")" >/dev/null
ab2 wait --url "**/dashboard" >/dev/null
owner_sql "insert into devices (id, user_id, name, os) select 'dev_e2e_' || substr(md5(random()::text), 1, 20), id, 'E2E Mac', 'macOS 26.1' from users where email = '$moved'" >/dev/null
open_page /account
dom_click "Sign out everywhere else" >/dev/null
wait_text "Signed out 2 other sessions"
check "sign out everywhere else ends the other session and the device" grep -q "Signed out 2 other sessions" <<<"$(notice_text)"
check "no active devices remain" test "$(owner_sql "select count(*) from devices d join users u on u.id = d.user_id where u.email = '$moved' and d.revoked_at is null")" = 0
ab2 open "$E2E_URL/account" >/dev/null
check "the other browser is signed out" bash -c 'for _ in $(seq 40); do [[ $(agent-browser --session "$1" --profile "$HOME/.agent-browser/profiles/$1" get url) == *"/sign-in?redirect"* ]] && exit 0; sleep 0.25; done; exit 1' _ "$SECOND"
ab2 close >/dev/null 2>&1 || true

finish
