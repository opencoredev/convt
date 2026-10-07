"""Regression: Linux SDK-removal probe receipts must be rebuild-stable."""
import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location('linux_scope', Path(__file__).with_name('linux-source-scope.py'))
scope = importlib.util.module_from_spec(spec)
spec.loader.exec_module(scope)


def cargo_log(tree, home, compiling, duration, colored=False):
    """A cargo progress log like the one shipped in run 37621664130."""
    compiling_lines = '\n'.join(f'   Compiling {name}' for name in compiling)
    finished = f'    Finished `release` profile [optimized] target(s) in {duration}'
    body = (
        f'{compiling_lines}\n'
        f'error: could not compile `demo` (lib)\n'
        f'  --> {tree}/crates/demo/src/lib.rs:1:1\n'
        f'   --> {home}/scratch/out.rs\n'
        f'note: rustc wrote /tmp/rustcABCDEF/diag\n'
        f'{finished}\n'
    )
    if colored:
        return f'\x1b[1m\x1b[92m{body}\x1b[0m'
    return body


class ProbeReceipt(unittest.TestCase):
    def test_two_rebuild_dirs_write_identical_probe_bytes(self):
        # Linux dry run 37621664130: only remove-sdk-only-probe.json differed.
        # ci-a vs ci-b, same mtime/mode, content sha c52fc0a1… vs a40e2c47….
        with tempfile.TemporaryDirectory(prefix='convt-probe-repro-') as tmp:
            left_tree = Path(tmp) / 'ci-a' / 'convt-source'
            right_tree = Path(tmp) / 'ci-b' / 'convt-source'
            left_home = Path(tmp) / 'cargo-aaaa'
            right_home = Path(tmp) / 'cargo-bbbb'
            left = scope.sdk_remove_probe(
                0,
                cargo_log(left_tree, left_home, ('foo v1.0.0', 'bar v2.0.0'), '12.34s', colored=True),
                left_tree,
                left_home,
            )
            right = scope.sdk_remove_probe(
                0,
                cargo_log(right_tree, right_home, ('bar v2.0.0', 'foo v1.0.0'), '2m 15s'),
                right_tree,
                right_home,
            )
            self.assertEqual(left, right)
            dumped = []
            for name, receipt in ('a.json', left), ('b.json', right):
                path = Path(tmp) / name
                scope.dump(path, receipt)
                dumped.append(path.read_bytes())
            self.assertEqual(dumped[0], dumped[1])
            self.assertEqual(hashlib.sha256(dumped[0]).hexdigest(), hashlib.sha256(dumped[1]).hexdigest())

    def test_both_cargo_duration_forms_become_placeholder(self):
        tree, home = Path('/tree'), Path('/home')
        for raw in ('12.34s', '45s', '2m 05s', '6m 05s'):
            text = scope.canonical_probe_stderr(f'Finished target(s) in {raw}\n', tree, home)
            self.assertEqual(text, 'Finished target(s) in $DURATION\n')
            self.assertIsNone(scope.DURATION.search(text))

    def test_canonical_log_strips_paths_duration_color_and_sorts(self):
        tree = Path('/work/ci-a/convt-source')
        home = Path('/tmp/convt-linux-empty-cargo-xyz')
        text = scope.canonical_probe_stderr(
            cargo_log(tree, home, ('serde v1.0.0', 'clap v4.0.0'), '6m 05s', colored=True),
            tree,
            home,
        )
        self.assertNotIn('/work/ci-a', text)
        self.assertNotIn(str(home), text)
        self.assertNotIn('6m 05s', text)
        self.assertNotIn('\x1b', text)
        self.assertNotIn('/tmp/rustcABCDEF', text)
        self.assertIn('$SOURCE/crates/demo/src/lib.rs', text)
        self.assertIn('$EMPTY_CARGO_HOME/scratch/out.rs', text)
        self.assertIn('$RUSTC_TMP/diag', text)
        self.assertIn('in $DURATION', text)
        lines = text.splitlines()
        self.assertEqual(lines, sorted(lines))
        compiling = [line for line in lines if 'Compiling' in line]
        self.assertEqual(compiling, ['   Compiling clap v4.0.0', '   Compiling serde v1.0.0'])

    def test_receipt_refuses_uncanonical_path_or_duration(self):
        tree = Path('/work/ci-a/convt-source')
        home = Path('/tmp/empty-home')
        original = scope.canonical_probe_stderr
        try:
            scope.canonical_probe_stderr = lambda text, *_a, **_k: text
            with self.assertRaisesRegex(ValueError, 'rebuild path'):
                scope.sdk_remove_probe(1, f'failed in {tree}', tree, home)
            with self.assertRaisesRegex(ValueError, 'cargo duration'):
                scope.sdk_remove_probe(1, 'Finished in 2m 05s\n', tree, home)
        finally:
            scope.canonical_probe_stderr = original

    def test_receipt_schema_stays_explicit(self):
        receipt = scope.sdk_remove_probe(0, '', Path('/tree'), Path('/home'),
                                         [{'name': 'objc2', 'version': '0.6.2'}])
        # stderr is not kept: it differed between identical rebuilds (run 37641703105).
        self.assertEqual(set(receipt), {
            'command', 'empty_cargo_home', 'exit_code', 'removed_packages',
        })
        self.assertEqual(receipt['command'], scope.PROBE_COMMAND)
        self.assertTrue(receipt['empty_cargo_home'])
        self.assertEqual(receipt['removed_packages'], [{'name': 'objc2', 'version': '0.6.2'}])

    def test_dump_is_sorted_json(self):
        with tempfile.TemporaryDirectory(prefix='convt-probe-dump-') as tmp:
            path = Path(tmp) / 'probe.json'
            scope.dump(path, {'b': 1, 'a': 0})
            self.assertEqual(path.read_text(), json.dumps({'a': 0, 'b': 1}, indent=2) + '\n')


if __name__ == '__main__':
    unittest.main()
