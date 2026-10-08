"""AppImage AppRun: desktop app by default, CLI as `convt` or `--cli`."""
import os
import subprocess
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).parent


class AppRun(unittest.TestCase):
    def setUp(self):
        self.tmp = Path(tempfile.mkdtemp(prefix="convt-apprun-"))
        self.addCleanup(self._cleanup)
        apprun = self.tmp / "AppRun"
        apprun.write_text((HERE / "AppRun").read_text())
        apprun.chmod(0o755)
        for name in ("convt", "convt-app"):
            script = self.tmp / name
            script.write_text(
                f'#!/bin/sh\nprintf "%s\\n" "{name}" "$@"\n'
            )
            script.chmod(0o755)

    def _cleanup(self):
        for path in self.tmp.rglob("*"):
            if path.is_file():
                path.unlink()
        self.tmp.rmdir()

    def run_apprun(self, args, argv0=None):
        env = {**os.environ}
        if argv0 is not None:
            env["ARGV0"] = argv0
        return subprocess.run(
            [str(self.tmp / "AppRun"), *args],
            check=True,
            env=env,
            capture_output=True,
            text=True,
            timeout=10,
        ).stdout.split()

    def test_default_is_the_app(self):
        self.assertEqual(self.run_apprun(["open", "--", "a.png"]), ["convt-app", "open", "--", "a.png"])

    def test_cli_flag(self):
        self.assertEqual(self.run_apprun(["--cli", "a.png", "--to", "webp"]), ["convt", "a.png", "--to", "webp"])

    def test_invoked_as_convt(self):
        self.assertEqual(self.run_apprun(["formats"], argv0="/tmp/convt"), ["convt", "formats"])

    def test_app_name_does_not_select_the_cli(self):
        self.assertEqual(self.run_apprun([], argv0="/tmp/Convt.AppImage"), ["convt-app"])
