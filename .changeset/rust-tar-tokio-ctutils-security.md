---
"@convt/desktop": patch
"@convt/cli": patch
---

Update the Rust dependencies tar to 0.4.46 (GHSA-3pv8-6f4r-ffg2), tokio to 1.53.3 (RUSTSEC-2026-0386), ctutils to 0.4.4 (RUSTSEC-2026-0385), roxmltree to 0.21.2 in the conversion engine (CVE-2026-92987; the copy at 0.20.0 used by the desktop app's SVG rendering has no fix in its release line yet) and heapless to 0.9.4 (RUSTSEC-2026-0574 to RUSTSEC-2026-0582). Conversions behave the same.
