"""Homebrew cask shape, bumping, and the post-release updater."""
from importlib.util import module_from_spec, spec_from_file_location
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[1]

spec = spec_from_file_location("homebrew_cask", HERE / "homebrew_cask.py")
homebrew = module_from_spec(spec)
spec.loader.exec_module(homebrew)

cli_spec = spec_from_file_location("update_homebrew_cask", HERE / "update-homebrew-cask.py")
cli = module_from_spec(cli_spec)
cli_spec.loader.exec_module(cli)

CASK = (REPO / "Casks" / "convt.rb").read_text()
CASK_VERSION, CASK_SHA = homebrew.cask_fields(CASK)


def manifest(version="0.2.1", sha256="b" * 64, url=None):
    return {
        "builds": [
            {
                "version": version,
                "artifacts": [
                    {
                        "platform": "macos-arm64",
                        "kind": "dmg",
                        "url": url
                        or f"https://github.com/opencoredev/convt/releases/download/v{version}/convt-macos-arm64.dmg",
                        "sha256": sha256,
                    }
                ],
            }
        ]
    }


class CaskFileTests(unittest.TestCase):
    def test_committed_cask_is_valid(self):
        homebrew.validate_cask(CASK)
        self.assertRegex(CASK_VERSION, r"^\d+\.\d+\.\d+$")
        self.assertRegex(CASK_SHA, r"^[0-9a-f]{64}$")
        self.assertIn(f'version "{CASK_VERSION}"', CASK)
        self.assertIn(CASK_SHA, CASK)
        self.assertIn("convt-macos-arm64.dmg", CASK)
        self.assertIn("depends_on arch: :arm64", CASK)
        self.assertIn("depends_on macos: :ventura", CASK)
        self.assertNotIn("x86_64", CASK)
        self.assertNotIn("intel", CASK)
        self.assertNotIn("arch arm:", CASK)
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
        bumped = homebrew.bump_cask(CASK, version="9.9.9", sha256="c" * 64)
        self.assertIn('version "9.9.9"', bumped)
        self.assertIn('sha256 "' + "c" * 64 + '"', bumped)
        self.assertNotIn(f'version "{CASK_VERSION}"', bumped)
        self.assertNotIn(CASK_SHA, bumped)
        self.assertIn("livecheck do", bumped)
        self.assertIn("zap trash:", bumped)
        self.assertEqual(bumped.count("cask "), 1)

    def test_url_override_keeps_the_manifest_host(self):
        url = "https://downloads.convt.app/9.9.9/convt-macos-arm64.dmg"
        bumped = homebrew.bump_cask(CASK, version="9.9.9", sha256="c" * 64, url=url)
        self.assertIn(url, bumped)
        self.assertNotIn("github.com/opencoredev/convt/releases/download", bumped)

    def test_rejects_bad_values(self):
        with self.assertRaisesRegex(ValueError, "invalid version"):
            homebrew.bump_cask(CASK, version="v0.2.1", sha256="c" * 64)
        with self.assertRaisesRegex(ValueError, "invalid sha256"):
            homebrew.bump_cask(CASK, version="0.2.1", sha256="C" * 64)

    def test_refuses_an_older_version(self):
        newer = homebrew.bump_cask(CASK, version="0.3.0", sha256="d" * 64)
        with self.assertRaisesRegex(ValueError, r"downgrade cask from 0\.3\.0 to 0\.2\.1"):
            homebrew.bump_cask(newer, version="0.2.1", sha256="e" * 64)
        same = homebrew.bump_cask(newer, version="0.3.0", sha256="f" * 64)
        self.assertIn('sha256 "' + "f" * 64 + '"', same)
        self.assertGreater(homebrew.version_key("0.2.10"), homebrew.version_key("0.2.9"))

    def test_manifest_reads_latest_mac_dmg(self):
        url = "https://downloads.convt.app/0.2.1/convt-macos-arm64.dmg"
        version, digest, got = homebrew.dmg_from_manifest(manifest(url=url))
        self.assertEqual(version, "0.2.1")
        self.assertEqual(digest, "b" * 64)
        self.assertEqual(got, url)
        with self.assertRaisesRegex(ValueError, "macos-arm64 dmg"):
            homebrew.dmg_from_manifest({"builds": [{"version": "1.0.0", "artifacts": []}]})
        with self.assertRaisesRegex(ValueError, "bad url"):
            homebrew.dmg_from_manifest(manifest(url="http://example.test/convt-macos-arm64.dmg"))

    def test_cli_rewrites_a_copy(self):
        url = "https://downloads.convt.app/0.3.0/convt-macos-arm64.dmg"
        with tempfile.TemporaryDirectory(prefix="convt-cask-") as tmp:
            dest = Path(tmp) / "Casks" / "convt.rb"
            dest.parent.mkdir()
            dest.write_text(CASK)
            payload = Path(tmp) / "release-manifest.json"
            payload.write_text(json.dumps(manifest("0.3.0", "d" * 64, url=url)))
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
            self.assertIn(url, text)
            self.assertNotIn("github.com/opencoredev/convt/releases/download", text)
            self.assertIn("updated ", result.stdout)

    def test_cli_refuses_an_older_release(self):
        with tempfile.TemporaryDirectory(prefix="convt-cask-old-") as tmp:
            dest = Path(tmp) / "Casks" / "convt.rb"
            dest.parent.mkdir()
            dest.write_text(homebrew.bump_cask(CASK, version="0.3.0", sha256="d" * 64))
            original = dest.read_text()
            payload = Path(tmp) / "release-manifest.json"
            payload.write_text(json.dumps(manifest("0.2.9", "e" * 64)))
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
            self.assertNotEqual(result.returncode, 0, result.stdout)
            self.assertIn("downgrade", result.stderr)
            self.assertEqual(dest.read_text(), original)


