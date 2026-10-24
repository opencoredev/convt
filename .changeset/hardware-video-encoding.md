---
"@convt/desktop": patch
"@convt/cli": patch
---

Video encodes to H.264 and HEVC on the graphics hardware when it works: VideoToolbox on Macs and NVENC on NVIDIA cards. Conversions finish sooner and leave the CPU free. If the hardware can't handle a file, convt encodes it in software as before. Set `CONVT_HWACCEL=0` to always use software.
