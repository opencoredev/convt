#!/usr/bin/env python3
"""Fetch only locked build inputs; verify even previously cached files."""
import hashlib
import http.client
import json
import pathlib
import sys
import urllib.request


def sha256(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def download(item, path):
    # The Ubuntu pool drops superseded versions; `mirrors` hold permanent
    # copies of the same bytes, tried in order. The digest decides, not the host.
    partial = path.with_suffix(path.suffix + '.partial')
    for url in [item['url'], *item.get('mirrors', [])]:
        try:
            with urllib.request.urlopen(url, timeout=45) as response, partial.open('wb') as output:
                while block := response.read(1024 * 1024):
                    output.write(block)
        except (OSError, http.client.HTTPException) as error:
            # http.client errors such as IncompleteRead are not OSErrors.
            partial.unlink(missing_ok=True)
            print(f'{url}: {error!r}', file=sys.stderr)
            continue
        if sha256(partial) == item['sha256']:
            partial.rename(path)
            return
        partial.unlink()
        print(f'{url}: SHA-256 mismatch', file=sys.stderr)
    sys.exit(f'SHA-256 mismatch or no reachable URL: {path}')


cache = pathlib.Path(sys.argv[1])
cache.mkdir(parents=True, exist_ok=True)
lock = pathlib.Path(sys.argv[2]) if len(sys.argv) > 2 else pathlib.Path(__file__).with_name('inputs.lock.json')
for item in json.loads(lock.read_text()):
    path = cache / item['name']
    if not path.exists() and item.get('built_by'):
        sys.exit(f'Missing source-built input: {path}; run its owning build wrapper')
    if not path.exists():
        download(item, path)
    if sha256(path) != item['sha256']:
        sys.exit(f'SHA-256 mismatch: {path}')
    print(f'verified {item["name"]}')
