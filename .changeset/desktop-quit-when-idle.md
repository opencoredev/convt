---
"@convt/desktop": patch
---

On Linux and Windows, convt quits once its last window is closed and its conversions finish, including conversions started from the file manager, instead of staying in the background with nothing to reopen it from. The Menu bar icon switch, which only does something on macOS, is no longer shown there.
