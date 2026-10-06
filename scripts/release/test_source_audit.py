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
 def test_repository_status_is_not_ready(self):
  self.assertTrue(audit.mac_gaps(Path(__file__).resolve().parents[2]))

if __name__=='__main__':unittest.main()