class GuardTests(unittest.TestCase):
    def test_http_manifest_url_is_refused(self):
        with self.assertRaisesRegex(ValueError, "https"):
            homebrew.fetch_json("http://example.test/release-manifest.json", attempts=1, delay=0)

    def test_missing_tap_error_omits_the_token(self):
        token = "SECRETTOKENVALUE"

        def fail_clone(url, dest, env):
            self.assertIn("this-tap-does-not-exist-3756", url)
            self.assertNotIn(token, url)
            raise subprocess.CalledProcessError(128, ["git", "clone"])

        with patch.object(homebrew, "clone_tap", side_effect=fail_clone):
            with self.assertRaises(ValueError) as raised:
                homebrew.publish_tap(
                    REPO / "Casks" / "convt.rb",
                    tap="opencoredev/this-tap-does-not-exist-3756",
                    token=token,
                    version="0.2.0",
                )
        self.assertIn("HOMEBREW_TAP_TOKEN", str(raised.exception))
        self.assertNotIn(token, str(raised.exception))

    def test_push_retries_when_the_cask_already_matches(self):
        with tempfile.TemporaryDirectory(prefix="convt-cask-retry-") as tmp:
            dest = Path(tmp) / "Casks" / "convt.rb"
            dest.parent.mkdir()
            dest.write_text(homebrew.bump_cask(CASK, version="0.3.0", sha256="d" * 64))
            payload = Path(tmp) / "release-manifest.json"
            payload.write_text(json.dumps(manifest("0.3.0", "d" * 64)))
            with (
                patch.object(cli.homebrew, "commit_if_changed", return_value=False) as commit,
                patch.object(cli.homebrew, "push_head") as push,
            ):
                rc = cli.main(
                    [
                        "--cask",
                        str(dest),
                        "--manifest",
                        str(payload),
                        "--commit",
                        "--push",
                        "--skip-tap",
                    ]
                )
            self.assertEqual(rc, 0)
            commit.assert_called_once()
            push.assert_called_once()

    def test_commit_only_the_cask_file(self):
        with tempfile.TemporaryDirectory(prefix="convt-cask-commit-") as tmp:
            repo = Path(tmp)
            dest = repo / "Casks" / "convt.rb"
            dest.parent.mkdir()
            dest.write_text(CASK)
            extra = repo / "NOTES"
            extra.write_text("already staged leftover")
            homebrew.run_git(["init", "-b", "main"], cwd=repo)
            homebrew.run_git(["add", "--", "Casks/convt.rb"], cwd=repo)
            homebrew.run_git(
                [*homebrew.git_ident_args(), "commit", "-m", "seed", "--", "Casks/convt.rb"],
                cwd=repo,
            )
            dest.write_text(homebrew.bump_cask(CASK, version="0.3.0", sha256="d" * 64))
            homebrew.run_git(["add", "--", "NOTES"], cwd=repo)
            self.assertTrue(homebrew.commit_if_changed(repo, dest, "chore(homebrew): bump convt to 0.3.0"))
            committed = homebrew.run_git(
                ["diff-tree", "--no-commit-id", "--name-only", "-r", "HEAD"],
                cwd=repo,
            )
            self.assertEqual(committed, "Casks/convt.rb")
            still_staged = homebrew.run_git(["diff", "--cached", "--name-only"], cwd=repo)
            self.assertEqual(still_staged, "NOTES")
            self.assertIn("already staged leftover", extra.read_text())

    def test_commit_failure_still_publishes_the_tap(self):
        with tempfile.TemporaryDirectory(prefix="convt-cask-tap-") as tmp:
            dest = Path(tmp) / "Casks" / "convt.rb"
            dest.parent.mkdir()
            dest.write_text(CASK)
            payload = Path(tmp) / "release-manifest.json"
            payload.write_text(json.dumps(manifest("0.3.0", "d" * 64)))
            with (
                patch.object(cli.homebrew, "commit_if_changed", side_effect=RuntimeError("hook")),
                patch.dict("os.environ", {"HOMEBREW_TAP_TOKEN": "tap-token"}, clear=False),
                patch.object(cli.homebrew, "publish_tap") as publish,
            ):
                rc = cli.main(
                    ["--cask", str(dest), "--manifest", str(payload), "--commit", "--tap", "opencoredev/homebrew-tap"]
                )
            self.assertEqual(rc, 1)
            publish.assert_called_once()

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
            url = "https://downloads.convt.app/0.4.0/convt-macos-arm64.dmg"
            payload = {
                "distribution_ready": True,
                "builds": [
                    {
                        "version": "0.4.0",
                        "artifacts": [
                            {
                                "platform": "macos-arm64",
                                "kind": "dmg",
                                "url": url,
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
            self.assertIn(url, rendered)
            self.assertIn("livecheck do", rendered)
            self.assertIn("zap trash:", rendered)
            status = json.loads((out / "STATUS.json").read_text())
            self.assertEqual(status["homebrew"], "rendered")


if __name__ == "__main__":
    unittest.main()
