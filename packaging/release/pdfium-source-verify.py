#!/usr/bin/env python3
"""Validate source locks against retained downloads; never changes readiness gates."""
import argparse
import base64
import hashlib
import io
import json
import os
from pathlib import Path
import subprocess
import tarfile
import gzip
import tempfile
import time
import urllib.request
import zipfile

ROOT = Path(__file__).resolve().parents[2]
CACHE = Path(os.environ.get('CONVT_BUNDLE_CACHE', ROOT / 'packaging/.cache')).resolve()


def digest(path):
    h = hashlib.sha256()
    with path.open('rb') as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b''):
            h.update(block)
    return h.hexdigest()


def artifacts(value):
    if isinstance(value, dict):
        if {'url', 'sha256', 'cache_filename'} <= value.keys():
            yield value
        for child in value.values():
            yield from artifacts(child)
    elif isinstance(value, list):
        for child in value:
            yield from artifacts(child)


def cache_path(item):
    path = (CACHE / item['cache_filename']).resolve()
    if not path.is_relative_to(CACHE.resolve()):
        raise ValueError('Cache path escapes packaging/.cache')
    return path


def fetch(item, path):
    path.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='source-fetch-', dir=CACHE) as temporary:
        downloaded = Path(temporary) / 'download'
        retrieval = item.get('retrieval', {})
        if retrieval.get('type') == 'git-archive':
            repository = Path(temporary) / 'git'
            subprocess.run(['git', 'init', '--quiet', str(repository)], check=True, timeout=10)
            subprocess.run(['git', '-C', str(repository), 'fetch', '--quiet', '--depth=1',
                            item['url'], item['revision']], check=True, timeout=120)
            subprocess.run(['git', '-C', str(repository), 'archive', '--format=tar.gz',
                            '--output=' + str(downloaded), item['revision']], check=True, timeout=60)
        else:
            start = time.monotonic()
            request = urllib.request.Request(item['url'], headers={'User-Agent': 'convt-source-lock/1'})
            with urllib.request.urlopen(request, timeout=45) as response, downloaded.open('wb') as stream:
                while block := response.read(1024 * 1024):
                    stream.write(block)
                    if time.monotonic() - start > 180:
                        raise TimeoutError('Source download exceeded 180 seconds')
        if retrieval.get('type') == 'canonical-gitiles-archive':
            with tarfile.open(downloaded, 'r:gz') as archive:
                members = []
                for member in archive.getmembers():
                    data = archive.extractfile(member).read() if member.isfile() else None
                    member.uid = member.gid = 0
                    member.uname = member.gname = ''
                    member.mtime = 0
                    members.append((member, data))
            canonical = Path(temporary) / 'canonical.tar.gz'
            compressed = io.BytesIO()
            with gzip.GzipFile(fileobj=compressed, mode='wb', mtime=0, compresslevel=9) as stream:
                with tarfile.open(fileobj=stream, mode='w:', format=tarfile.PAX_FORMAT) as archive:
                    for member, data in members:
                        member.pax_headers = {}
                        archive.addfile(member, io.BytesIO(data) if data is not None else None)
            canonical.write_bytes(compressed.getvalue())
            downloaded = canonical
        if digest(downloaded) != item['sha256']:
            raise ValueError('Downloaded SHA-256 mismatch: ' + item['cache_filename'])
        downloaded.replace(path)


def verify(item, path):
    if digest(path) != item['sha256']:
        raise ValueError('SHA-256 mismatch: ' + item['cache_filename'])
    if path.stat().st_size != item['size_bytes']:
        raise ValueError('Size mismatch: ' + item['cache_filename'])
    if item.get('cipd_resolution'):
        instance = item['cipd_resolution']['instance']
        assert instance['hashAlgo'] == 'SHA256'
        assert instance['hexDigest'] == item['sha256']
        assert base64.urlsafe_b64encode(bytes.fromhex(item['sha256'])).decode().rstrip('=') == item['instance_id']
    if item.get('binary_member'):
        with zipfile.ZipFile(path) as archive:
            data = archive.read(item['binary_member'])
            assert hashlib.sha256(data).hexdigest() == item['binary_sha256']
            assert item['embedded_configuration'].encode() in data
    if zipfile.is_zipfile(path):
        with zipfile.ZipFile(path) as archive:
            if archive.testzip():
                raise ValueError('ZIP CRC failure: ' + item['cache_filename'])
            names = archive.namelist()
            if 'member_count' in item:
                assert len(names) == item['member_count']
            for member in item.get('notice_members', []):
                name = member['member'] if isinstance(member, dict) else member
                data = archive.read(name)
                if isinstance(member, dict):
                    assert hashlib.sha256(data).hexdigest() == member['sha256'], name
    elif tarfile.is_tarfile(path):
        with tarfile.open(path) as archive:
            names = archive.getnames()
            if 'member_count' in item:
                assert len(names) == item['member_count']
            for member in item.get('notice_members', []) + item.get('recipe_members', []):
                name = member['member'] if isinstance(member, dict) else member
                stream = archive.extractfile(name)
                assert stream is not None, name
                if isinstance(member, dict):
                    assert hashlib.sha256(stream.read()).hexdigest() == member['sha256'], name
            for name in item.get('required_members', []):
                assert name in names, name


