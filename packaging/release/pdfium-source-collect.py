#!/usr/bin/env python3
"""Hash-verify PDFium inputs and assemble a deterministic source delivery archive."""
import argparse
import gzip
import hashlib
import importlib.util
import io
import json
import os
import sys
from pathlib import Path
import tarfile
import tempfile

sys.dont_write_bytecode = True
HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location('pdfium_source_verify', HERE / 'pdfium-source-verify.py')
verify = importlib.util.module_from_spec(spec)
spec.loader.exec_module(verify)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--lock', type=Path, default=HERE / 'pdfium-source.lock.json')
    parser.add_argument('--output', type=Path)
    parser.add_argument('--epoch', type=int, default=int(os.environ.get('SOURCE_DATE_EPOCH', '0')))
    parser.add_argument('--fetch', action='store_true', help='Fetch absent inputs using pinned URLs/digests')
    args = parser.parse_args()
    if args.epoch < 0:
        parser.error('Epoch must be nonnegative')
    lock = json.loads(args.lock.read_text())
    inputs = {}
    for item in verify.artifacts(lock):
        key = item['cache_filename']
        if key in inputs and inputs[key]['sha256'] != item['sha256']:
            raise ValueError('Conflicting artifact hashes: ' + key)
        inputs[key] = item
    for key, item in inputs.items():
        path = verify.cache_path(item)
        if not path.exists() and args.fetch:
            verify.fetch(item, path)
        verify.verify(item, path)
    verify.verify_associations(lock)
    verify.verify_notice_reproduction(lock)
    print('PASS verified', len(inputs), 'artifacts, attestation subject/builder associations and all source notices', flush=True)
    output = (args.output or verify.CACHE / 'pdfium-source-delivery.tar.gz').resolve()
    if not output.is_relative_to(verify.CACHE):
        raise ValueError('Output must remain within CONVT_BUNDLE_CACHE / packaging/.cache')
    if output.exists():
        raise FileExistsError(output)
    output.parent.mkdir(parents=True, exist_ok=True)
    manifest = {'schema_version': 1, 'pdfium_revision': lock['pdfium_revision'],
                'builder_revision': lock['builder_revision'], 'source_date_epoch': args.epoch,
                'artifacts': [{'archive_member': 'packaging/.cache/' + key,
                               'sha256': item['sha256'], 'size_bytes': item['size_bytes']}
                              for key, item in sorted(inputs.items())],
                'qualification': 'Source/recipe/notice delivery only. No readiness flags or historical rebuild claim.'}
    root = 'pdfium-source-delivery/'
    with tempfile.TemporaryDirectory(prefix='pdfium-delivery-', dir=verify.CACHE) as temporary:
        partial = Path(temporary) / 'delivery.tar.gz'
        with partial.open('wb') as stream, gzip.GzipFile(fileobj=stream, mode='wb', filename='', mtime=args.epoch, compresslevel=1) as compressed, tarfile.open(fileobj=compressed, mode='w|') as archive:
            def add_bytes(member, data):
                entry = tarfile.TarInfo(root + member)
                entry.size = len(data)
                entry.mode = 0o644
                entry.mtime = args.epoch
                archive.addfile(entry, io.BytesIO(data))

            for key, item in sorted(inputs.items()):
                entry = tarfile.TarInfo(root + 'packaging/.cache/' + key)
                entry.size = item['size_bytes']
                entry.mode = 0o644
                entry.mtime = args.epoch
                with verify.cache_path(item).open('rb') as source:
                    archive.addfile(entry, source)
            add_bytes('packaging/release/pdfium-source.lock.json', args.lock.read_bytes())
            for name in ['pdfium-source-verify.py', 'pdfium-source-collect.py', 'pdfium-source-notes.md']:
                add_bytes('packaging/release/' + name, (HERE / name).read_bytes())
            add_bytes('pdfium-delivery.json', (json.dumps(manifest, indent=2) + '\n').encode())
            for binary in lock['binary_associations']:
                platform = Path(binary['cache_filename']).stem.removeprefix('pdfium-')
                with tarfile.open(verify.cache_path(binary)) as source:
                    for notice in binary['notice_members']:
                        data = source.extractfile(notice['member']).read()
                        add_bytes('notices/binary/' + platform + '/' + notice['member'], data)
            grouped = {}
            for notice in lock['supplemental_source_notices']:
                grouped.setdefault(notice['source_cache_filename'], {})[notice['member']] = notice
            for filename, members in grouped.items():
                with tarfile.open(verify.CACHE / filename, 'r|*') as source:
                    for member in source:
                        if member.name in members:
                            data = source.extractfile(member).read()
                            component = Path(filename).stem.removesuffix('.tar')
                            add_bytes('notices/runtime/' + component + '/' + member.name, data)
        # Verify every delivered cached input from the completed archive before publishing it locally.
        expected = {root + item['archive_member']: item for item in manifest['artifacts']}
        seen = set()
        with tarfile.open(partial, 'r|gz') as archive:
            for member in archive:
                if member.name in expected:
                    h = hashlib.sha256()
                    with archive.extractfile(member) as source:
                        for block in iter(lambda: source.read(1024 * 1024), b''):
                            h.update(block)
                    assert h.hexdigest() == expected[member.name]['sha256'], member.name
                    seen.add(member.name)
        assert seen == set(expected)
        partial.replace(output)
    print('PASS source delivery archive:', output)
    print('PASS archive SHA-256:', verify.digest(output), 'bytes:', output.stat().st_size)


if __name__ == '__main__':
    main()
