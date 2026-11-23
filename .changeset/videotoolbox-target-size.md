---
"@convt/desktop": patch
"@convt/cli": patch
---

Fitting a video under a target size works again on Macs that encode with VideoToolbox. The hardware encoder can't go below a certain bitrate on busy footage, so convt kept shrinking the budget and refused with "too small" even when the size was reachable. Now, when a hardware encode (VideoToolbox or NVENC) comes out too big, convt encodes it again in software at the same budget, which is slower but lands under the size.