def verify_notice_reproduction(lock):
    sources = {item['checkout_path']: item for item in lock['sources'] if item.get('checkout_path') is not None}
    archives = {}
    try:
        for item in lock['notice_reproduction']:
            source = sources[item['source_checkout']]
            filename = source['cache_filename']
            if filename not in archives:
                archives[filename] = tarfile.open(cache_path(source))
            data = archives[filename].extractfile(item['source_member']).read()
            raw = data
            lines = data.splitlines(keepends=True)
            transform = item['transformation']
            if transform == 'strip blank lines, stop before #ifndef':
                data = b''.join(line for line in lines[:next(i for i, line in enumerate(lines) if b'#ifndef' in line)] if line.strip())
            elif transform == 'stop before first blank line':
                data = b''.join(lines[:next(i for i, line in enumerate(lines) if not line.strip())])
            elif transform == 'through first closing comment':
                data = b''.join(lines[:next(i for i, line in enumerate(lines) if b'*/' in line) + 1])
            elif transform == 'comment starting /* zlib.h through first closing comment':
                start = next(i for i, line in enumerate(lines) if line.startswith(b'/* zlib.h'))
                stop = next(i for i in range(start, len(lines)) if b'*/' in lines[i])
                data = b''.join(lines[start:stop + 1])
            elif transform == 'strip leading // and optional space':
                import re
                data = b''.join(re.sub(rb'^//[ \t]?', b'', line) for line in lines)
            elif transform != 'copy':
                raise ValueError('Unknown notice transformation: ' + transform)
            assert hashlib.sha256(data).hexdigest() == item['sha256'], item['binary_member']
            for association in item['verified_binary_associations']:
                observed = raw if association['transformation'].startswith('copy source LICENSE unchanged') else data
                assert hashlib.sha256(observed).hexdigest() == association['sha256']
                binary = next(b for b in lock['binary_associations'] if b['cache_filename'] == association['cache_filename'])
                notice = next(n for n in binary['notice_members'] if n['member'] == item['binary_member'])
                assert notice['sha256'] == association['sha256']
    finally:
        for archive in archives.values():
            archive.close()


def verify_associations(lock):
    if not lock.get('attestation'):
        return
    bundle = json.loads(cache_path(lock['attestation']).read_text())
    statement = json.loads(base64.b64decode(bundle['dsseEnvelope']['payload']))
    recipe_commit = statement['predicate']['buildDefinition']['resolvedDependencies'][0]['digest']['gitCommit']
    assert recipe_commit == lock['builder_revision'] == lock['build_recipe']['revision']
    subjects = {entry['name']: entry['digest']['sha256'] for entry in statement['subject']}
    for binary in lock['binary_associations']:
        assert subjects[Path(binary['cache_filename']).name] == binary['sha256']
        assert binary['source_revision'] == lock['pdfium_revision']
        assert binary['build_recipe_revision'] == lock['builder_revision']
        assert binary['source_archive_indexes'] == list(range(len(lock['sources'])))
    grouped = {}
    for notice in lock.get('supplemental_source_notices', []):
        grouped.setdefault(notice['source_cache_filename'], {})[notice['member']] = notice['sha256']
    for filename, members in grouped.items():
        seen = set()
        with tarfile.open(CACHE / filename, 'r|*') as archive:
            for member in archive:
                if member.name in members:
                    data = archive.extractfile(member).read()
                    assert hashlib.sha256(data).hexdigest() == members[member.name]
                    seen.add(member.name)
        assert seen == set(members)



def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('locks', nargs='*', type=Path)
    parser.add_argument('--fetch', action='store_true', help='Fetch absent artifacts using locked URLs and hashes')
    args = parser.parse_args()
    locks = args.locks or [ROOT / 'packaging/release/pdfium-source.lock.json', ROOT / 'packaging/release/macos-source-ffmpeg.lock.json']
    count = 0
    for lock_path in locks:
        lock = json.loads(lock_path.read_text())
        seen = set()
        for item in artifacts(lock):
            if item['cache_filename'] in seen:
                continue
            seen.add(item['cache_filename'])
            path = cache_path(item)
            if not path.exists() and args.fetch:
                fetch(item, path)
            verify(item, path)
            count += 1
        local_recipe = lock.get('source_build_alternative', {}).get('build_recipe')
        if local_recipe:
            assert digest(ROOT / local_recipe['path']) == local_recipe['sha256'], 'Local build helper differs from source lock'
        verify_associations(lock)
        if lock.get('notice_reproduction'):
            verify_notice_reproduction(lock)
        print('PASS', lock_path.name, len(seen), 'artifacts; source/binary notice associations validated')
    print('PASS', count, 'artifact records; no readiness gates changed')


if __name__ == '__main__':
    main()
