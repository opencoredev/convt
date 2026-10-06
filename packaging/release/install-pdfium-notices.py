#!/usr/bin/env python3
"""Install hash-validated PDFium compiler-runtime notices from pinned sources."""
import argparse
import hashlib
import json
from pathlib import Path
import tarfile


def install(lock_path, cache, destination):
    lock = json.loads(lock_path.read_text())
    sources = {item['cache_filename']: item for item in lock['sources']}
    grouped = {}
    for index, notice in enumerate(lock['supplemental_source_notices']):
        grouped.setdefault(notice['source_cache_filename'], []).append((index, notice))
    for filename, notices in grouped.items():
        path = cache / filename
        with path.open('rb') as stream:
            digest = hashlib.file_digest(stream, 'sha256').hexdigest()
        if digest != sources[filename]['sha256']:
            raise ValueError('PDFium source hash mismatch: ' + filename)
        wanted = {notice['member']: (index, notice) for index, notice in notices}
        seen = set()
        with tarfile.open(path, 'r|gz') as archive:
            for member in archive:
                if member.name not in wanted:
                    continue
                index, notice = wanted[member.name]
                data = archive.extractfile(member).read()
                if hashlib.sha256(data).hexdigest() != notice['sha256']:
                    raise ValueError('PDFium notice hash mismatch: ' + member.name)
                destination.mkdir(parents=True, exist_ok=True)
                (destination / (str(index) + '-LICENSE.txt')).write_bytes(data)
                seen.add(member.name)
        if seen != set(wanted):
            raise ValueError('Missing PDFium source notices: ' + filename)
    print('Installed ' + str(sum(map(len, grouped.values()))) + ' PDFium supplemental notices')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('lock', type=Path)
    parser.add_argument('cache', type=Path)
    parser.add_argument('destination', type=Path)
    args = parser.parse_args()
    install(args.lock, args.cache, args.destination)
