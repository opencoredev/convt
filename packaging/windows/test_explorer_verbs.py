#!/usr/bin/env python3
"""Classic Explorer verb table and WiX fragment, without a Windows build."""

from __future__ import annotations

import unittest
from pathlib import Path

import gen_explorer_verbs as verbs

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[1]


class ExplorerVerbs(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.formats = verbs.parse_formats((REPO / "crates/convt-core/src/formats.rs").read_text())
        cls.rows = verbs.associations(cls.formats)
        cls.wxs = verbs.generate()

    def test_every_format_extension_is_registered(self) -> None:
        registered = {extension for extension, _, _ in self.rows}
        expected = {extension for _, _, extensions in self.formats for extension in extensions}
        self.assertEqual(registered, expected)

    def test_png_submenu_matches_the_core_menu_policy(self) -> None:
        png = [targets for extension, source, targets in self.rows if extension == "png"]
        self.assertEqual(png, [["jpeg", "webp"]])

    def test_gif_keeps_video_and_still_targets(self) -> None:
        gif = [targets for extension, source, targets in self.rows if extension == "gif"]
        self.assertEqual(gif, [["mp4", "webp", "png"]])

    def test_docx_and_mp4_use_their_category_lists(self) -> None:
        docx = next(targets for extension, _, targets in self.rows if extension == "docx")
        mp4 = next(targets for extension, _, targets in self.rows if extension == "mp4")
        self.assertEqual(docx, ["pdf", "txt"])
        self.assertEqual(mp4, ["mov", "gif", "mp3"])

    def test_wxs_uses_hkmu_system_file_associations_and_the_gui_exe(self) -> None:
        self.assertIn('Root="HKMU"', self.wxs)
        self.assertIn(r"Software\Classes\SystemFileAssociations\.png\shell\ConvertWithConvt", self.wxs)
        self.assertIn("ForceDeleteOnUninstall=\"yes\"", self.wxs)
        self.assertIn("MultiSelectModel", self.wxs)
        self.assertIn("&quot;[INSTALLFOLDER]convt-app.exe&quot; open --to jpeg -- &quot;%1&quot;", self.wxs)
        self.assertIn("&quot;[INSTALLFOLDER]convt-app.exe&quot; open -- &quot;%1&quot;", self.wxs)
        self.assertNotIn("convt.exe", self.wxs.replace("convt-app.exe", ""))
        self.assertIn('Id="SendToShortcut"', self.wxs)
        self.assertIn('Target="[INSTALLFOLDER]convt-app.exe"', self.wxs)
        self.assertIn('Arguments="open --"', self.wxs)
        self.assertNotIn("@", self.wxs)

    def test_committed_fragment_matches_the_generator(self) -> None:
        self.assertEqual(0, verbs.main(["--check"]))

    def test_package_wxs_includes_the_verb_group(self) -> None:
        package = (HERE / "convt.wxs").read_text()
        self.assertIn('<ComponentGroupRef Id="ExplorerVerbs" />', package)
        self.assertNotIn("ConvertWithConvt", package)


if __name__ == "__main__":
    unittest.main()
