"""Homebrew cask shape, bumping, and the post-release updater."""
from importlib.util import module_from_spec, spec_from_file_location
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[1]

spec = spec_from_file_location("homebrew_cask", HERE / "homebrew_cask.py")
homebrew = module_from_spec(spec)
spec.loader.exec_module(homebrew)

CASK = (REPO / "Casks" / "convt.rb").read_text()


def manifest(version="0.2.1", sha256="b" * 64):
    return {
        "builds": [
            {
                "version": version,
                "artifacts": [
                    {
                        "platform": "macos-arm64",
                        "kind": "dmg",
                        "url": f"https://github.com/opencoredev/convt/releases/download/v{version}/convt-macos-arm64.dmg",
                        "sha256": sha256,
                    }
                ],
            }
        ]
    }


class CaskFileTests(unittest.TestCase):
    def test_committed_cask_is_valid(self):
        homebrew.validate_cask(CASK)
        self.assertIn('version "0.2.0"', CASK)
        self.assertIn("8fc47f8b9873adbf4ad6df1db9e050d74205c26cff7e8c7e919ce0d398ae2ea3", CASK)
        self.assertIn("convt-macos-arm64.dmg", CASK)
        self.assertIn("depends_on arch: :arm64", CASK)
        self.assertIn("depends_on macos: :ventura", CASK)
        self.assertNotIn("x86_64", CASK)
        self.assertNotIn("intel", CASK)
        self.assertNotIn('arch arm:', CASK)
        self.assertIn("~/Library/Application Support/convt", CASK)
        for line in CASK.splitlines():
            if line and not line.startswith("cask ") and line != "end":
                self.assertTrue(len(line) - len(line.lstrip(" ")) in {2, 4, 6}, line)

    def test_check_cli(self):
        result = subprocess.run(
            [sys.executable, str(HERE / "update-homebrew-cask.py"), "--cask", str(REPO / "Casks" / "convt.rb"), "--check"],
            check=False,
            capture_output=True,
            text=True,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("ok ", result.stdout)


class BumpTests(unittest.TestCase):
    def test_replaces_version_and_sha_only(self):
        bumped = homebrew.bump_cask(CASK, version="0.2.1", sha256="c" * 64)
        self.assertIn('version "0.2.1"', bumped)
        self.assertIn('sha256 "' + "c" * 64 + '"', bumped)
        self.assertNotIn('version "0.2.0"', bumped)
        self.assertIn("livecheck do", bumped)
        self.assertIn("zap trash:", bumped)
        self.assertEqual(bumped.count("cask "), 1)

    def test_rejects_bad_values(self):
        with self.assertRaisesRegex(ValueError, "invalid version"):
            homebrew.bump_cask(CASK, version="v0.2.1", sha256="c" * 64)
        with self.assertRaisesRegex(ValueError, "invalid sha256"):
            homebrew.bump_cask(CASK, version="0.2.1", sha256="C" * 64)

    def test_manifest_reads_latest_mac_dmg(self):
        version, digest = homebrew.dmg_from_manifest(manifest())
        self.assertEqual(version, "0.2.1")
        self.assertEqual(digest, "b" * 64)
        with self.assertRaisesRegex(ValueError, "macos-arm64 dmg"):
            homebrew.dmg_from_manifest({"builds": [{"version": "1.0.0", "artifacts": []}]})

    def test_cli_rewrites_a_copy(self):
        with tempfile.TemporaryDirectory(prefix="convt-cask-") as tmp:
            dest = Path(tmp) / "Casks" / "convt.rb"
            dest.parent.mkdir()
            dest.write_text(CASK)
            payload = Path(tmp) / "release-manifest.json"
            payload.write_text(json.dumps(manifest("0.3.0", "d" * 64)))
            result = subprocess.run(
                [
                    sys.executable,
                    str(HERE / "update-homebrew-cask.py"),
                    "--cask",
                    str(dest),
                    "--manifest",
                    str(payload),
                    "--skip-tap",
                ],
                check=False,
                capture_output=True,
                text=True,
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            text = dest.read_text()
            self.assertIn('version "0.3.0"', text)
            self.assertIn('sha256 "' + "d" * 64 + '"', text)
            self.assertIn("updated ", result.stdout)


class GuardTests(unittest.TestCase):
    def test_http_manifest_url_is_refused(self):
        with self.assertRaisesRegex(ValueError, "https"):
            homebrew.fetch_json("http://example.test/release-manifest.json", attempts=1, delay=0)

    def test_missing_tap_error_omits_the_token(self):
        token = "SECRETTOKENVALUE"
        with self.assertRaises(ValueError) as raised:
            homebrew.publish_tap(
                REPO / "Casks" / "convt.rb",
                tap="opencoredev/this-tap-does-not-exist-3756",
                token=token,
                version="0.2.0",
            )
        self.assertIn("HOMEBREW_TAP_TOKEN", str(raised.exception))
        self.assertNotIn(token, str(raised.exception))

    def test_release_workflow_cannot_block_publish(self):
        text = (REPO / ".github" / "workflows" / "release.yml").read_text()
        self.assertIn("continue-on-error: true", text)
        self.assertIn("Update Homebrew cask", text)
        self.assertIn("HOMEBREW_TAP_TOKEN", text)
        self.assertLess(text.index("gh release create"), text.index("Update Homebrew cask"))


class TemplateTests(unittest.TestCase):
    def test_templates_emit_the_bumped_cask(self):
        with tempfile.TemporaryDirectory(prefix="convt-templates-") as tmp:
            root = Path(tmp)
            (root / "convt-macos-arm64.dmg").write_bytes(b"dmg")
            digest = __import__("hashlib").sha256(b"dmg").hexdigest()
            payload = {
                "distribution_ready": True,
                "builds": [
                    {
                        "version": "0.4.0",
                        "artifacts": [
                            {
                                "platform": "macos-arm64",
                                "kind": "dmg",
                                "url": "https://github.com/opencoredev/convt/releases/download/v0.4.0/convt-macos-arm64.dmg",
                                "size": 3,
                                "sha256": digest,
                            }
                        ],
                    }
                ],
            }
            manifest_path = root / "release-manifest.json"
            manifest_path.write_text(json.dumps(payload))
            out = root / "repositories"
            result = subprocess.run(
                [sys.executable, str(HERE / "templates.py"), str(manifest_path), str(out)],
                check=False,
                capture_output=True,
                text=True,
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            rendered = (out / "homebrew" / "convt.rb").read_text()
            self.assertIn('version "0.4.0"', rendered)
            self.assertIn(digest, rendered)
            self.assertIn("livecheck do", rendered)
            self.assertIn("zap trash:", rendered)
            status = json.loads((out / "STATUS.json").read_text())
            self.assertEqual(status["homebrew"], "rendered")


if __name__ == "__main__":
    unittest.main()
