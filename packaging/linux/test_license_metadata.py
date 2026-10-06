"""Fail-closed notice collection regressions using isolated input copies."""
import importlib.util
import json
from pathlib import Path
import shutil
import tempfile
import unittest

HERE = Path(__file__).resolve().parent


class NativeNotices(unittest.TestCase):
    def setUp(self):
        self.work = tempfile.TemporaryDirectory(prefix='convt-pkg-notice-test-')
        self.addCleanup(self.work.cleanup)
        base = Path(self.work.name)
        self.inputs = base / 'inputs'
        self.inputs.mkdir()
        shutil.copyfile(HERE / 'native-notices.lock.json', self.inputs / 'native-notices.lock.json')
        shutil.copytree(HERE / 'license-notices', self.inputs / 'license-notices')
        spec = importlib.util.spec_from_file_location('notice_test', HERE / 'collect-native-notices.py')
        self.collector = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(self.collector)
        self.collector.__file__ = str(self.inputs / 'collect-native-notices.py')
        self.root = base / 'payload'
        (self.root / 'lib').mkdir(parents=True)
        (self.root / 'licenses').mkdir()
        self.library = self.root / 'lib/libxcb.so.1'
        self.library.write_bytes(b'isolated native component')
        self.origins = {'libxcb.so.1': {'rpm': 'libxcb-1.13.1-1.el8.x86_64', 'sha256': 'original-copy-receipt'}}
        self.receipt = self.root / 'licenses/native-origins.json'
        self.save()

    def save(self):
        self.receipt.write_text(json.dumps(self.origins))

    def test_pinned_notice_and_protocol_supplement_are_retained(self):
        self.collector.collect(self.root)
        origin = json.loads(self.receipt.read_text())['libxcb.so.1']
        self.assertEqual(len(origin['notices']), 2)
        for notice in origin['notices']:
            self.assertTrue((self.root / notice).is_file())
        self.assertEqual(origin['sha256'], 'original-copy-receipt')
        self.assertEqual(len(origin['payload_sha256']), 64)

    def test_unmapped_component_is_rejected(self):
        self.origins['libxcb.so.1']['rpm'] = 'unknown-runtime-1'
        self.save()
        with self.assertRaisesRegex(ValueError, 'No audited notice mapping'):
            self.collector.collect(self.root)

    def test_missing_component_is_rejected(self):
        self.library.unlink()
        with self.assertRaisesRegex(ValueError, 'Missing shipped native component'):
            self.collector.collect(self.root)

    def test_missing_or_corrupt_notice_is_rejected(self):
        notice = self.inputs / 'license-notices/libxcb.txt'
        for contents in (b'', b'tampered notice'):
            notice.write_bytes(contents)
            with self.assertRaisesRegex(ValueError, 'Missing or corrupt pinned notice'):
                self.collector.collect(self.root)
        notice.unlink()
        with self.assertRaisesRegex(ValueError, 'Missing or corrupt pinned notice'):
            self.collector.collect(self.root)


if __name__ == '__main__':
    unittest.main()
