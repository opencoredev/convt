#!/usr/bin/env python3
"""Collect hash-pinned upstream notices for the baseline RPM ELF closure."""
import hashlib
import json
from pathlib import Path
import shutil
import sys


def collect(root):
    here = Path(__file__).resolve().parent
    lock = json.loads((here / 'native-notices.lock.json').read_text())
    by_rpm = {rpm: item for item in lock for rpm in item['rpms']}
    receipt = root / 'licenses/native-origins.json'
    origins = json.loads(receipt.read_text())
    for name, origin in origins.items():
        binary = root / 'lib' / name
        if not binary.is_file():
            raise ValueError(f'Missing shipped native component: {name}')
        # Build-time stripping can change ELF bytes after the original copy receipt.
        origin['payload_sha256'] = hashlib.sha256(binary.read_bytes()).hexdigest()
        package = origin.get('rpm')
        if package not in by_rpm:
            raise ValueError(f'No audited notice mapping for shipped component {name}: {package}')
        item = by_rpm[package]
        origin['notices'] = []
        origin['notice_sources'] = []
        for index, entry in enumerate([item] + item.get('supplements', [])):
            notice = here / entry['notice']
            if not notice.is_file() or hashlib.sha256(notice.read_bytes()).hexdigest() != entry['notice_sha256']:
                raise ValueError(f'Missing or corrupt pinned notice for {name}: {notice}')
            dest = root / 'licenses/rpm' / package / ('COPYING.upstream' if index == 0 else entry['name'] + '.upstream.txt')
            dest.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(notice, dest)
            origin['notices'].append(str(dest.relative_to(root)))
            origin['notice_sources'].append({k: entry[k] for k in ('url', 'sha256', 'member', 'notice_sha256')})
        origin.pop('notice_source', None)
    receipt.write_text(json.dumps(origins, indent=2, sort_keys=True) + '\n')
    shutil.copyfile(here / 'native-notices.lock.json', root / 'licenses/native-notices.lock.json')
    print(f'Collected pinned notices for {len(origins)} native libraries')


if __name__ == '__main__':
    try:
        collect(Path(sys.argv[1]).resolve())
    except (ValueError, OSError, KeyError) as error:
        sys.exit(str(error))
