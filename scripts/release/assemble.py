#!/usr/bin/env python3
"""Collect only audited public assets, never intermediate apps or payloads."""
import argparse
import json
from pathlib import Path
import shutil
import re
import tarfile
import hashlib

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('downloads', type=Path)
parser.add_argument('output', type=Path)
parser.add_argument('version')
parser.add_argument('--include-windows', action='store_true')
args = parser.parse_args()
if not re.fullmatch(r'\d+\.\d+\.\d+', args.version):
    parser.error('expected stable semver')
args.output.mkdir(exist_ok=False)
linux = args.downloads / 'linux-release-review'
macos = args.downloads / 'macos-release-review'
windows = args.downloads / 'windows-release-review'
assets = [linux / f'convt-{args.version}-source.tar.gz', linux / 'source-audit.json']
for kind in ('.deb', '.rpm', '.AppImage'):
    matches = list(linux.glob(f'*{kind}'))
    if len(matches) != 1:
        raise ValueError(f'Expected one Linux {kind} artifact')
    assets.extend(matches)
tarballs = list(linux.glob('*linux-x86_64*.tar.gz'))
if len(tarballs) != 1:
    raise ValueError('Expected one Linux binary tarball')
assets.extend(tarballs)
assets.append(macos / 'convt-macos-arm64.dmg')
if args.include_windows:
    windows_source = windows / f'convt-{args.version}-windows-source.tar.gz'
    assets += [windows / f'convt-{args.version}-windows-x86_64.msi', windows_source]
    # The MSI ships only with its third-party corresponding source: every
    # pinned tarball from both Windows locks, byte for byte.
    repo = Path(__file__).resolve().parents[2]
    expected = {s['name']: s['sha256'] for s in json.loads((repo / 'packaging/windows/ffmpeg-source.lock.json').read_text())['sources']}
    expected |= {f['name']: f['sha256'] for f in json.loads((repo / 'packaging/windows/inputs.lock.json').read_text())['files']
                 if f.get('kind') in ('source', 'patches')}
    root = f'convt-{args.version}-windows-source'
    with tarfile.open(windows_source) as archive:
        for name, digest in expected.items():
            try:
                member = archive.extractfile(f'{root}/sources/{name}')
            except KeyError:
                member = None
            if member is None or hashlib.sha256(member.read()).hexdigest() != digest:
                raise ValueError(f'Windows source archive lacks pinned source {name}')
        names = set(archive.getnames())
        for name in ('recipe/packaging/windows/build-ffmpeg.sh', 'ffmpeg-build/receipt.json'):
            if f'{root}/{name}' not in names:
                raise ValueError(f'Windows source archive lacks {name}')
# Coverage comes from the source builder; never promote it merely because binaries exist.
audit = json.loads((linux / 'source-audit.json').read_text())
required = {'linux-x86_64', 'macos-arm64'}
if args.include_windows:
    required.add('windows-x86_64')
if (audit.get('distribution_ready') is not True or audit.get('gaps') != []
        or not required.issubset(audit.get('covered_platforms', []))
        or any(audit.get('platform_gaps', {}).get(p) for p in required)):
    raise ValueError('Corresponding-source audit must cover every included platform before publication')
for path in assets:
    if not path.is_file() or path.stat().st_size == 0:
        raise ValueError(f'Missing or empty release asset: {path}')
    destination = args.output / path.name
    if destination.exists():
        raise ValueError(f'Duplicate release asset: {path.name}')
    shutil.copyfile(path, destination)
print(f'Assembled {len(assets)} public assets for {args.version}')
