---
"@convt/desktop": patch
"@convt/cli": patch
"@convt/web": patch
---

The Windows desktop app no longer opens a black console window when launched from the Start menu, and conversion tools no longer flash one. Linux .desktop entries set Terminal=false so a launching terminal does not need to stay open.
