"""Windows public assets include the MSI and the on-demand document pack."""
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

SCRIPT = Path(__file__).with_name("assemble.py")


def write(path: Path, data: bytes = b"x") -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(data)


def linux_tree(root: Path, version: str) -> None:
    review = root / "linux-release-review"
    write(review / f"convt-{version}-source.tar.gz")
    write(review / "convt_0.2.0-1_amd64.deb")
    write(review / f"convt-{version}-1.x86_64.rpm")
    write(review / "convt-linux-x86_64.AppImage")
    write(review / "convt-linux-x86_64.tar.gz")
    write(
        review / "source-audit.json",
        json.dumps(
            {
                "distribution_ready": True,
                "gaps": [],
                "covered_platforms": ["linux-x86_64", "macos-arm64"],
                "platform_gaps": {},
            }
        ).encode(),
    )
    write(root / "macos-release-review" / "convt-macos-arm64.dmg")


class AssembleWindowsDocumentsTest(unittest.TestCase):
    def test_windows_requires_the_document_pack_and_checksum(self) -> None:
        with tempfile.TemporaryDirectory() as raw:
            downloads = Path(raw) / "downloaded"
            linux_tree(downloads, "0.2.0")
            windows = downloads / "windows-release-review"
            write(windows / "convt-0.2.0-windows-x86_64.msi")
            out = Path(raw) / "release"
            missing = subprocess.run(
                [sys.executable, str(SCRIPT), str(downloads), str(out), "0.2.0", "--include-windows"],
                capture_output=True,
                text=True,
            )
            self.assertNotEqual(missing.returncode, 0)
            self.assertIn("documents.tar.gz", missing.stderr + missing.stdout)

    def test_windows_copies_msi_pack_and_checksum(self) -> None:
        with tempfile.TemporaryDirectory() as raw:
            downloads = Path(raw) / "downloaded"
            linux_tree(downloads, "0.2.0")
            windows = downloads / "windows-release-review"
            write(windows / "convt-0.2.0-windows-x86_64.msi", b"msi")
            write(windows / "convt-0.2.0-windows-x86_64-documents.tar.gz", b"pack")
            write(
                windows / "convt-0.2.0-windows-x86_64-documents.tar.gz.sha256",
                b"deadbeef  convt-0.2.0-windows-x86_64-documents.tar.gz\n",
            )
            out = Path(raw) / "release"
            result = subprocess.run(
                [sys.executable, str(SCRIPT), str(downloads), str(out), "0.2.0", "--include-windows"],
                capture_output=True,
                text=True,
                check=True,
            )
            self.assertIn("Assembled", result.stdout)
            self.assertEqual((out / "convt-0.2.0-windows-x86_64.msi").read_bytes(), b"msi")
            self.assertEqual(
                (out / "convt-0.2.0-windows-x86_64-documents.tar.gz").read_bytes(), b"pack"
            )
            self.assertTrue(
                (out / "convt-0.2.0-windows-x86_64-documents.tar.gz.sha256").is_file()
            )


if __name__ == "__main__":
    unittest.main()
