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
ROOT = HERE.parents[1]
spec = importlib.util.spec_from_file_location("native_source_tools", HERE / "native-source-tools.py")
tools = importlib.util.module_from_spec(spec)
spec.loader.exec_module(tools)


def sha(data):
    return hashlib.sha256(data).hexdigest()


def fixture_lock(evidence, extras=None):
    lock = {
        "sources": [],
        "components": [],
        "closure_complete": True,
        "blockers": [],
        "build": {"runtime_cache": "appimage-source/rebuilt/runtime-x86_64",
                  "runtime_sha256": "ab" * 32},
        "linked_archives": ["libz.a", "libzstd.a"],
        "startup_objects": ["rcrt1.o"],
        "compression_closure": {
            "enabled": ["zlib", "zstd"],
            "excluded": ["xz/liblzma", "lzo", "lz4"],
        },
        "build_evidence": evidence,
    }
    if extras:
        lock.update(extras)
    return lock


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
        self.assertEqual(lock["build"]["runtime_cache"], "appimage-source/rebuilt/runtime-x86_64")
        for entry in lock["build_evidence"]:
            self.assertTrue(entry["cache"].startswith(prefix), entry["cache"])
            self.assertNotIn("url", entry)
            self.assertIn("/output/" + Path(entry["cache"]).name, script)
            if "sha256" not in entry:
                self.assertIn(Path(entry["cache"]).name, tools.SEMANTIC_EVIDENCE)

    def test_appimage_compression_closure_is_zlib_zstd_only(self):
        lock = json.loads((HERE / "appimage-source-closure.lock.json").read_text())
        enabled, excluded = tools.locked_squashfuse_codecs(lock)
        self.assertEqual(enabled, ("ZLIB", "ZSTD"))
        self.assertEqual(set(excluded), {"XZ", "LZO", "LZ4"})

    def test_appimage_sources_are_url_pinned(self):
        lock = json.loads((HERE / "appimage-source-closure.lock.json").read_text())
        self.assertTrue(all(s.get("url") and s.get("sha256") for s in lock["sources"]))
        for check in lock.get("binary_checks", []):
            self.assertTrue(check.get("sha256"), check)
            self.assertTrue(str(check.get("cache", "")).startswith("appimage-source/rebuilt/"))

    def test_local_recipe_pins_match_disk(self):
        lock = json.loads((HERE / "appimage-source-closure.lock.json").read_text())
        for component in lock["components"]:
            for ref in component.get("recipes", []):
                if "local" not in ref:
                    continue
                path = ROOT / ref["local"]
                self.assertEqual(sha(path.read_bytes()), ref["sha256"], ref["local"])

    def test_native_lock_has_no_warm_cache_evidence(self):
        lock = json.loads((HERE / "native-sources.lock.json").read_text())
        self.assertFalse(lock.get("build_evidence"))
        self.assertFalse(lock.get("binary_checks"))
        self.assertTrue(all(s.get("url") and s.get("sha256") for s in lock["sources"]))


