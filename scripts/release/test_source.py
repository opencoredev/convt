"""Regression proof that executable-mode changes invalidate a frozen tree."""
import hashlib
import importlib.util
import io
import json
from pathlib import Path
import subprocess
import sys
import tarfile
import tempfile
import unittest

spec=importlib.util.spec_from_file_location('release_source',Path(__file__).with_name('source.py'))
source=importlib.util.module_from_spec(spec)
spec.loader.exec_module(source)

def write_tree(root):
    (root/'src').mkdir(parents=True)
    (root/'src'/'main.rs').write_text('fn main() {}\n')
    (root/'src'/'target.rs').write_text('// source file named target.rs, not Cargo output\n')
    (root/'README').write_text('source\n')
    (root/'target'/'release'/'.fingerprint'/'aho-corasick-0'/'lib-aho_corasick').parent.mkdir(parents=True)
    (root/'target'/'.rustc_info.json').write_text('{}\n')
    (root/'target'/'release'/'.fingerprint'/'aho-corasick-0'/'lib-aho_corasick').write_text('build-a\n')
    (root/'__pycache__').mkdir()
    (root/'__pycache__'/'x.pyc').write_bytes(b'x')
    (root/'third-party'/'native').mkdir(parents=True)
    (root/'third-party'/'native'/'downloaded-source.tar.gz').write_bytes(b'large audit input')

class FreezeModeTests(unittest.TestCase):
    def test_mode_change_rejected(self):
        with tempfile.TemporaryDirectory(prefix='convt-frozen-mode-') as tmp:
            root=Path(tmp); script=root/'build.sh';script.write_text('#!/bin/sh\ntrue\n');script.chmod(0o755)
            (root/'release-tree.json').write_text(json.dumps({'files':{'build.sh':source.sha(script)},'symlinks':{},'modes':{'build.sh':0o755}}))
            source.check(root)
            script.chmod(0o644)
            with self.assertRaisesRegex(ValueError,'modes'):source.check(root)
    def test_cli_failure_prints_traceback_and_script_name(self):
        result=subprocess.run([sys.executable,str(Path(__file__).with_name('source.py')),'check','/no/such/frozen-tree'],
                              capture_output=True,text=True)
        self.assertNotEqual(result.returncode,0)
        self.assertIn('Traceback (most recent call last):',result.stderr)
        self.assertIn('source.py: ',result.stderr)
        self.assertNotEqual(result.stderr.strip(),'too many values to unpack (expected 2)')

class SourceArchiveMembers(unittest.TestCase):
    def test_ci_fingerprint_paths_are_build_output(self):
        # Linux dry run 37607929480: every differing member was under convt-source/target/.
        leaked=[
            'convt-source/target/.rustc_info.json',
            'convt-source/target/release/.fingerprint/aho-corasick-1/lib-aho_corasick(.json)',
            'convt-source/target/release/.fingerprint/cc-1/lib-cc',
            'convt-source/target/release/.fingerprint/const-oid-1/lib-const_oid',
        ]
        self.assertEqual(source.source_archive_build_output_members(leaked,'convt-source'),leaked)
        kept=['convt-source/src/main.rs','convt-source/src/target.rs','convt-source/third-party/rust/foo-1/src/lib.rs']
        self.assertEqual(source.source_archive_build_output_members(kept,'convt-source'),[])

    def test_pack_drops_target_and_pycache_and_keeps_source(self):
        with tempfile.TemporaryDirectory(prefix='convt-source-pack-') as tmp:
            root=Path(tmp)/'convt-source'
            write_tree(root)
            archive=Path(tmp)/'convt-0.2.0-source.tar.gz'
            source.pack_source_archive(root,archive,1791331200)
            names=source.refuse_build_output_in_archive(archive,root.name)
            self.assertIn(f'{root.name}/src/main.rs',names)
            self.assertIn(f'{root.name}/src/target.rs',names)
            self.assertIn(f'{root.name}/README',names)
            self.assertFalse(any(Path(n).parts[:2]==(root.name,'target') for n in names))
            self.assertFalse(any('__pycache__' in Path(n).parts for n in names))
            self.assertFalse(any(Path(n).parts[:2]==(root.name,'third-party') for n in names))

    def test_two_packs_match_when_only_target_differs(self):
        with tempfile.TemporaryDirectory(prefix='convt-source-repro-') as tmp:
            first=Path(tmp)/'a'/'convt-source'
            second=Path(tmp)/'b'/'convt-source'
            write_tree(first)
            write_tree(second)
            (second/'target'/'release'/'.fingerprint'/'aho-corasick-0'/'lib-aho_corasick').write_text('build-b\n')
            (second/'target'/'.rustc_info.json').write_text('{"host":"other"}\n')
            left=Path(tmp)/'left.tar.gz'
            right=Path(tmp)/'right.tar.gz'
            source.pack_source_archive(first,left,1791331200)
            source.pack_source_archive(second,right,1791331200)
            self.assertEqual(hashlib.sha256(left.read_bytes()).hexdigest(),hashlib.sha256(right.read_bytes()).hexdigest())

    def test_guard_names_leaked_target_members(self):
        with tempfile.TemporaryDirectory(prefix='convt-source-guard-') as tmp:
            archive=Path(tmp)/'dirty.tar.gz'
            with tarfile.open(archive,'w:gz') as packed:
                for name,data in [('convt-source/README',b'source\n'),
                                  ('convt-source/target/.rustc_info.json',b'{}\n')]:
                    info=tarfile.TarInfo(name); info.size=len(data)
                    packed.addfile(info,io.BytesIO(data))
            with self.assertRaisesRegex(ValueError,r'source archive contains build output \(1 members\): convt-source/target/\.rustc_info\.json'):
                source.refuse_build_output_in_archive(archive,'convt-source')

if __name__=='__main__':unittest.main()
