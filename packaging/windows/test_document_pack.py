"""The Windows document pack is a named GitHub release asset, not an MSI member."""
import json
import os
import tempfile
import unittest
from pathlib import Path
from unittest import mock

from document_pack import asset_name, checksum_name, public_url, publish


class DocumentPackNamingTest(unittest.TestCase):
    def test_asset_names_are_versioned_and_platform_specific(self) -> None:
        self.assertEqual(
            asset_name("0.2.0"), "convt-0.2.0-windows-x86_64-documents.tar.gz"
        )
        self.assertEqual(
            checksum_name("0.2.0"),
            "convt-0.2.0-windows-x86_64-documents.tar.gz.sha256",
        )
        with self.assertRaises(ValueError):
            asset_name("not-a-version")

    def test_public_url_points_at_the_github_release_asset(self) -> None:
        with mock.patch.dict(os.environ, {}, clear=True):
            self.assertEqual(
                public_url("0.2.1"),
                "https://github.com/opencoredev/convt/releases/download/v0.2.1/"
                "convt-0.2.1-windows-x86_64-documents.tar.gz",
            )
        with mock.patch.dict(os.environ, {"GITHUB_REPOSITORY": "acme/fork"}, clear=True):
            self.assertEqual(
                public_url("1.0.0"),
                "https://github.com/acme/fork/releases/download/v1.0.0/"
                "convt-1.0.0-windows-x86_64-documents.tar.gz",
            )
        with mock.patch.dict(
            os.environ, {"CONVT_DOCUMENT_PACK_URL": "https://example.test/pack.tar.gz"}, clear=True
        ):
            self.assertEqual(public_url("0.2.0"), "https://example.test/pack.tar.gz")

    def test_publish_writes_checksum_and_renames_the_archive(self) -> None:
        with tempfile.TemporaryDirectory() as raw:
            root = Path(raw)
            archive = root / "documents.tar.gz"
            archive.write_bytes(b"libreoffice-pack")
            receipt = publish(archive, "0.2.0", root)
            named = root / "convt-0.2.0-windows-x86_64-documents.tar.gz"
            checksum = root / "convt-0.2.0-windows-x86_64-documents.tar.gz.sha256"
            self.assertFalse(archive.exists())
            self.assertEqual(named.read_bytes(), b"libreoffice-pack")
            self.assertTrue(checksum.read_text(encoding="utf-8").startswith(receipt["sha256"]))
            self.assertIn("convt-0.2.0-windows-x86_64-documents.tar.gz", checksum.read_text())
            self.assertEqual(receipt["size"], len(b"libreoffice-pack"))
            json.dumps(receipt)


if __name__ == "__main__":
    unittest.main()
