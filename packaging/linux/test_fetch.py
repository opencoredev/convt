"""fetch.py mirror fallback and digest checks against a local server only."""
import functools
import hashlib
import http.server
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import threading
import unittest

HERE = Path(__file__).resolve().parent
PAYLOAD = b'locked source archive'


class QuietHandler(http.server.SimpleHTTPRequestHandler):
    def log_message(self, *args):
        pass

    def do_GET(self):
        if self.path == '/truncated.tar.gz':
            # Promise more than is sent, then drop the connection.
            self.send_response(200)
            self.send_header('Content-Length', str(len(PAYLOAD) + 64))
            self.end_headers()
            self.wfile.write(PAYLOAD[:5])
            self.wfile.flush()
            self.close_connection = True
            return
        if self.path == '/error-page.tar.gz':
            # Some hosts answer a missing file with 200 and an HTML page.
            body = b'<html>Not found</html>'
            self.send_response(200)
            self.send_header('Content-Length', str(len(body)))
            self.end_headers()
            self.wfile.write(body)
            return
        super().do_GET()


class Fetch(unittest.TestCase):
    def setUp(self):
        self.work = tempfile.TemporaryDirectory(prefix='convt-fetch-test-')
        self.addCleanup(self.work.cleanup)
        base = Path(self.work.name)
        self.served = base / 'served'
        self.served.mkdir()
        self.cache = base / 'cache'
        self.lock = base / 'inputs.lock.json'
        handler = functools.partial(QuietHandler, directory=str(self.served))
        self.server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), handler)
        threading.Thread(target=self.server.serve_forever, daemon=True).start()
        self.addCleanup(self.server.server_close)
        self.addCleanup(self.server.shutdown)
        self.base_url = f'http://127.0.0.1:{self.server.server_address[1]}'

    def fetch(self, item):
        self.lock.write_text(json.dumps([item]))
        # Keep requests on the loopback server even where a proxy is configured.
        env = dict(os.environ, no_proxy='*', NO_PROXY='*')
        return subprocess.run([sys.executable, str(HERE / 'fetch.py'), str(self.cache), str(self.lock)],
                              capture_output=True, text=True, env=env, timeout=60)

    def item(self, *paths, sha256=hashlib.sha256(PAYLOAD).hexdigest()):
        urls = [self.base_url + path for path in paths]
        return {'name': 'source.orig.tar.gz', 'url': urls[0], 'mirrors': urls[1:], 'sha256': sha256}

    def test_missing_pool_file_falls_back_to_mirror(self):
        (self.served / 'mirror.tar.gz').write_bytes(PAYLOAD)
        result = self.fetch(self.item('/pool/gone.tar.gz', '/mirror.tar.gz'))
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('404', result.stderr)
        self.assertEqual((self.cache / 'source.orig.tar.gz').read_bytes(), PAYLOAD)

    def test_digest_mismatch_is_rejected_and_not_cached(self):
        (self.served / 'pool.tar.gz').write_bytes(b'tampered')
        (self.served / 'mirror.tar.gz').write_bytes(b'also tampered')
        result = self.fetch(self.item('/pool.tar.gz', '/mirror.tar.gz'))
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('SHA-256 mismatch', result.stderr)
        self.assertEqual(list(self.cache.iterdir()), [])

    def test_cut_connection_falls_back_and_leaves_no_partial(self):
        (self.served / 'mirror.tar.gz').write_bytes(PAYLOAD)
        result = self.fetch(self.item('/truncated.tar.gz', '/mirror.tar.gz'))
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('truncated.tar.gz', result.stderr)
        self.assertEqual(sorted(p.name for p in self.cache.iterdir()), ['source.orig.tar.gz'])

    def test_failed_downloads_leave_no_partial_in_the_cache(self):
        result = self.fetch(self.item('/truncated.tar.gz', '/gone.tar.gz'))
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(list(self.cache.iterdir()), [])

    def test_source_verifier_tries_mirrors_after_a_200_error_page(self):
        spec = importlib.util.spec_from_file_location(
            'pdfium_source_verify', HERE.parent / 'release' / 'pdfium-source-verify.py')
        verify = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(verify)
        (self.served / 'mirror.tar.gz').write_bytes(PAYLOAD)
        self.cache.mkdir()
        verify.CACHE = self.cache
        item = self.item('/error-page.tar.gz', '/mirror.tar.gz')
        item['cache_filename'] = item['name']
        target = self.cache / 'sources' / 'source.orig.tar.gz'
        old = os.environ.get('no_proxy')
        os.environ['no_proxy'] = '*'
        try:
            verify.fetch(item, target)
        finally:
            os.environ.pop('no_proxy') if old is None else os.environ.__setitem__('no_proxy', old)
        self.assertEqual(target.read_bytes(), PAYLOAD)

    def test_cached_file_with_wrong_digest_is_rejected(self):
        (self.served / 'pool.tar.gz').write_bytes(PAYLOAD)
        self.cache.mkdir()
        (self.cache / 'source.orig.tar.gz').write_bytes(b'stale')
        result = self.fetch(self.item('/pool.tar.gz'))
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('SHA-256 mismatch', result.stderr)

    def test_locked_mirrors_are_https_and_keep_the_pool_first(self):
        for name in ('inputs.lock.json', 'ffmpeg-source-inputs.lock.json', 'package-tools-deb.lock.json'):
            for item in json.loads((HERE / name).read_text()):
                mirrors = item.get('mirrors', [])
                self.assertTrue(all(url.startswith('https://') for url in mirrors), item['name'])
                self.assertNotIn(item.get('url'), mirrors, item['name'])
                if '/ubuntu/pool/' in item.get('url', ''):
                    self.assertTrue(mirrors, item['name'])


if __name__ == '__main__':
    unittest.main()
