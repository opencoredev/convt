#!/usr/bin/env python3
"""Source canonicalization must refuse archive members that would leave its work directory."""
import importlib.util
import io
from pathlib import Path
import tarfile
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('pdfium_source_verify', ROOT / 'packaging/release/pdfium-source-verify.py')
verify = importlib.util.module_from_spec(spec)
spec.loader.exec_module(verify)


def archive(path, *members):
    with tarfile.open(path, 'w:gz') as result:
        for member in members:
            if isinstance(member, tarfile.TarInfo):
                result.addfile(member)
                continue
            name, data = member
            info = tarfile.TarInfo(name)
            info.size = len(data)
            result.addfile(info, io.BytesIO(data))


class Canonicalize(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.base = Path(temporary.name)
        self.cache = self.base / 'cache'
        self.cache.mkdir()
        self.outside = self.base / 'outside'
        self.outside.mkdir()
        original = verify.CACHE
        verify.CACHE = self.cache
        self.addCleanup(setattr, verify, 'CACHE', original)

    def files(self):
        return sorted(str(path.relative_to(self.base)) for path in self.base.rglob('*') if path.is_file())

    def assert_refused(self, *members):
        source = self.base / 'source.tar.gz'
        archive(source, ('pkg/README', b'ok\n'), *members)
        before = self.files()
        for sort_members in (True, False):
            output = self.base / 'canonical.tar.gz'
            with self.subTest(sort_members=sort_members):
                with self.assertRaises(ValueError):
                    verify.canonicalize(source, output, sort_members=sort_members)
                output.unlink(missing_ok=True)
                self.assertEqual(self.files(), before)
                self.assertEqual(list(self.outside.iterdir()), [])

    def test_refuses_parent_directory_member(self):
        for name in ('../outside/escaped', 'pkg/../../outside/escaped'):
            with self.subTest(name=name):
                self.assert_refused((name, b'x'))

    def test_refuses_absolute_member(self):
        self.assert_refused((str(self.outside / 'absolute'), b'x'))

    def test_refuses_device_member(self):
        device = tarfile.TarInfo('pkg/null')
        device.type = tarfile.CHRTYPE
        self.assert_refused(device)

    def test_canonicalizes_ordinary_archive(self):
        source = self.base / 'source.tar.gz'
        link = tarfile.TarInfo('pkg/link')
        link.type = tarfile.SYMTYPE
        link.linkname = 'README'
        archive(source, ('pkg/b', b'b'), ('pkg/README', b'ok\n'), link)
        for sort_members in (True, False):
            output = self.base / f'canonical-{sort_members}.tar.gz'
            verify.canonicalize(source, output, sort_members=sort_members)
            with tarfile.open(output) as result:
                names = result.getnames()
                self.assertEqual(result.extractfile('pkg/README').read(), b'ok\n')
            expected = ['pkg/README', 'pkg/b', 'pkg/link']
            self.assertEqual(names, expected if sort_members else ['pkg/b', 'pkg/README', 'pkg/link'])
        self.assertEqual(list(self.cache.iterdir()), [])


if __name__ == '__main__':
    unittest.main()
