"""The Windows MSI harvest may only contain runtime files."""
import tempfile
import unittest
from pathlib import Path

from stage_payload import DLL_FAMILIES, REQUIRED, stage, validate


def write(path: Path, size: int = 8) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(b"x" * size)


def runtime_payload(root: Path) -> None:
    for name in REQUIRED:
        write(root / name, 32 if name.endswith((".exe", ".dll")) else 16)
    write(root / "heif.dll")
    write(root / "libde265.dll")
    write(root / "libx265.dll")
    write(root / "aom.dll")
    write(root / "msvcp140.dll")
    write(root / "vcruntime140.dll")
    write(root / "licenses" / "ffmpeg-LICENSE.txt")
    write(root / "licenses" / "pdfium-LICENSE.txt")
    write(root / "licenses" / "inputs.lock.json")
    write(root / "licenses" / "libheif-explicit-init.patch")
    write(root / "licenses" / "codecs" / "aom" / "LICENSE")


class StagePayloadTest(unittest.TestCase):
    def test_runtime_payload_is_copied_largest_first(self) -> None:
        with tempfile.TemporaryDirectory() as raw:
            source = Path(raw) / "payload"
            dest = Path(raw) / "msi"
            runtime_payload(source)
            write(source / "convt-app.exe", 100)
            write(source / "ffmpeg.exe", 80)
            staged = stage(source, dest)
            self.assertEqual(staged[0][0].as_posix(), "convt-app.exe")
            self.assertTrue((dest / "convt.exe").is_file())
            self.assertTrue((dest / "licenses" / "ffmpeg-LICENSE.txt").is_file())
            self.assertTrue((dest / "licenses" / "codecs" / "aom" / "LICENSE").is_file())
            self.assertEqual(validate(source), [])

    def test_document_pack_and_sources_are_refused(self) -> None:
        with tempfile.TemporaryDirectory() as raw:
            source = Path(raw) / "payload"
            dest = Path(raw) / "msi"
            runtime_payload(source)
            write(source / "documents.tar.gz", 64)
            write(source / "licenses" / "native-source" / "aom" / "aom.c", 64)
            write(source / "convt.pdb")
            write(source / "heif.lib")
            write(source / "include" / "heif.h")
            errors = "\n".join(validate(source))
            self.assertIn("documents.tar.gz", errors)
            self.assertIn("native-source", errors)
            self.assertIn("convt.pdb", errors)
            self.assertIn("heif.lib", errors)
            self.assertIn("heif.h", errors)
            with self.assertRaises(SystemExit) as raised:
                stage(source, dest)
            self.assertIn("documents.tar.gz", str(raised.exception))
            self.assertFalse(dest.exists())

    def test_missing_codec_dll_is_refused(self) -> None:
        with tempfile.TemporaryDirectory() as raw:
            source = Path(raw) / "payload"
            runtime_payload(source)
            (source / "heif.dll").unlink()
            errors = "\n".join(validate(source))
            self.assertIn("libheif", errors)
            self.assertTrue(any(name in errors for name in DLL_FAMILIES["libheif"]))


if __name__ == "__main__":
    unittest.main()
