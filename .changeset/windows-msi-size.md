---
"@convt/desktop": patch
"@convt/cli": patch
"@convt/web": patch
---

The Windows installer no longer embeds the LibreOffice document pack or codec source trees, so the MSI is much closer to the Mac and Linux download size. Document support downloads once from the GitHub release, same as the existing Install flow on Mac and Linux.
