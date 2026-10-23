#!/usr/bin/env python3
"""PE subsystem helper and packaging checks that do not need a Windows build."""

import struct
import tempfile
import unittest
from pathlib import Path

from pe_subsystem import WINDOWS_CUI, WINDOWS_GUI, pe_subsystem, require

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[1]


def fake_pe(subsystem: int, pe32plus: bool = True) -> bytes:
    optional = bytearray(72)
    struct.pack_into("<H", optional, 0, 0x20B if pe32plus else 0x10B)
    struct.pack_into("<H", optional, 68, subsystem)
    e_lfanew = 0x80
    buf = bytearray(e_lfanew + 24 + 72)
    buf[0:2] = b"MZ"
    struct.pack_into("<I", buf, 0x3C, e_lfanew)
    buf[e_lfanew : e_lfanew + 4] = b"PE\0\0"
    buf[e_lfanew + 24 : e_lfanew + 24 + 72] = optional
    return bytes(buf)


class PeSubsystem(unittest.TestCase):
    def test_reads_gui_and_console(self):
        for expected, pe32plus in ((WINDOWS_GUI, True), (WINDOWS_CUI, False)):
            with tempfile.TemporaryDirectory() as tmp:
                path = Path(tmp) / "app.exe"
                path.write_bytes(fake_pe(expected, pe32plus=pe32plus))
                self.assertEqual(pe_subsystem(path), expected)
                require(path, expected)

    def test_rejects_a_non_pe(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "note.txt"
            path.write_text("not an exe")
            with self.assertRaises(ValueError):
                pe_subsystem(path)


class Packaging(unittest.TestCase):
    def test_start_menu_shortcut_targets_the_gui_exe(self):
        wxs = (HERE / "convt.wxs").read_text()
        shortcuts = [
            line for line in wxs.splitlines() if "<Shortcut" in line and "Target=" in line
        ]
        self.assertEqual(len(shortcuts), 1, shortcuts)
        self.assertIn('Target="[INSTALLFOLDER]convt-app.exe"', shortcuts[0])
        self.assertNotIn("convt.exe", shortcuts[0].replace("convt-app.exe", ""))

    def test_appimage_desktop_does_not_need_a_terminal(self):
        desktop = (REPO / "packaging/linux/convt.desktop").read_text()
        self.assertIn("\nTerminal=false\n", desktop)
        self.assertIn("Exec=convt-app", desktop)


if __name__ == "__main__":
    unittest.main()
