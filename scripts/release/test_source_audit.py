import hashlib
import importlib.util
from pathlib import Path
import tempfile
import unittest

spec=importlib.util.spec_from_file_location('source_audit',Path(__file__).with_name('source-audit.py'))
audit=importlib.util.module_from_spec(spec);spec.loader.exec_module(audit)

class Sources(unittest.TestCase):
 def test_locked_source_is_retained_and_corruption_refused(self):
  with tempfile.TemporaryDirectory() as d:
   tree=Path(d);cache=tree/'cache';cache.mkdir();source=cache/'source.tar';source.write_bytes(b'exact pinned corresponding source')
   item={'name':'source.tar','url':'https://example.com/source.tar','sha256':hashlib.sha256(source.read_bytes()).hexdigest()}
   record=audit.retain(tree,cache,tree/'sources',item)
   self.assertEqual((tree/record['path']).read_bytes(),source.read_bytes())
   source.write_bytes(b'version-adjacent source')
   with self.assertRaises(ValueError):audit.retain(tree,cache,tree/'sources',item)
 def test_source_path_cannot_escape_cache(self):
  with tempfile.TemporaryDirectory() as d:
   tree=Path(d)
   with self.assertRaises(ValueError):audit.retain(tree,tree,tree/'sources',{'name':'../outside','sha256':'0'*64})
 def test_missing_url_backed_source_is_fetched_and_hashed(self):
  with tempfile.TemporaryDirectory() as d:
   tree=Path(d);cache=tree/'cache';cache.mkdir()
   remote=cache/'remote';remote.mkdir();payload=b'fetched corresponding source'
   (remote/'source.tar').write_bytes(payload)
   item={'name':'source.tar','url':(remote/'source.tar').resolve().as_uri(),
         'sha256':hashlib.sha256(payload).hexdigest()}
   record=audit.retain(tree,cache,tree/'sources',item)
   self.assertEqual((cache/'source.tar').read_bytes(),payload)
   self.assertEqual((tree/record['path']).read_bytes(),payload)
 def test_fetched_source_hash_mismatch_is_refused(self):
  with tempfile.TemporaryDirectory() as d:
   tree=Path(d);cache=tree/'cache';cache.mkdir()
   remote=cache/'remote';remote.mkdir();(remote/'source.tar').write_bytes(b'tampered')
   item={'name':'source.tar','url':(remote/'source.tar').resolve().as_uri(),
         'sha256':hashlib.sha256(b'expected').hexdigest()}
   with self.assertRaisesRegex(ValueError,'Downloaded source hash mismatch'):
    audit.retain(tree,cache,tree/'sources',item)
   self.assertFalse((cache/'source.tar').exists())

class CrateGraphLines(unittest.TestCase):
    def identities(self, *lines):
        return audit.crate_graph_identities('\n'.join(lines) + '\n')

    def test_colorized_duplicate_marker_is_not_a_third_field(self):
        # GitHub Actions sets CARGO_TERM_COLOR=always. Cargo 1.95 then prints a
        # colorized (*) that is not prefixed by " (", which the previous
        # 2-tuple split treated as a third field.
        colored = 'objc2-app-kit v0.3.2 \x1b[33m\x1b[2m(*)\x1b[39m\x1b[22m'
        self.assertEqual(len(colored.split(' (')[0].replace(' v', ' ', 1).split()), 3)
        self.assertEqual(self.identities(colored), {('objc2-app-kit', '0.3.2')})
        name, version = next(iter(self.identities(colored)))
        self.assertEqual((name, version), ('objc2-app-kit', '0.3.2'))

    def test_plain_and_annotated_package_lines(self):
        self.assertEqual(self.identities(
            'serde v1.0.210',
            'serde v1.0.210 (*)',
            'quote v1.0.40 (proc-macro)',
            'convt-app v0.2.0 (/tmp/convt-source/crates/convt-app)',
            'spirv v0.4.0+sdk-1.4.341.0',
            '',
            '[build-dependencies]',
            'objc2-core-audio-types v0.3.2 \x1b[33;2m(*)\x1b[0m',
        ), {
            ('serde', '1.0.210'),
            ('quote', '1.0.40'),
            ('convt-app', '0.2.0'),
            ('spirv', '0.4.0+sdk-1.4.341.0'),
            ('objc2-core-audio-types', '0.3.2'),
        })

    def test_unrecognized_line_names_the_script_and_value(self):
        with self.assertRaisesRegex(ValueError, r"source-audit\.py: cannot parse cargo tree package line: 'Zlib OR Apache-2.0 OR MIT'"):
            audit.parse_crate_graph_line('Zlib OR Apache-2.0 OR MIT')

class MacStatus(unittest.TestCase):
 def status(self,tree,value):
  path=tree/'packaging/macos/release-status.json';path.parent.mkdir(parents=True,exist_ok=True);path.write_text(__import__('json').dumps(value))
 def test_missing_or_unready_status_blocks(self):
  with tempfile.TemporaryDirectory() as d:
   tree=Path(d)
   self.assertTrue(audit.mac_gaps(tree))
   self.status(tree,{'distribution_ready':False,'gaps':[]});self.assertTrue(audit.mac_gaps(tree))
   self.status(tree,{'distribution_ready':False,'gaps':['not notarized']});self.assertEqual(audit.mac_gaps(tree),['not notarized'])
 def test_ready_status_must_have_no_gaps(self):
  with tempfile.TemporaryDirectory() as d:
   tree=Path(d)
   self.status(tree,{'distribution_ready':True,'gaps':['x']})
   with self.assertRaises(ValueError):audit.mac_gaps(tree)
   self.status(tree,{'distribution_ready':True,'gaps':[]});self.assertEqual(audit.mac_gaps(tree),[])
 def test_repository_status_matches_file(self):
  tree=Path(__file__).resolve().parents[2]
  status=__import__('json').loads((tree/'packaging/macos/release-status.json').read_text())
  gaps=audit.mac_gaps(tree)
  if status.get('distribution_ready') is True:
   self.assertEqual(gaps, [])
  else:
   self.assertTrue(gaps)

if __name__=='__main__':unittest.main()
