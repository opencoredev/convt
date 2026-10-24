#!/usr/bin/env python3
"""Read the PE Optional Header Subsystem field.

IMAGE_SUBSYSTEM_WINDOWS_GUI is 2 (no console). IMAGE_SUBSYSTEM_WINDOWS_CUI
is 3 (console). Used by smoke.ps1 and CI to prove convt-app.exe is a GUI
binary and convt.exe stays a console tool.
"""

from __future__ import annotations

import argparse
import struct
import sys
from pathlib import Path

WINDOWS_GUI = 2
WINDOWS_CUI = 3
NAMES = {WINDOWS_GUI: "WINDOWS_GUI", WINDOWS_CUI: "WINDOWS_CUI"}


def pe_subsystem(path: Path) -> int:
    data = path.read_bytes()
    if len(data) < 64 or data[:2] != b"MZ":
        raise ValueError(f"{path}: not a PE file")
    e_lfanew = struct.unpack_from("<I", data, 0x3C)[0]
    if e_lfanew + 24 + 70 > len(data) or data[e_lfanew : e_lfanew + 4] != b"PE\0\0":
        raise ValueError(f"{path}: missing PE signature")
    magic = struct.unpack_from("<H", data, e_lfanew + 24)[0]
    if magic not in (0x10B, 0x20B):
        raise ValueError(f"{path}: unknown optional header magic {magic:#x}")
    return struct.unpack_from("<H", data, e_lfanew + 24 + 68)[0]


def require(path: Path, expected: int) -> int:
    got = pe_subsystem(path)
    if got != expected:
        want = NAMES.get(expected, str(expected))
        saw = NAMES.get(got, str(got))
        raise SystemExit(f"{path}: subsystem is {saw} ({got}), expected {want} ({expected})")
    return got


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("exe", type=Path)
    group = parser.add_mutually_exclusive_group(required=True)
    group.add_argument("--require-gui", action="store_true")
    group.add_argument("--require-console", action="store_true")
    args = parser.parse_args(argv)
    expected = WINDOWS_GUI if args.require_gui else WINDOWS_CUI
    got = require(args.exe, expected)
    print(f"{args.exe}: {NAMES[got]} ({got})")
    return 0


if __name__ == "__main__":
    sys.exit(main())
