#!/usr/bin/env python3
"""Empty-cache fetch/collect for pinned native sources."""
import hashlib
import http.server
import io
import json
from pathlib import Path
import socketserver
import subprocess
import sys
import tarfile
import tempfile
import threading
import unittest

ROOT = Path(__file__).resolve().parents[2]
TOOLS = ROOT / 'packaging/release/native-source-tools.py'


def sha256(data):
    return hashlib.sha256(data).hexdigest()


def tarball(members):
    buffer = io.BytesIO()
    with tarfile.open(fileobj=buffer, mode='w:gz') as archive:
        for name, data in members.items():
            info = tarfile.TarInfo(name)
            info.size = len(data)
            archive.addfile(info, io.BytesIO(data))
    return buffer.getvalue()


class Handler(http.server.SimpleHTTPRequestHandler):
    def log_message(self, *_args):
        pass


class FetchCollect(unittest.TestCase):
    def serve(self, directory):
        class Bound(Handler):
            def __init__(self, *args, **kwargs):
                super().__init__(*args, directory=str(directory), **kwargs)

        httpd = socketserver.TCPServer(('127.0.0.1', 0), Bound)
        thread = threading.Thread(target=httpd.serve_forever, daemon=True)
        thread.start()
        self.addCleanup(httpd.shutdown)
        self.addCleanup(httpd.server_close)
        return f'http://127.0.0.1:{httpd.server_address[1]}'

    def run_tools(self, command, lock, cache, output=None, extra=()):
        args = [sys.executable, str(TOOLS), command, '--lock', str(lock), '--cache', str(cache)]
        if output is not None:
            args += ['--output', str(output)]
        args += list(extra)
        return subprocess.run(args, check=True, capture_output=True, text=True)

    def fixture(self, directory, payload, digest=None):
        directory.mkdir(parents=True, exist_ok=True)
        (directory / 'demo.tar.gz').write_bytes(payload)
        recipe = sha256(b'spec\n')
        notice = sha256(b'copy\n')
        lock = {
            'schema_version': 1,
            'closure_complete': True,
            'sources': [{
                'name': 'demo.tar.gz',
                'url': self.serve(directory) + '/demo.tar.gz',
                'sha256': digest or sha256(payload),
                'cache': 'native-source/demo.tar.gz',
            }],
            'components': [{
                'name': 'demo',
                'sources': ['demo.tar.gz'],
                'recipes': [{'source': 'demo.tar.gz', 'member': 'demo/recipe.spec',
                             'sha256': recipe, 'retain_as': 'recipe.spec'}],
                'notices': [{'source': 'demo.tar.gz', 'member': 'demo/COPYING',
                             'sha256': notice}],
            }],
        }
        path = directory / 'native-sources.lock.json'
        path.write_text(json.dumps(lock))
        return path

    def test_collect_on_empty_cache_fetches_and_verifies_locked_source(self):
        payload = tarball({'demo/recipe.spec': b'spec\n', 'demo/COPYING': b'copy\n'})
        with tempfile.TemporaryDirectory() as raw:
            work = Path(raw)
            lock = self.fixture(work / 'origin', payload)
            cache = work / 'cache'
            output = work / 'out'
            self.run_tools('collect', lock, cache, output, extra=('--source-only',))
            cached = cache / 'native-source' / 'demo.tar.gz'
            self.assertEqual(sha256(cached.read_bytes()), sha256(payload))
            self.assertEqual((output / 'demo/recipes/recipe.spec').read_bytes(), b'spec\n')
            self.assertEqual((output / 'demo/notices/000-COPYING').read_bytes(), b'copy\n')
            self.assertEqual((output / 'sources/demo.tar.gz').read_bytes(), payload)

    def test_verify_does_not_download_missing_sources(self):
        payload = tarball({'demo/recipe.spec': b'spec\n', 'demo/COPYING': b'copy\n'})
        with tempfile.TemporaryDirectory() as raw:
            work = Path(raw)
            lock = self.fixture(work / 'origin', payload)
            cache = work / 'cache'
            result = subprocess.run(
                [sys.executable, str(TOOLS), 'verify', '--lock', str(lock), '--cache', str(cache)],
                capture_output=True, text=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertFalse((cache / 'native-source' / 'demo.tar.gz').exists())
            self.assertIn('No such file', result.stderr)

    def test_downloaded_hash_mismatch_is_refused(self):
        payload = tarball({'demo/recipe.spec': b'spec\n', 'demo/COPYING': b'copy\n'})
        with tempfile.TemporaryDirectory() as raw:
            work = Path(raw)
            lock = self.fixture(work / 'origin', payload, digest='0' * 64)
            cache = work / 'cache'
            result = subprocess.run(
                [sys.executable, str(TOOLS), 'collect', '--source-only',
                 '--lock', str(lock), '--cache', str(cache), '--output', str(work / 'out')],
                capture_output=True, text=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn('Downloaded source hash mismatch', result.stderr)
            self.assertFalse((cache / 'native-source' / 'demo.tar.gz').exists())
            self.assertFalse((cache / 'native-source' / 'demo.tar.gz.download').exists())


if __name__ == '__main__':
    unittest.main()
