#!/usr/bin/env python3
"""Fresh-cache collect must populate every locked AppImage provenance input."""
import hashlib
import importlib.util
import io
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest import mock

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("native_source_tools", HERE / "native-source-tools.py")
tools = importlib.util.module_from_spec(spec)
spec.loader.exec_module(tools)


def sha(data):
    return hashlib.sha256(data).hexdigest()


def fixture_lock(evidence):
    return {
        "sources": [],
        "components": [],
        "closure_complete": True,
        "blockers": [],
        "build": {"runtime_cache": "appimage-source/rebuilt/runtime-x86_64",
                  "runtime_sha256": "ab" * 32},
        "build_evidence": evidence,
    }


def run_main(lock_path, args):
    old = sys.argv
    sys.argv = ["native-source-tools.py", *args]
    try:
        with mock.patch("sys.stdout", io.StringIO()):
            tools.main(lock_path)
    finally:
        sys.argv = old


class LockAudit(unittest.TestCase):
    def test_appimage_evidence_is_rebuild_output_under_runtime_hash(self):
        lock = json.loads((HERE / "appimage-source-closure.lock.json").read_text())
        script = (HERE / "appimage-source-build.sh").read_text()
        prefix = "appimage-source/provenance-" + lock["build"]["runtime_sha256"][:12] + "/"
        self.assertTrue(lock["build_evidence"])
        for entry in lock["build_evidence"]:
            self.assertTrue(entry["cache"].startswith(prefix), entry["cache"])
            self.assertNotIn("url", entry)
            self.assertIn("/output/" + Path(entry["cache"]).name, script)
            if entry.get("reproducible") is False:
                self.assertNotIn("sha256", entry)
            else:
                self.assertTrue(entry.get("sha256"))
        recipe = next(r for c in lock["components"] for r in c["recipes"]
                      if r.get("local") == "packaging/release/appimage-source-build.py")
        self.assertEqual(recipe["sha256"], sha((HERE / "appimage-source-build.py").read_bytes()))

    def test_native_lock_has_no_warm_cache_evidence(self):
        lock = json.loads((HERE / "native-sources.lock.json").read_text())
        self.assertFalse(lock.get("build_evidence"))
        self.assertFalse(lock.get("binary_checks"))
        self.assertTrue(all(s.get("url") and s.get("sha256") for s in lock["sources"]))


