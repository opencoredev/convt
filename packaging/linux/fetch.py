#!/usr/bin/env python3
"""Fetch only locked build inputs; verify even previously cached files."""
import hashlib
import json
import pathlib
import sys
import urllib.request

downloaded_only = '--downloaded-only' in sys.argv[1:]
args = [argument for argument in sys.argv[1:] if argument != '--downloaded-only']
if not args:
    sys.exit('usage: fetch.py [--downloaded-only] CACHE [LOCK]')
cache = pathlib.Path(args[0])
cache.mkdir(parents=True, exist_ok=True)
lock = pathlib.Path(args[1]) if len(args) > 1 else pathlib.Path(__file__).with_name('inputs.lock.json')
if len(args) > 2:
    sys.exit('usage: fetch.py [--downloaded-only] CACHE [LOCK]')
for item in json.loads(lock.read_text()):
    path = cache / item['name']
    if not path.exists() and item.get('built_by'):
        if downloaded_only:
            continue
        sys.exit(f'Missing source-built input: {path}; run its owning build wrapper')
    if not path.exists():
        partial = path.with_suffix(path.suffix + '.partial')
        with urllib.request.urlopen(item['url'], timeout=45) as response, partial.open('wb') as output:
            while block := response.read(1024 * 1024):
                output.write(block)
        partial.rename(path)
    digest = hashlib.file_digest(path.open('rb'), 'sha256').hexdigest()
    if digest != item['sha256']:
        sys.exit(f'SHA-256 mismatch: {path}\n  expected {item["sha256"]}\n  got      {digest}')
    print(f'verified {item["name"]}')
