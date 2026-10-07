"""AppImage lock pins must stay on immutable URLs and match fetched bytes."""
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

HERE = Path(__file__).resolve().parent
FETCH = HERE / 'fetch.py'
LOCK = HERE / 'appimage-inputs.lock.json'


def fetch(*args, check=False):
    return subprocess.run(
        [sys.executable, str(FETCH), *args],
        capture_output=True,
        text=True,
        check=check,
    )


class AppImageInputs(unittest.TestCase):
    def setUp(self):
        self.lock = json.loads(LOCK.read_text())
        self.tool = next(item for item in self.lock if item['name'] == 'appimagetool.AppImage')

    def test_appimagetool_uses_versioned_release_url(self):
        url = self.tool['url']
        self.assertTrue(url.startswith('https://github.com/AppImage/appimagetool/releases/download/'))
        self.assertNotIn('/download/continuous/', url)
        self.assertIn(f'/download/{self.tool["version"]}/', url)
        self.assertEqual(len(self.tool['sha256']), 64)

    def test_downloaded_only_skips_source_built_inputs(self):
        with tempfile.TemporaryDirectory(prefix='convt-appimage-pin-') as tmp:
            cache = Path(tmp) / 'cache'
            payload = Path(tmp) / 'tool.bin'
            payload.write_bytes(b'pinned-appimagetool')
            digest = hashlib.sha256(payload.read_bytes()).hexdigest()
            lock = Path(tmp) / 'lock.json'
            lock.write_text(json.dumps([
                {
                    'name': 'tool.bin',
                    'url': payload.resolve().as_uri(),
                    'sha256': digest,
                    'version': '1.0',
                },
                {
                    'name': 'runtime-source-built-x86_64',
                    'sha256': '0' * 64,
                    'version': 'deadbeef',
                    'built_by': 'packaging/release/appimage-source-build.py',
                },
            ]))
            result = fetch('--downloaded-only', str(cache), str(lock))
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertIn('verified tool.bin', result.stdout)
            self.assertNotIn('runtime-source-built-x86_64', result.stdout)
            self.assertFalse((cache / 'runtime-source-built-x86_64').exists())

    def test_hash_mismatch_names_expected_and_actual(self):
        with tempfile.TemporaryDirectory(prefix='convt-appimage-pin-') as tmp:
            cache = Path(tmp) / 'cache'
            payload = Path(tmp) / 'tool.bin'
            payload.write_bytes(b'wrong-bytes')
            lock = Path(tmp) / 'lock.json'
            lock.write_text(json.dumps([{
                'name': 'tool.bin',
                'url': payload.resolve().as_uri(),
                'sha256': 'a' * 64,
            }]))
            result = fetch('--downloaded-only', str(cache), str(lock))
            self.assertEqual(result.returncode, 1)
            self.assertIn('SHA-256 mismatch:', result.stderr + result.stdout)
            self.assertIn('expected ' + 'a' * 64, result.stderr + result.stdout)
            self.assertIn('got      ' + hashlib.sha256(b'wrong-bytes').hexdigest(), result.stderr + result.stdout)


if __name__ == '__main__':
    unittest.main()
