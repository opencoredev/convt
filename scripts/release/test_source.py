"""Regression proof that executable-mode changes invalidate a frozen tree."""
import importlib.util
import json
from pathlib import Path
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
if __name__=='__main__':unittest.main()
