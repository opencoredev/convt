---
"@convt/desktop": patch
"@convt/cli": patch
---

Update the Markdown parser, pulldown-cmark, to 0.13.6 for RUSTSEC-2026-0433. convt does not turn on the wikilink syntax that advisory affects, so conversions behave the same.
