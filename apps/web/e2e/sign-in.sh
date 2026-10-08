#!/usr/bin/env bash
# Sign-in flows: email code, the emailed link, signed-out redirect and return,
# the OAuth mock identities, the email check for non-authoritative identities,
# account_not_linked, and the header's Sign out.
source "$(dirname "$0")/lib.sh"

width desktop
theme light
clear_mail

# Signed out: the dashboard sends you to sign-in and back.
sign_out_all_cookies
open_page "/dashboard/billing"
check "signed-out visit redirects with the return path" wait_url "/sign-in?redirect=%2Fdashboard%2Fbilling"
shot signin-desktop-light

# By code, returning to the page asked for.
reset_limits
ab fill 'input[type=email]' desktop@convt.test >/dev/null
ab click 'button[type=submit]' >/dev/null
check "the form sends a code and shows check-email" wait_url "/sign-in/check-email?email=desktop%40convt.test"
shot signin-check-email-desktop-light
check "check-email has no preview note" bash -c '! grep -q "Preview only" <<<"$1"' _ "$(page_text)"
type_code 000000
ab wait --text "isn't right" >/dev/null
check "a wrong code shows an inline error" grep -q "That code isn't right" <<<"$(page_text)"
shot signin-wrong-code-desktop-light
type_code "$(mail_code desktop@convt.test)"
check "the code signs in and returns to /dashboard/billing" wait_url "/dashboard/billing"

# Sign out from the header's account menu.
ab click 'header button[aria-expanded]' >/dev/null
ab find role button click --name "Sign out" >/dev/null
check "Sign out in the header ends the session" wait_url "/sign-in"
open_page "/dashboard"
check "after signing out the dashboard needs sign-in again" wait_url "/sign-in?redirect="

# By the emailed link: the fragment carries the code and is stripped on load.
reset_limits
clear_mail
open_page "/sign-in"
ab fill 'input[type=email]' trial@convt.test >/dev/null
ab click 'button[type=submit]' >/dev/null
wait_url /sign-in/check-email
link=$(mail_text trial@convt.test | grep -o 'http[^ ]*/sign-in/verify#[^ ]*' | head -1)
check "the email holds a /sign-in/verify link with the code in the fragment" grep -q "#email=trial%40convt.test&code=[0-9]\{6\}" <<<"$link"
trial_sessions() { owner_sql "select count(*) from sessions s join users u on u.id = s.user_id where u.email = 'trial@convt.test'"; }
before=$(trial_sessions)
ab open "$link" >/dev/null
ab wait --text "Continue as trial@convt.test" >/dev/null
check "the fragment is gone from the address bar on load" bash -c '[[ $1 != *"#"* && $1 != *code* ]]' _ "$(url_now)"
check "nothing signs in until Continue is pressed" test "$(trial_sessions)" = "$before"
shot signin-verify-link-desktop-light
ab find role button click --name "Continue" >/dev/null
check "Continue signs in" wait_url /dashboard
check "with one new session" test "$(trial_sessions)" = "$((before + 1))"
headers=$(curl -sI "$E2E_URL/sign-in/verify")
check "the verify page is no-store" grep -qiE "cache-control: (private, )?no-store" <<<"$headers"
check "the verify page sends no referrer" grep -qi "referrer-policy: no-referrer" <<<"$headers"

# OAuth through the mock.
oauth_sign_in() { # provider identity [email]
  sign_out_all_cookies
  reset_limits
  open_page "/sign-in"
  ab find role button click --name "$([[ $1 == github ]] && echo GitHub || echo "Continue with Google")" >/dev/null
  wait_url "/$1/authorize"
  local url
  url=$(url_now)
  ab open "$url&identity=$2${3:+&email=$3}" >/dev/null
}

# A new account lands on /download (CNV-69), a returning one on the dashboard.
owner_sql "delete from users where split_part(email, '@', 1) = 'gail.mock'" >/dev/null
oauth_sign_in google google-gmail
check "Google sign-up with a Gmail address lands on /download" wait_url /download
shot oauth-google-gmail-download-desktop-light
oauth_sign_in google google-gmail
check "signing in again with the same Google account lands on the dashboard" wait_url /dashboard

oauth_sign_in github github-pro
check "the GitHub identity linked to pro@ signs in to pro@" wait_url /dashboard
check "and shows pro's account" grep -q "Account: Leo" <<<"$(header_account)"

oauth_sign_in google google-thirdparty "e2e-thirdparty-$RANDOM@thirdparty.test"
check "a third-party Google address must confirm its email first" wait_url /sign-in/verify-email
shot verify-email-desktop-light
open_page "/dashboard"
check "the dashboard stays closed until then" wait_url /sign-in/verify-email
email=$(page_text | grep -o 'e2e-thirdparty-[0-9]*@thirdparty.test' | head -1)
ab find role button click --name "Email me a code" >/dev/null
ab wait 'input[aria-label="Digit 1 of 6"]' >/dev/null
shot verify-email-code-desktop-light
type_code "$(mail_code "$email")"
check "the code confirms it and opens the dashboard" wait_url /dashboard

# The rest of the mock identities: where each one lands after sign-up.
for case in "google google-workspace /download" "google google-unverified /sign-in/verify-email" \
  "github github-verified /sign-in/verify-email" "github github-public-differs /sign-in/verify-email" \
  "github github-no-email error=email_not_found"; do
  read -r provider identity lands <<<"$case"
  # Fresh sign-ups each run: remove what earlier runs created for the mock identities.
  owner_sql "delete from users where email in ('walt@workspace.test', 'una@unverified.test', 'octo@github-user.test', 'public@github-user.test')" >/dev/null
  oauth_sign_in "$provider" "$identity"
  check "$identity lands on $lands" wait_url "$lands"
done

oauth_sign_in github github-verified desktop@convt.test
check "an OAuth email that belongs to an account gets account_not_linked" wait_url "error=account_not_linked"
ab wait --text "isn't connected" >/dev/null
check "and the sign-in page says to use a code and connect it in Settings" grep -q "Sign in with an email code, then connect it in Settings" <<<"$(page_text)"
shot signin-account-not-linked-desktop-light

oauth_sign_in github error
check "a denied OAuth request comes back to sign-in with a message" wait_url "/sign-in?error="

finish
