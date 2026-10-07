#!/usr/bin/env python3
"""Sync Changesets' fixed product version without resolving Rust dependencies."""
import argparse
import json
from pathlib import Path
import re
import tomllib

ROOT = Path(__file__).resolve().parents[2]
PACKAGES = ('apps/desktop/package.json', 'tools/cli/package.json', 'apps/web/package.json')


def version():
    versions = [json.loads((ROOT / path).read_text())['version'] for path in PACKAGES]
    if len(set(versions)) != 1 or not re.fullmatch(r'(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)', versions[0]):
        raise ValueError(f'Product versions must be synchronized stable semver: {versions}')
    return versions[0]


def synchronize(check=False):
    target = version()
    manifest = ROOT / 'Cargo.toml'
    text = manifest.read_text()
    current = tomllib.loads(text)['workspace']['package']['version']
    text, count = re.subn(r'(\[workspace\.package\]\s*\nversion = ")[^"]+("\n)',
                          lambda m: m[1] + target + m[2], text)
    if count != 1:
        raise ValueError('Expected one workspace.package version')
    crates = set()
    for path in (ROOT / 'crates').glob('*/Cargo.toml'):
        package = tomllib.loads(path.read_text())['package']
        if package['version'] != {'workspace': True}:
            raise ValueError(f'{path} must inherit the workspace version')
        crates.add(package['name'])
    lock = ROOT / 'Cargo.lock'
    locked = lock.read_text()
    found = set()

    def update(match):
        block = match[0]
        package = tomllib.loads(block)['package'][0]
        if package['name'] not in crates or 'source' in package:
            return block
        found.add(package['name'])
        if check and package['version'] != target:
            raise ValueError(f'Cargo.lock {package["name"]} is not {target}')
        return re.sub(r'^version = "[^"]+"$', f'version = "{target}"', block, count=1, flags=re.M)

    updated = re.sub(r'\[\[package\]\][\s\S]*?(?=\n\[\[package\]\]|\Z)', update, locked)
    if found != crates:
        raise ValueError(f'Workspace crates missing from lock: {crates - found}')
    if check:
        if current != target:
            raise ValueError(f'Cargo.toml is {current}, expected {target}')
    else:
        manifest.write_text(text)
        lock.write_text(updated)
    print(target)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--check', action='store_true')
    synchronize(parser.parse_args().check)
