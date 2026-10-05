#!/usr/bin/env python3
"""Fetch only locked build inputs; verify even previously cached files."""
import hashlib
import json
import pathlib
import sys
import urllib.request

cache = pathlib.Path(sys.argv[1])
cache.mkdir(parents=True, exist_ok=True)
lock = pathlib.Path(sys.argv[2]) if len(sys.argv) > 2 else pathlib.Path(__file__).with_name('inputs.lock.json')
for item in json.loads(lock.read_text()):
    path = cache / item['name']
    if not path.exists():
        partial = path.with_suffix(path.suffix + '.partial')
        with urllib.request.urlopen(item['url']) as response, partial.open('wb') as output:
            while block := response.read(1024 * 1024):
                output.write(block)
        partial.rename(path)
    digest = hashlib.file_digest(path.open('rb'), 'sha256').hexdigest()
    if digest != item['sha256']:
        sys.exit(f'SHA-256 mismatch: {path}')
    print(f'verified {item["name"]}')
