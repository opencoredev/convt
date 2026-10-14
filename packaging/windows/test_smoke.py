"""Smoke must convert a document and fail a dead GUI; installer cleanup is owned."""
import unittest
import xml.etree.ElementTree as ET
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
        self.assertIn("LOCALAPPDATA", SMOKE)
        self.assertIn("CONVT_DATA_DIR", SMOKE)
        self.assertIn("NewGuid", SMOKE)
        self.assertIn("convt-smoke-", SMOKE)
        self.assertIn("msi-extract-", SMOKE)
        self.assertIn("Remove-OwnedDir", SMOKE)

    def test_gui_launch_fails_when_the_app_exits(self) -> None:
        self.assertIn("convt-app.exe exited", SMOKE)
        self.assertIn("convt-app.exe did not start", SMOKE)
        self.assertNotIn("launch skipped", SMOKE)
        self.assertNotIn("CLI conversions still passed", SMOKE)


class UrlProtocolTest(unittest.TestCase):
    """Desktop sign-in returns through convt://auth, so the MSI must register it."""

    def test_msi_registers_convt_links_for_the_installed_app(self) -> None:
        ns = {"w": "http://wixtoolset.org/schemas/v4/wxs"}
        root = ET.parse(WINDOWS / "convt.wxs").getroot()
        refs = [r.get("Id") for r in root.iterfind(".//w:Feature/w:ComponentRef", ns)]
        self.assertIn("UrlProtocol", refs)
        component = root.find(".//w:Component[@Id='UrlProtocol']", ns)
        self.assertIsNotNone(component)
        key = component.find("w:RegistryKey", ns)
        self.assertEqual(key.get("Root"), "HKCU")
        self.assertEqual(key.get("Key"), "Software\\Classes\\convt")
        values = {(v.get("Key"), v.get("Name")): v.get("Value") for v in key.iterfind("w:RegistryValue", ns)}
        self.assertEqual(values[(None, None)], "URL:convt")
        self.assertEqual(values[(None, "URL Protocol")], "")
        self.assertEqual(
            values[("shell\\open\\command", None)], '"[INSTALLFOLDER]convt-app.exe" "%1"'
        )

    def test_smoke_opens_a_link_and_uninstalls(self) -> None:
        self.assertIn("Test-UrlProtocol", SMOKE)
        self.assertIn("convt://auth?state=smoke", SMOKE)
        self.assertIn("uninstalling left the convt protocol key behind", SMOKE)


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
