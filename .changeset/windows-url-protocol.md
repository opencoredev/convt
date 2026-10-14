---
"@convt/desktop": patch
---

Signing in from the Windows app now finishes. The installer didn't register `convt://` links, so after you approved the sign-in in the browser, Windows had nothing to hand it back to and the app kept waiting. License links from convt.app open the app on Windows for the same reason.
