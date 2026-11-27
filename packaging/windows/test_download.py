"""Run the build's PowerShell download loop against a loopback HTTP server."""

import hashlib
import http.server
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import threading
import unittest

HERE = Path(__file__).resolve().parent
PAYLOAD = b"exact locked Windows build input"
POWERSHELLS = tuple(dict.fromkeys(
    path for name in ("powershell.exe", "pwsh.exe") if (path := shutil.which(name))
))
PROBE = r"""
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$ProgressPreference = 'SilentlyContinue'
$WarningPreference = 'SilentlyContinue'
[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false)
$tokens = $null; $errors = $null
$ast = [System.Management.Automation.Language.Parser]::ParseFile(
    $env:CONVT_TEST_DOWNLOAD_SOURCE, [ref]$tokens, [ref]$errors)
if ($errors) { throw 'Windows build script contains syntax errors' }
$loops = @($ast.EndBlock.Statements | Where-Object {
    $_ -is [System.Management.Automation.Language.ForEachStatementAst] -and
        $_.Variable.VariablePath.UserPath -eq 'File' -and
        $_.Condition.Extent.Text -eq '$Lock.files'
})
if ($loops.Count -ne 1) { throw 'Expected one build-input download loop' }
$case = $env:CONVT_TEST_DOWNLOAD_CASE | ConvertFrom-Json
$Cache = $case.cache
$Lock = $case.lock
# Execute the production loop unchanged; never run extraction, compilation or MSI actions.
try {
    & ([scriptblock]::Create($loops[0].Extent.Text))
    exit 0
} catch {
    [Console]::Error.WriteLine($_.Exception.Message)
    exit 1
}
"""


@unittest.skipUnless(os.name == "nt" and POWERSHELLS, "requires Windows PowerShell")
class Download(unittest.TestCase):
    def setUp(self):
        work = tempfile.TemporaryDirectory(prefix="convt-windows-download-")
        self.addCleanup(work.cleanup)
        self.root = Path(work.name)
        self.requests = []
        self.invocations = 0
        requests = self.requests

        class Handler(http.server.BaseHTTPRequestHandler):
            def log_message(self, *args):
                pass

            def do_GET(self):
                requests.append(self.path)
                if self.path == "/unavailable":
                    self.send_error(404)
                    return
                if self.path not in ("/wrong", "/valid", "/unused"):
                    self.send_error(404)
                    return
                body = b"wrong archive bytes" if self.path == "/wrong" else PAYLOAD
                self.send_response(200)
                self.send_header("Content-Length", str(len(body)))
                self.end_headers()
                self.wfile.write(body)

        self.server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        self.addCleanup(self.server.server_close)
        self.addCleanup(self.server.shutdown)
        worker = threading.Thread(target=self.server.serve_forever, daemon=True)
        worker.start()
        self.base_url = f"http://127.0.0.1:{self.server.server_port}"

    def download(self, shell, *paths, cached=None):
        self.requests.clear()
        cache = self.root / f"cache-{self.invocations}"
        self.invocations += 1
        cache.mkdir()
        if cached is not None:
            (cache / "source.tar.gz").write_bytes(cached)
        item = {"name": "source.tar.gz", "url": self.base_url + paths[0],
                "sha256": hashlib.sha256(PAYLOAD).hexdigest()}
        if len(paths) > 1:
            item["mirrors"] = [self.base_url + path for path in paths[1:]]
        env = dict(os.environ, no_proxy="*", NO_PROXY="*",
                   CONVT_TEST_DOWNLOAD_SOURCE=str(HERE / "build.ps1"),
                   CONVT_TEST_DOWNLOAD_CASE=json.dumps({"cache": str(cache), "lock": {"files": [item]}}))
        # Each host must discover its own modules, rather than inherit PowerShell 7's.
        env.pop("PSMODULEPATH", None)
        result = subprocess.run([shell, "-NoProfile", "-NonInteractive", "-Command", PROBE],
                                env=env, capture_output=True, text=True, encoding="utf-8", timeout=30)
        return result, cache

    def test_unavailable_primary_and_wrong_digest_mirror_reach_valid_mirror(self):
        for shell in POWERSHELLS:
            with self.subTest(shell=shell):
                result, cache = self.download(shell, "/unavailable", "/wrong", "/valid", "/unused")
                self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
                self.assertEqual(self.requests, ["/unavailable", "/wrong", "/valid"])
                self.assertEqual((cache / "source.tar.gz").read_bytes(), PAYLOAD)
                self.assertEqual(sorted(path.name for path in cache.iterdir()), ["source.tar.gz"])

    def test_exhausted_mirrors_fail_without_caching_wrong_bytes_or_partials(self):
        for shell in POWERSHELLS:
            with self.subTest(shell=shell):
                result, cache = self.download(shell, "/unavailable", "/wrong")
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("Download failed or SHA-256 mismatch", result.stderr)
                self.assertEqual(self.requests, ["/unavailable", "/wrong"])
                self.assertEqual(list(cache.iterdir()), [])

    def test_valid_primary_needs_no_mirrors_property(self):
        for shell in POWERSHELLS:
            with self.subTest(shell=shell):
                result, cache = self.download(shell, "/valid")
                self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
                self.assertEqual(self.requests, ["/valid"])
                self.assertEqual((cache / "source.tar.gz").read_bytes(), PAYLOAD)
                self.assertEqual(sorted(path.name for path in cache.iterdir()), ["source.tar.gz"])

    def test_valid_cache_avoids_all_downloads(self):
        for shell in POWERSHELLS:
            with self.subTest(shell=shell):
                result, cache = self.download(shell, "/unavailable", "/valid", cached=PAYLOAD)
                self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
                self.assertEqual(self.requests, [])
                self.assertEqual((cache / "source.tar.gz").read_bytes(), PAYLOAD)

    def test_corrupt_cache_is_rejected_and_preserved(self):
        for shell in POWERSHELLS:
            with self.subTest(shell=shell):
                result, cache = self.download(shell, "/valid", cached=b"corrupt cached bytes")
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("Hash mismatch", result.stderr)
                self.assertEqual(self.requests, [])
                self.assertEqual((cache / "source.tar.gz").read_bytes(), b"corrupt cached bytes")


if __name__ == "__main__":
    unittest.main()
