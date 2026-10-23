#!/usr/bin/env bash
# The email code boxes (components/app/code-input.tsx): keyboard movement, a pasted
# code that submits once, input ignored while a code is checked, a wrong code that
# clears and refocuses, typing during the shake, the 320 px fit, and a change-email
# reply that lands after Cancel.
source "$(dirname "$0")/lib.sh"

js() { ab eval "$1" | tail -1 | tr -d '"'; }
digits='[...document.querySelectorAll("input[aria-label^=Digit]")].map((i) => i.value || "_").join("")'
focused='document.activeElement?.getAttribute("aria-label") ?? "none"'
# Counts code submissions to $1 (window.__n) and their finished responses
# (window.__done). While `hold` is on, each one waits until `release`.
watch_requests() {
  js "window.__n = 0; window.__done = 0; window.__held = false; window.__waiting = []; const f = window.fetch; window.fetch = async (...a) => { if (!String(a[0]).includes('$1')) return f(...a); window.__n++; if (window.__held) await new Promise((r) => window.__waiting.push(r)); try { return await f(...a); } finally { window.__done++; } }; 'ok'" >/dev/null
}
hold() { js "window.__held = true; 'ok'" >/dev/null; }
release() { js "window.__held = false; window.__waiting.splice(0).forEach((r) => r()); 'ok'" >/dev/null; }
wait_done() { ab wait --fn "window.__done >= $1" >/dev/null; }
paste_into() { # paste_into box text
  js "(() => { const el = document.querySelector('input[aria-label=\"Digit $1 of 6\"]'); el.focus(); const dt = new DataTransfer(); dt.setData('text/plain', '$2'); el.dispatchEvent(new ClipboardEvent('paste', { clipboardData: dt, bubbles: true, cancelable: true })); return 'pasted'; })()" >/dev/null
}
start_sign_in() {
  reset_limits
  sign_out_all_cookies
  clear_mail
  open_page "/sign-in"
  ab fill 'input[type=email]' "$1" >/dev/null
  ab click 'button[type=submit]' >/dev/null
  wait_url /sign-in/check-email
  hydrated
}

width desktop
theme light

start_sign_in new@convt.test
check "the first box has focus when the page opens" test "$(js "$focused")" = "Digit 1 of 6"
watch_requests /sign-in/email-otp
ab keyboard type 12 >/dev/null
check "typing fills a box and moves to the next" test "$(js "$digits")/$(js "$focused")" = "12____/Digit 3 of 6"
ab press Backspace >/dev/null
ab press Backspace >/dev/null
check "Backspace in an empty box clears the boxes before it" test "$(js "$digits")/$(js "$focused")" = "______/Digit 1 of 6"
ab keyboard type 345 >/dev/null
ab press Home >/dev/null
check "Home goes to the first box" test "$(js "$focused")" = "Digit 1 of 6"
ab press End >/dev/null
ab press ArrowLeft >/dev/null
check "End and ArrowLeft move between boxes" test "$(js "$focused")" = "Digit 5 of 6"
ab click 'input[aria-label="Digit 6 of 6"]' >/dev/null
check "a click past the first empty box lands on it" test "$(js "$focused")" = "Digit 4 of 6"
check "nothing is sent before the last digit" test "$(js "window.__n")" = 0

hold
paste_into 3 "000 000"
check "a pasted code fills every box" test "$(js "$digits")" = 000000
check "and is sent on its own, once" test "$(js "window.__n")" = 1
check "the boxes are busy while it is checked" test "$(js 'document.querySelector("[role=group]").getAttribute("aria-busy")')" = true
shot code-input-checking-desktop-light
paste_into 1 999999
ab press Backspace >/dev/null
ab keyboard type 7 >/dev/null
check "input while checking changes nothing and sends nothing" test "$(js "$digits")/$(js "window.__n")" = "000000/1"
release
ab wait --text "That code isn't right" >/dev/null
ab wait --fn "[...document.querySelectorAll('input[aria-label^=Digit]')].every((i) => i.value === '')" >/dev/null
check "a wrong code clears the boxes and refocuses the first" test "$(js "$digits")/$(js "$focused")" = "______/Digit 1 of 6"
check "and marks them invalid" test "$(js 'document.querySelector("input[aria-label^=Digit]").getAttribute("aria-invalid")')" = true
shot code-input-wrong-desktop-light

# Typing straight after a wrong code cuts the shake short instead of losing digits.
paste_into 1 111111
ab wait --text "That code isn't right" >/dev/null
ab keyboard type "$(mail_code new@convt.test)" >/dev/null
check "a code typed during the shake signs in" wait_url /dashboard

width phone
ab set viewport 320 700 >/dev/null
start_sign_in new@convt.test
fit=$(js '(() => { const g = document.querySelector("[role=group]").getBoundingClientRect(); const b = [...document.querySelectorAll("input[aria-label^=Digit]")].map((i) => i.getBoundingClientRect()); return b[0].width === b[0].height && b[5].right <= g.right && document.documentElement.scrollWidth <= 320; })()')
check "at 320 px the boxes stay square and fit the column" test "$fit" = true
shot code-input-320-light
width desktop

# Change email: a reply that lands after Cancel must not close a form opened since.
user="code-$RANDOM@convt.test"
moved="code-moved-$RANDOM@convt.test"
sign_in_code "$user"
open_page /account
open_change_email() {
  js "[...document.querySelectorAll('button')].find((b) => b.textContent.trim() === 'Change email').click(); 'ok'" >/dev/null
  ab wait 'input[type=email]' >/dev/null
}
open_change_email
ab fill 'input[type=email]' "$moved" >/dev/null
reset_limits
js "[...document.querySelectorAll('button')].find((b) => b.textContent.trim() === 'Send code').click(); 'ok'" >/dev/null
ab wait 'input[aria-label="Digit 1 of 6"]' >/dev/null
watch_requests /email-otp/change-email
hold
ab keyboard type "$(mail_code "$moved")" >/dev/null
ab wait --fn "window.__n === 1" >/dev/null
js "[...document.querySelectorAll('button')].find((b) => b.textContent.trim() === 'Cancel').click(); 'ok'" >/dev/null
# Reopen while the reply is still held, so it can only land on the new form.
open_change_email
release
wait_done 1
# The success ring lasts 700 ms; a stray onAccepted would have closed the form by now.
sleep 1.2
check "a reply after Cancel leaves a newly opened form open" test "$(js 'document.querySelector("input[type=email]") ? "open" : "closed"')" = open

finish
