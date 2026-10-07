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

class CrateGraphLines(unittest.TestCase):
 def test_colored_duplicate_marker_is_name_and_version(self):
  # Exact cargo 1.95 line when CARGO_TERM_COLOR=always and CARGO_HOME is empty.
  line='proc-macro2 v1.0.107 \x1b[33m\x1b[2m(*)\x1b[39m\x1b[22m'
  self.assertEqual(audit.parse_crate_line(line), ('proc-macro2', '1.0.107'))
 def test_split_parser_breaks_on_colored_duplicate_marker(self):
  line='proc-macro2 v1.0.107 \x1b[33m\x1b[2m(*)\x1b[39m\x1b[22m'
  parsed=tuple(line.split(' (')[0].replace(' v',' ',1).split())
  self.assertEqual(len(parsed), 3)
  with self.assertRaisesRegex(ValueError, 'too many values to unpack'):
   n, v = parsed
 def test_common_cargo_tree_suffixes(self):
  cases={
   'serde v1.0.229': ('serde', '1.0.229'),
   'serde_derive v1.0.229 (proc-macro)': ('serde_derive', '1.0.229'),
   'proc-macro2 v1.0.107 (*)': ('proc-macro2', '1.0.107'),
   'convt-app v0.2.0 (/workspace/crates/convt-app)': ('convt-app', '0.2.0'),
   'zed-font-kit v0.14.1-zed': ('zed-font-kit', '0.14.1-zed'),
   'wasi v0.11.0+wasi-snapshot-preview1': ('wasi', '0.11.0+wasi-snapshot-preview1'),
  }
  for line, expected in cases.items():
   self.assertEqual(audit.parse_crate_line(line), expected, line)
 def test_unrecognized_line_is_explicit(self):
  with self.assertRaisesRegex(ValueError, 'unrecognized cargo tree'):
   audit.parse_crate_line('too many values here without a version')
 def test_identities_unpack_as_name_version(self):
  output='\n'.join([
   'objc2-app-kit v0.3.2',
   'proc-macro2 v1.0.107 \x1b[33m\x1b[2m(*)\x1b[39m\x1b[22m',
   'serde_derive v1.0.229 (proc-macro)',
  ])
  unpacked=sorted(f'{n} {v}' for n, v in audit.crate_identities(output))
  self.assertEqual(unpacked, ['objc2-app-kit 0.3.2', 'proc-macro2 1.0.107', 'serde_derive 1.0.229'])

if __name__=='__main__':unittest.main()
