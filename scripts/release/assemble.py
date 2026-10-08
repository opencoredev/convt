#!/usr/bin/env python3
"""Collect only audited public assets, never intermediate apps or payloads."""
import argparse
import json
from pathlib import Path
import shutil
import re

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
assets = [linux / f'convt-{args.version}-source.tar.gz',
          linux / f'convt-{args.version}-source-closure.tar.gz',
          linux / 'source-audit.json']
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
    assets.append(windows / f'convt-{args.version}-windows-x86_64.msi')
    # Same on-demand pack the compiled Windows URL fetches. Not inside the MSI.
    assets.append(windows / f'convt-{args.version}-windows-x86_64-documents.tar.gz')
    assets.append(windows / f'convt-{args.version}-windows-x86_64-documents.tar.gz.sha256')
# Coverage comes from the source builder; never promote it merely because binaries exist.
# Windows MSI is an optional unsigned release asset until FFmpeg/PDFium corresponding
# source clears packaging/windows/inputs.lock.json. Do not require windows-x86_64 in
# covered_platforms or the Mac/Linux publication gate flips false.
audit = json.loads((linux / 'source-audit.json').read_text())
required = {'linux-x86_64', 'macos-arm64'}
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
