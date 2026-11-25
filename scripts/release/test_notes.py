"""Release notes from the fixed-version Changesets changelogs."""
from importlib.util import module_from_spec, spec_from_file_location
from pathlib import Path
import tempfile
import unittest

HERE = Path(__file__).resolve().parent
spec = spec_from_file_location("release_notes", HERE / "notes.py")
notes = module_from_spec(spec)
spec.loader.exec_module(notes)


def checkout(test, changelogs):
    scratch = tempfile.TemporaryDirectory()
    test.addCleanup(scratch.cleanup)
    root = Path(scratch.name)
    for package in notes.versions.PACKAGES:
        path = root / package
        path.parent.mkdir(parents=True)
        path.with_name("CHANGELOG.md").write_text(changelogs[path.parent.name])
    return root


WEB_ONLY = {
    "desktop": "# @convt/desktop\n\n## 0.7.0\n\n## 0.6.0\n\n- Old desktop change\n",
    "cli": "# @convt/cli\n\n## 0.7.0\n\n## 0.6.0\n\n- Old CLI change\n",
    "web": "# @convt/web\n\n## 0.7.0\n\n### Minor Changes\n\n- New API page\n\n## 0.6.0\n\n- Old web change\n",
}


class NotesTest(unittest.TestCase):
    def test_fixed_bump_without_entries_is_not_missing(self):
        body = notes.version_pr(checkout(self, WEB_ONLY), "0.7.0")
        self.assertIn("### desktop 0.7.0\n\nNo changes in this package", body)
        self.assertIn("### web 0.7.0\n\n### Minor Changes\n\n- New API page\n", body)
        self.assertNotIn("Old", body)

    def test_release_notes_fall_back_to_changed_packages(self):
        self.assertEqual(notes.release_notes(checkout(self, WEB_ONLY), "0.7.0"),
                         "## web\n\n### Minor Changes\n\n- New API page")

    def test_release_notes_prefer_desktop(self):
        changelogs = dict(WEB_ONLY, desktop="## 0.7.0\n\n- Desktop change\n")
        self.assertEqual(notes.release_notes(checkout(self, changelogs), "0.7.0"), "- Desktop change")

    def test_missing_heading_still_fails(self):
        with self.assertRaisesRegex(ValueError, "Missing changelog for 0.8.0"):
            notes.version_pr(checkout(self, WEB_ONLY), "0.8.0")

    def test_missing_heading_fails_even_when_desktop_has_notes(self):
        changelogs = dict(WEB_ONLY, desktop="## 0.7.0\n\n- Desktop change\n", cli="## 0.6.0\n")
        with self.assertRaisesRegex(ValueError, "Missing changelog for 0.7.0"):
            notes.release_notes(checkout(self, changelogs), "0.7.0")

    def test_empty_heading_at_end_of_file(self):
        changelogs = dict(WEB_ONLY, cli="# @convt/cli\n\n## 0.7.0")
        self.assertIn("### cli 0.7.0\n\nNo changes", notes.version_pr(checkout(self, changelogs), "0.7.0"))

    def test_version_without_any_entries_fails(self):
        changelogs = dict(WEB_ONLY, web="## 0.7.0\n\n## 0.6.0\n")
        with self.assertRaisesRegex(ValueError, "No changelog entries for 0.7.0"):
            notes.release_notes(checkout(self, changelogs), "0.7.0")


if __name__ == "__main__":
    unittest.main()
