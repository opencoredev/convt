#!/usr/bin/env python3
"""Exercise notice preservation and fail-closed supplemental source checks."""
import copy
import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location('rust_license_map', ROOT / 'linux/rust-license-map.py')
mapper = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(mapper)


class NoticeMapTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.directory = Path(self.temp.name)
        self.addCleanup(self.temp.cleanup)
        (self.directory / 'Cargo.toml').write_text('[package]\nname="fixture"\nversion="1.0.0"\nlicense="MIT"\n')
        self.package = {'name': 'fixture', 'version': '1.0.0', 'source': 'registry+https://github.com/rust-lang/crates.io-index',
                        'license': 'MIT', 'license_file': None, 'manifest_path': str(self.directory / 'Cargo.toml')}
        entry = json.loads((mapper.RELEASE_ROOT / 'rust-notice-sources.json').read_text())['packages'][0]
        self.supplement = copy.deepcopy(entry)
        self.supplement.update(name='fixture', version='1.0.0', source=self.package['source'],
                               manifest_sha256=mapper.digest((self.directory / 'Cargo.toml').read_bytes()))
        self.supplement.pop('source_match', None)
        self.supplement.pop('vcs_sha1', None)

    def collect(self):
        return mapper.collect(self.package, {('fixture', '1.0.0'): self.supplement})

    def test_nested_source_notices_preserve_bytes(self):
        path = self.directory / 'tests/native/NOTICE'
        path.parent.mkdir(parents=True)
        data = b'Copyright (c) 2026 Notice fixture\nPreserve this notice.\n'
        path.write_bytes(data)
        record = mapper.collect(self.package, {})
        self.assertEqual(record['notices'][0]['text'].encode(), data)
        self.assertEqual(record['notices'][0]['sha256'], hashlib.sha256(data).hexdigest())

    def test_header_identifier_without_manifest_grant_does_not_close_gap(self):
        self.package['license'] = None
        (self.directory / 'Cargo.toml').write_text('[package]\nname="fixture"\nversion="1.0.0"\n')
        (self.directory / 'lib.rs').write_text('// Copyright (c) 2026 Fixture\n// Licensed under MIT.\nfn fixture() {}\n')
        record = mapper.collect(self.package, {})
        self.assertFalse(record['notices'])
        self.assertTrue(record['status'].startswith('unresolved:'))
        self.assertEqual(len(record['source_headers']), 1)
        self.assertIn('Copyright (c) 2026 Fixture', record['holders'])

    def test_declared_spdx_terms_preserve_real_holder_and_manifest(self):
        source = '// Copyright (c) 2026 Actual Fixture Holder\n// Licensed under MIT.\nfn fixture() {}\n'
        (self.directory / 'lib.rs').write_text(source)
        record = mapper.collect(self.package, {})
        self.assertTrue(record['notice_complete'])
        self.assertIn('reproduced declared SPDX terms', record['status'])
        self.assertEqual(record['license_declaration']['text'], (self.directory / 'Cargo.toml').read_text())
        self.assertEqual(record['holders'], ['Copyright (c) 2026 Actual Fixture Holder'])
        self.assertEqual(record['source_headers'][0]['text'], source.split('fn fixture')[0])
        canonical = [n for n in record['notices'] if n['source_path'].startswith('reproduced-declared-SPDX/')]
        self.assertEqual(len(canonical), 1)
        self.assertIn('<copyright holders>', canonical[0]['text'])
        self.assertFalse(any('<copyright holders>' in holder for holder in record['holders']))

    def test_valid_declaration_with_no_holder_does_not_invent_one(self):
        record = mapper.collect(self.package, {})
        self.assertTrue(record['notice_complete'])
        self.assertTrue(record['holders'][0].startswith('NOASSERTION:'))

    def test_expression_parser_does_not_guess_unknown_terms(self):
        for expression in ('MIT OR', 'MIT Apache-2.0', 'MIT OR LicenseRef-Unknown', 'MIT WITH Unknown-exception', 'https://example.com/license', 'Unlicensed'):
            self.assertIsNone(mapper.declared_terms(expression), expression)
        normalized, terms = mapper.declared_terms('MIT/Apache-2.0')
        self.assertEqual(normalized, 'MIT OR Apache-2.0')
        self.assertEqual({n['provenance']['license_id'] for n in terms}, {'MIT', 'Apache-2.0'})

    def test_manifest_declaration_must_match_metadata(self):
        self.package['license'] = 'Apache-2.0'
        with self.assertRaisesRegex(ValueError, 'SPDX declaration differs from manifest'):
            mapper.collect(self.package, {})

    def test_corrupt_canonical_spdx_source_is_rejected(self):
        lock = json.loads((mapper.RELEASE_ROOT / 'rust-spdx-sources.json').read_text())
        lock['licenses'][0]['sha256'] = '0' * 64
        source_path = mapper.RELEASE_ROOT / lock['licenses'][0]['path']
        destination = self.directory / lock['licenses'][0]['path']
        destination.parent.mkdir(parents=True)
        destination.write_bytes(source_path.read_bytes())
        (self.directory / 'rust-spdx-sources.json').write_text(json.dumps(lock))
        mapper.canonical_spdx.cache_clear()
        try:
            with patch.object(mapper, 'RELEASE_ROOT', self.directory):
                with self.assertRaisesRegex(ValueError, 'SPDX source hash mismatch'):
                    mapper.collect(self.package, {})
        finally:
            mapper.canonical_spdx.cache_clear()

    def test_real_pinned_supplement_is_collected(self):
        self.assertTrue(self.collect()['notices'])

    def test_incomplete_upstream_declaration_keeps_release_gate_closed(self):
        self.supplement['notice_complete'] = False
        record = self.collect()
        self.assertFalse(record['notices'])
        self.assertFalse(record['notice_complete'])
        self.assertTrue(record['incomplete_notices'])
        self.assertTrue(record['status'].startswith('unresolved:'))

    def test_tampered_notice_hash_is_rejected(self):
        self.supplement['notices'][0]['sha256'] = '0' * 64
        with self.assertRaisesRegex(ValueError, 'notice hash mismatch'):
            self.collect()

    def test_wrong_manifest_identity_is_rejected(self):
        self.supplement['manifest_sha256'] = '0' * 64
        with self.assertRaisesRegex(ValueError, 'stale supplemental source identity'):
            self.collect()

    def test_source_content_mismatch_is_rejected(self):
        (self.directory / 'lib.rs').write_text('changed source\n')
        self.supplement['source_match'] = [{'path': 'lib.rs', 'sha256': '0' * 64}]
        with self.assertRaisesRegex(ValueError, 'upstream source match changed'):
            self.collect()

    def test_vendor_discovery_includes_unresolved_extra_crate(self):
        (self.directory / 'Cargo.toml').write_text('[package]\nname="fixture"\nversion="1.0.0"\nlicense="LicenseRef-Unknown"\n')
        (self.directory / '.cargo-checksum.json').write_text('{"package": "archive-checksum", "files": {}}')
        packages = list(mapper.vendor_packages(self.directory))
        self.assertEqual(len(packages), 1)
        self.assertEqual(packages[0]['name'], 'fixture')
        self.assertFalse(mapper.collect(packages[0], {})['notices'])


if __name__ == '__main__':
    unittest.main()
