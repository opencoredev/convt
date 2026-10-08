"""Smoke must convert a document and fail a dead GUI; installer cleanup is owned."""
import unittest
from pathlib import Path

WINDOWS = Path(__file__).resolve().parent
SMOKE = WINDOWS.joinpath("smoke.ps1").read_text(encoding="utf-8")
INSTALLER = WINDOWS.joinpath("installer.ps1").read_text(encoding="utf-8")


class SmokeScriptTest(unittest.TestCase):
    def test_installs_the_document_pack_and_converts_docx_to_pdf(self) -> None:
        self.assertIn("pack install documents", SMOKE)
        self.assertIn("--source", SMOKE)
        self.assertIn("--sha256", SMOKE)
        self.assertIn("sample.docx", SMOKE)
        self.assertIn("--to pdf", SMOKE)
        self.assertIn("document conversion failed", SMOKE)
        self.assertIn("%PDF-", SMOKE)

    def test_gui_launch_fails_when_the_app_exits(self) -> None:
        self.assertIn("convt-app.exe exited", SMOKE)
        self.assertIn("convt-app.exe did not start", SMOKE)
        self.assertNotIn("launch skipped", SMOKE)
        self.assertNotIn("CLI conversions still passed", SMOKE)


class InstallerCleanupTest(unittest.TestCase):
    def test_retry_cleanup_only_removes_owned_msi_payload(self) -> None:
        self.assertIn("msi-payload", INSTALLER)
        self.assertIn("outside packaging/out/windows", INSTALLER)
        self.assertIn("ReparsePoint", INSTALLER)
        self.assertIn("not the MSI harvest directory", INSTALLER)
        # `$ExpectedStage:` is a ParserError in pwsh; the name must be ${ExpectedStage}.
        self.assertNotRegex(INSTALLER, r'(?<!\{)\$ExpectedStage:')


if __name__ == "__main__":
    unittest.main()
