"""Regression proof that executable-mode changes invalidate a frozen tree."""
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

spec=importlib.util.spec_from_file_location('release_source',Path(__file__).with_name('source.py'))
source=importlib.util.module_from_spec(spec)
spec.loader.exec_module(source)
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
if __name__=='__main__':unittest.main()
