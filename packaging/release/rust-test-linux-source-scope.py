#!/usr/bin/env python3
"""Regression tests for manifest-only Linux derivation."""
import importlib.util
from pathlib import Path
import tomllib
import unittest

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('linux_scope', ROOT / 'scripts/release/linux-source-scope.py')
scope = importlib.util.module_from_spec(spec)
spec.loader.exec_module(scope)


class DerivationTests(unittest.TestCase):
    def derive(self, text, names, members=None):
        return tomllib.loads(scope.derive_manifest(text.encode(), set(names), members).decode())

    def test_dotted_workspace_dependency_keys_survive(self):
        result = self.derive('''[package]
name = "client"
[dependencies]
core.workspace = true
image = { workspace = true, features = ["png"] }
''', ['core', 'image'])
        self.assertEqual(result['dependencies'], {'core': {'workspace': True}, 'image': {'workspace': True, 'features': ['png']}})

    def test_inactive_dependencies_keep_compiled_feature_names(self):
        result = self.derive('''[package]
name = "client"
[features]
default = ["mac", "serde"]
serde = ["dep:serde_core"]
std = ["mac?/std", "serde_core?/std"]
[target.'cfg(target_os = "macos")'.dependencies.mac]
version = "1"
optional = true
[dependencies.serde_core]
version = "1"
optional = true
[dev-dependencies.serde]
version = "1"
''', ['serde_core'])
        self.assertNotIn('target', result)
        self.assertEqual(result['features']['default'], ['mac', 'serde'])
        self.assertEqual(result['features']['mac'], [])
        self.assertEqual(result['features']['std'], ['serde_core?/std'])
        self.assertEqual(result['features']['serde'], ['dep:serde_core'])

    def test_shared_nonoptional_dependency_references_are_valid(self):
        result = self.derive('''[package]
name = "client"
[features]
macos_kqueue = ["mio"]
[target.'cfg(target_os = "macos")'.dependencies.mio]
version = "1"
optional = true
[target.'cfg(target_os = "linux")'.dependencies.mio]
version = "1"
''', ['mio'])
        self.assertEqual(result['features']['macos_kqueue'], [])
        self.assertIn('cfg(target_os = "linux")', result['target'])

    def test_workspace_scope_is_explicit(self):
        result = self.derive('''[workspace]
resolver = "3"
members = ["crates/*"]
default-members = ["crates/server"]
''', [], ['crates/cli', 'crates/app'])
        self.assertEqual(result['workspace']['members'], ['crates/cli', 'crates/app'])
        self.assertEqual(result['workspace']['default-members'], ['crates/cli', 'crates/app'])

    def test_linux_target_evaluation(self):
        self.assertTrue(scope.linux_target('cfg(all(unix, not(target_os = "macos")))'))
        self.assertFalse(scope.linux_target('cfg(any(target_os = "macos", target_os = "windows"))'))


if __name__ == '__main__':
    unittest.main()