class ReleaseCacheInputs(unittest.TestCase):
    """Every packaging/.cache path collect / source-audit / rebuild-cli reads."""

    def test_url_backed_locks_used_after_collect_all_have_sha256(self):
        files = [
            ROOT / "packaging/linux/ffmpeg-source-inputs.lock.json",
            ROOT / "packaging/linux/inputs.lock.json",
            ROOT / "packaging/linux/build-rpms.lock.json",
            ROOT / "packaging/linux/appimage-inputs.lock.json",
        ]
        for path in files:
            items = json.loads(path.read_text())
            for item in items:
                self.assertTrue(item.get("sha256"), item)
                if item.get("built_by"):
                    self.assertEqual(item["built_by"], "packaging/release/appimage-source-build.py")
                    self.assertNotIn("url", item)
                else:
                    self.assertTrue(item.get("url"), item)

    def test_pdfium_and_macos_ffmpeg_artifacts_are_url_pinned(self):
        spec = importlib.util.spec_from_file_location(
            "pdfium_source_verify", HERE / "pdfium-source-verify.py")
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        for name in ("pdfium-source.lock.json", "macos-source-ffmpeg.lock.json"):
            lock = json.loads((HERE / name).read_text())
            artifacts = list(module.artifacts(lock))
            self.assertTrue(artifacts, name)
            for item in artifacts:
                self.assertTrue(item.get("url") and item.get("sha256") and item.get("cache_filename"), item)
        build = json.loads((HERE / "macos-source-ffmpeg.lock.json").read_text())["source_build_alternative"]
        for item in build["sources"]:
            self.assertTrue(item.get("url") and item.get("sha256") and item.get("cache_filename"), item)


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
            runtime = b"runtime-bytes"
            (rebuilt / "runtime-x86_64").write_bytes(runtime)
            files = {
                "installed-apks.txt": b"apk-a\napk-b\n",
                "clang-version.txt": b"clang 19\n",
                "link-inputs.txt": b"libz.a\nlibzstd.a\nrcrt1.o\n",
                "squashfuse-config.log": b"sq_decompressors=ZLIB ZSTD\n",
                "runtime-version.txt": b"AppImage runtime version: pinned\n",
            }
            evidence = []
            for name, data in files.items():
                (rebuilt / name).write_bytes(data)
                entry = {"cache": f"appimage-source/provenance-abababababab/{name}"}
                if name in {"installed-apks.txt", "clang-version.txt"}:
                    entry["sha256"] = sha(data)
                else:
                    entry["reproducible"] = False
                evidence.append(entry)
            lock = fixture_lock(evidence, {"build": {
                "runtime_cache": "appimage-source/rebuilt/runtime-x86_64",
                "runtime_sha256": sha(runtime),
            }, "binary_checks": [{
                "cache": "appimage-source/rebuilt/runtime-x86_64",
                "sha256": sha(runtime),
            }]})
            out = self.collect(cache, lock)
            for entry in evidence:
                locked = cache / entry["cache"]
                self.assertTrue(locked.is_file(), entry["cache"])
                self.assertEqual((out / "build-evidence" / Path(entry["cache"]).name).read_bytes(),
                                 locked.read_bytes())
                if "sha256" in entry:
                    self.assertEqual(sha(locked.read_bytes()), entry["sha256"])

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

    def test_unpinned_log_still_requires_locked_contents(self):
        with tempfile.TemporaryDirectory() as d:
            cache = Path(d) / "cache"
            cache.mkdir()
            rebuilt = cache / "appimage-source/rebuilt"
            rebuilt.mkdir(parents=True)
            (rebuilt / "link-inputs.txt").write_bytes(b"no archives here\n")
            evidence = [{"cache": "appimage-source/provenance-abababababab/link-inputs.txt",
                         "reproducible": False}]
            with self.assertRaisesRegex(ValueError, "Linker trace missing locked inputs"):
                self.collect(cache, fixture_lock(evidence))

    def test_configure_log_accepts_autoconf_quoted_assignment(self):
        entry = {"cache": "appimage-source/provenance-x/squashfuse-config.log",
                 "reproducible": False}
        # Alpine autoconf 2.72 writes the substituted value with quotes and a
        # leading space (SQ_CHECK_DECOMPRESS does sq_decompressors="$sq_decompressors $1").
        log = b"\n".join([
            b"## ----------------- ##",
            b"## Output variables. ##",
            b"## ----------------- ##",
            b"",
            b"sq_decompressors=' ZLIB ZSTD'",
            b"ac_cv_search_uncompress='-lz'",
            b"ac_cv_search_ZSTD_decompress='-lzstd'",
            b"ac_cv_search_lzma_stream_buffer_decode=no",
            b"ac_cv_search_lzo1x_decompress_safe=no",
            b"ac_cv_search_LZ4_decompress_safe=no",
            b"",
        ])
        self.assertNotIn(b"sq_decompressors=ZLIB ZSTD", log)
        tools.verify_evidence_contents(entry, log, fixture_lock([entry]))

    def test_configure_log_accepts_probe_cache_without_assignment(self):
        entry = {"cache": "appimage-source/provenance-x/squashfuse-config.log",
                 "reproducible": False}
        log = b"\n".join([
            b"ac_cv_search_uncompress='-lz'",
            b"ac_cv_search_ZSTD_decompress='-lzstd'",
            b"ac_cv_search_lzma_stream_buffer_decode=no",
            b"ac_cv_search_lzo1x_decompress_safe=no",
            b"ac_cv_search_LZ4_decompress_safe=no",
            b"",
        ])
        tools.verify_evidence_contents(entry, log, fixture_lock([entry]))

    def test_configure_log_missing_list_is_refused(self):
        entry = {"cache": "appimage-source/provenance-x/squashfuse-config.log",
                 "reproducible": False}
        with self.assertRaisesRegex(ValueError, "Configure log missing locked decompressor list ZLIB ZSTD"):
            tools.verify_evidence_contents(entry, b"checking for gcc... gcc\n", fixture_lock([entry]))

    def test_configure_log_extra_decompressor_is_refused(self):
        entry = {"cache": "appimage-source/provenance-x/squashfuse-config.log",
                 "reproducible": False}
        with self.assertRaisesRegex(ValueError, r"decompressor list .* != locked"):
            tools.verify_evidence_contents(
                entry, b"sq_decompressors=' ZLIB XZ ZSTD'\n", fixture_lock([entry]))

    def test_configure_log_excluded_probe_is_refused(self):
        entry = {"cache": "appimage-source/provenance-x/squashfuse-config.log",
                 "reproducible": False}
        log = b"\n".join([
            b"sq_decompressors=' ZLIB ZSTD'",
            b"ac_cv_search_uncompress='-lz'",
            b"ac_cv_search_ZSTD_decompress='-lzstd'",
            b"ac_cv_search_lzma_stream_buffer_decode='-llzma'",
            b"ac_cv_search_lzo1x_decompress_safe=no",
            b"ac_cv_search_LZ4_decompress_safe=no",
            b"",
        ])
        with self.assertRaisesRegex(ValueError, "enabled excluded decompressors"):
            tools.verify_evidence_contents(entry, log, fixture_lock([entry]))

    def test_unknown_unpinned_evidence_is_refused(self):
        entry = {"cache": "appimage-source/provenance-x/mystery.log", "reproducible": False}
        with self.assertRaisesRegex(ValueError, "Unpinned build evidence has no semantic check"):
            tools.verify_evidence_contents(entry, b"??\n", fixture_lock([entry]))

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

    def test_runtime_file_is_copied_from_rebuilt_runtime(self):
        with tempfile.TemporaryDirectory() as d:
            cache = Path(d) / "cache"
            cache.mkdir()
            rebuilt = cache / "appimage-source/rebuilt"
            rebuilt.mkdir(parents=True)
            runtime = b"source-built-runtime"
            (rebuilt / "runtime-x86_64").write_bytes(runtime)
            wrapper = cache / "runtime-source-built-x86_64"
            lock = fixture_lock([], {"build": {
                "runtime_cache": "appimage-source/rebuilt/runtime-x86_64",
                "runtime_sha256": sha(runtime),
            }, "binary_checks": [{
                "cache": "appimage-source/rebuilt/runtime-x86_64",
                "sha256": sha(runtime),
            }]})
            self.collect(cache, lock, extra=["--runtime-file", str(wrapper)])
            self.assertEqual(wrapper.read_bytes(), runtime)

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