class CollectInputs(unittest.TestCase):
    def collect(self, cache, lock_data, extra=None):
        lock_path = cache.parent / "lock.json"
        lock_path.write_text(json.dumps(lock_data))
        out = cache.parent / "out"
        args = ["collect", "--source-only", "--lock", str(lock_path),
                "--cache", str(cache), "--output", str(out)]
        if extra:
            args.extend(extra)
        run_main(lock_path, args)
        return out

    def test_collect_copies_every_rebuild_product_into_locked_provenance(self):
        with tempfile.TemporaryDirectory() as d:
            cache = Path(d) / "cache"
            cache.mkdir()
            rebuilt = cache / "appimage-source/rebuilt"
            rebuilt.mkdir(parents=True)
            files = {"installed-apks.txt": b"apk-a\napk-b\n",
                     "link-inputs.txt": b"libz.a\n",
                     "clang-version.txt": b"clang 19\n"}
            evidence = []
            for name, data in files.items():
                (rebuilt / name).write_bytes(data)
                item = {"cache": f"appimage-source/provenance-abababababab/{name}"}
                if name == "link-inputs.txt":
                    item["reproducible"] = False
                else:
                    item["sha256"] = sha(data)
                evidence.append(item)
            out = self.collect(cache, fixture_lock(evidence))
            for entry in evidence:
                locked = cache / entry["cache"]
                produced = (rebuilt / Path(entry["cache"]).name).read_bytes()
                self.assertEqual(locked.read_bytes(), produced)
                if "sha256" in entry:
                    self.assertEqual(sha(locked.read_bytes()), entry["sha256"])
                self.assertEqual((out / "build-evidence" / Path(entry["cache"]).name).read_bytes(),
                                 produced)

    def test_rebuild_product_hash_mismatch_is_refused(self):
        with tempfile.TemporaryDirectory() as d:
            cache = Path(d) / "cache"
            cache.mkdir()
            rebuilt = cache / "appimage-source/rebuilt"
            rebuilt.mkdir(parents=True)
            (rebuilt / "installed-apks.txt").write_bytes(b"wrong rebuild bytes\n")
            evidence = [{"cache": "appimage-source/provenance-abababababab/installed-apks.txt",
                         "sha256": sha(b"locked provenance\n")}]
            with self.assertRaisesRegex(ValueError, "Rebuild product hash mismatch"):
                self.collect(cache, fixture_lock(evidence))

    def test_fetch_url_populates_missing_evidence_with_sha256(self):
        with tempfile.TemporaryDirectory() as d:
            cache = Path(d) / "cache"
            cache.mkdir()
            source = cache / "remote" / "installed-apks.txt"
            source.parent.mkdir(parents=True)
            payload = b"fetched provenance\n"
            source.write_bytes(payload)
            evidence = [{"cache": "appimage-source/provenance-abababababab/installed-apks.txt",
                         "sha256": sha(payload), "url": source.resolve().as_uri()}]
            self.collect(cache, fixture_lock(evidence))
            locked = cache / evidence[0]["cache"]
            self.assertEqual(locked.read_bytes(), payload)

    def test_fetch_command_still_skips_unbuilt_provenance(self):
        with tempfile.TemporaryDirectory() as d:
            tree = Path(d)
            cache = tree / "cache"
            cache.mkdir()
            lock_path = tree / "lock.json"
            lock_path.write_text(json.dumps(fixture_lock([
                {"cache": "appimage-source/provenance-abababababab/installed-apks.txt",
                 "sha256": sha(b"not yet built\n")}])))
            run_main(lock_path, ["fetch", "--lock", str(lock_path), "--cache", str(cache)])
            self.assertFalse((cache / "appimage-source/provenance-abababababab/installed-apks.txt").exists())

    def test_collect_without_rebuild_or_url_names_the_missing_input(self):
        with tempfile.TemporaryDirectory() as d:
            cache = Path(d) / "cache"
            cache.mkdir()
            evidence = [{"cache": "appimage-source/provenance-abababababab/installed-apks.txt",
                         "sha256": sha(b"missing\n")}]
            with self.assertRaisesRegex(ValueError, "Missing locked cache input"):
                self.collect(cache, fixture_lock(evidence))

    def test_unpinned_rebuild_log_is_copied_without_digest(self):
        with tempfile.TemporaryDirectory() as d:
            cache = Path(d) / "cache"
            cache.mkdir()
            rebuilt = cache / "appimage-source/rebuilt"
            rebuilt.mkdir(parents=True)
            (rebuilt / "link-inputs.txt").write_bytes(b"volatile linker trace\n")
            evidence = [{"cache": "appimage-source/provenance-abababababab/link-inputs.txt",
                         "reproducible": False}]
            out = self.collect(cache, fixture_lock(evidence))
            self.assertEqual((cache / evidence[0]["cache"]).read_bytes(), b"volatile linker trace\n")
            self.assertEqual((out / "build-evidence/link-inputs.txt").read_bytes(),
                             b"volatile linker trace\n")

    def test_promote_writes_locked_paths_from_rebuild_output(self):
        with tempfile.TemporaryDirectory() as d:
            cache = Path(d)
            output = cache / "appimage-source/rebuilt"
            output.mkdir(parents=True)
            data = b"apk list\n"
            (output / "installed-apks.txt").write_bytes(data)
            lock = fixture_lock([{"cache": "appimage-source/provenance-abababababab/installed-apks.txt",
                                  "sha256": sha(data)}])
            tools.promote_build_evidence(lock, cache, output)
            self.assertEqual((cache / lock["build_evidence"][0]["cache"]).read_bytes(), data)

    def test_promote_copies_unpinned_logs_and_still_checks_pinned_ones(self):
        with tempfile.TemporaryDirectory() as d:
            cache = Path(d)
            output = cache / "appimage-source/rebuilt"
            output.mkdir(parents=True)
            (output / "installed-apks.txt").write_bytes(b"apk list\n")
            (output / "link-inputs.txt").write_bytes(b"trace\n")
            lock = fixture_lock([
                {"cache": "appimage-source/provenance-abababababab/installed-apks.txt",
                 "sha256": sha(b"apk list\n")},
                {"cache": "appimage-source/provenance-abababababab/link-inputs.txt",
                 "reproducible": False},
            ])
            tools.promote_build_evidence(lock, cache, output)
            self.assertEqual((cache / lock["build_evidence"][0]["cache"]).read_bytes(), b"apk list\n")
            self.assertEqual((cache / lock["build_evidence"][1]["cache"]).read_bytes(), b"trace\n")

    def test_downloaded_source_hash_mismatch_is_refused(self):
        with tempfile.TemporaryDirectory() as d:
            cache = Path(d)
            remote = cache / "remote.bin"
            remote.write_bytes(b"tampered")
            path = cache / "appimage-source/src.bin"
            with self.assertRaisesRegex(ValueError, "Downloaded source hash mismatch"):
                tools.fetch_verified(path, remote.resolve().as_uri(), sha(b"expected"), "src.bin")
            self.assertFalse(path.exists())


class PromoteEscapes(unittest.TestCase):
    def test_lock_path_cannot_escape_cache(self):
        with tempfile.TemporaryDirectory() as d:
            cache = Path(d)
            with self.assertRaises(ValueError):
                tools.cache_file(cache, "../outside")


if __name__ == "__main__":
    unittest.main()
