#!/usr/bin/env python3
"""Build the macOS document pack from The Document Foundation's signed DMG.

    packaging/macos/build-document-pack.py OUTPUT.tar.gz

Downloads the DMG pinned in document-pack.lock.json (cached in
$CONVT_MAC_CACHE, default packaging/.cache), checks its SHA-256 and its
Developer ID team, and repacks LibreOffice.app with a top-level `soffice`
launcher, the DMG's licence notices, and nothing but regular files and
directories: the installer refuses links. Links to files become copies of
them. Links to directories (the Python framework's `Versions/Current` and its
top-level aliases, and `MacOS/urelibs`) are left out rather than duplicated;
headless conversion never goes through them. Every Mach-O keeps its embedded
signature. Prints a JSON receipt: archive, sha256, bytes, installed_bytes,
version and the links left out.
"""
from __future__ import annotations

import gzip
import hashlib
import io
import json
import os
import subprocess
import sys
import tarfile
import tempfile
import urllib.request
from pathlib import Path

HERE = Path(__file__).resolve().parent
LAUNCHER = b'''#!/bin/sh
# convt document pack: run LibreOffice from this folder, headless.
# LibreOffice writes its extension cache inside the pack on first run; keep it
# free of group and world write bits, or discovery rejects the pack.
umask "$(printf '%o' $(( $(umask) | 022 )))"
pack_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
exec "$pack_dir/LibreOffice.app/Contents/MacOS/soffice" "$@"
'''


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open('rb') as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b''):
            digest.update(block)
    return digest.hexdigest()


def fetch(lock: dict, cache: Path) -> Path:
    cache.mkdir(parents=True, exist_ok=True)
    dmg = cache / lock['name']
    if not dmg.is_file() or sha256(dmg) != lock['sha256']:
        partial = dmg.with_suffix('.partial')
        with urllib.request.urlopen(lock['url'], timeout=60) as response, partial.open('wb') as out:
            while block := response.read(1024 * 1024):
                out.write(block)
        partial.replace(dmg)
    if sha256(dmg) != lock['sha256']:
        sys.exit(f'{dmg.name} does not match its pinned SHA-256')
    return dmg


def entries(app: Path, omitted: list[str]):
    """(name inside the pack, path to read, is_dir), sorted, links resolved."""
    root = app.resolve()
    for current, dirs, files in os.walk(app):
        dirs.sort()
        files.sort()
        base = Path(current)
        for name in list(dirs):
            path = base / name
            if path.is_symlink():
                dirs.remove(name)
                if not path.resolve().is_relative_to(root):
                    sys.exit(f'Link leaves the app: {path}')
                omitted.append(str(path.relative_to(app)))
                continue
            yield path.relative_to(app.parent), path, True
        for name in files:
            path = base / name
            if path.is_symlink():
                target = path.resolve()
                if not target.is_relative_to(root):
                    sys.exit(f'Link leaves the app: {path}')
                if target.is_dir():
                    omitted.append(str(path.relative_to(app)))
                    continue
            if not path.is_file():
                sys.exit(f'Not a regular file: {path}')
            yield path.relative_to(app.parent), path, False


def add(tar: tarfile.TarFile, name: str, mode: int, data: bytes | None = None, path: Path | None = None) -> int:
    info = tarfile.TarInfo(name)
    info.mode = mode
    if data is None and path is None:
        info.type = tarfile.DIRTYPE
        tar.addfile(info)
        return 0
    if path is not None:
        info.size = path.stat().st_size
        with path.open('rb') as stream:
            tar.addfile(info, stream)
    else:
        info.size = len(data)
        tar.addfile(info, io.BytesIO(data))
    return info.size


def main() -> None:
    output = Path(sys.argv[1]).resolve()
    if output.exists():
        sys.exit(f'Output exists: {output}')
    lock = json.loads((HERE / 'document-pack.lock.json').read_text())
    cache = Path(os.environ.get('CONVT_MAC_CACHE') or HERE.parent / '.cache')
    dmg = fetch(lock, cache)
    mount = Path(tempfile.mkdtemp(prefix='convt-lo-'))
    subprocess.run(['hdiutil', 'attach', '-nobrowse', '-readonly', '-noautoopen',
                    '-mountpoint', str(mount), str(dmg)], check=True, stdout=subprocess.DEVNULL)
    try:
        app = mount / 'LibreOffice.app'
        team = subprocess.run(['codesign', '-dv', str(app)], capture_output=True, text=True).stderr
        if f'TeamIdentifier={lock["team_id"]}' not in team:
            sys.exit('LibreOffice.app is not signed by The Document Foundation')
        subprocess.run(['codesign', '--verify', '--deep', '--strict', str(app)], check=True)
        omitted: list[str] = []
        installed = 0
        output.parent.mkdir(parents=True, exist_ok=True)
        with output.open('wb') as raw, \
                gzip.GzipFile(filename='', fileobj=raw, mode='wb', mtime=0) as zipped, \
                tarfile.open(fileobj=zipped, mode='w', format=tarfile.PAX_FORMAT) as tar:
            installed += add(tar, 'soffice', 0o755, data=LAUNCHER)
            sources = (
                'Corresponding source for this document pack.\n\n'
                f'LibreOffice.app is The Document Foundation\'s unmodified {lock["version"]} build:\n'
                f'  {lock["url"]}\n  sha256 {lock["sha256"]}\n'
                f'Its source code, for this exact release: {lock["source"]}\n\n'
                'convt adds only the soffice launcher in this folder and leaves out links to\n'
                'directories (packaging/macos/build-document-pack.py in\n'
                'https://github.com/opencoredev/convt). Licence notices are in licenses/.\n'
            ).encode()
            installed += add(tar, 'SOURCES.txt', 0o644, data=sources)
            notices = mount / 'LICENSEs'
            if notices.is_dir():
                add(tar, 'licenses', 0o755)
                for path in sorted(notices.iterdir()):
                    if path.is_file() and not path.is_symlink():
                        installed += add(tar, f'licenses/{path.name}', 0o644, path=path)
            for name, path, is_dir in entries(app, omitted):
                if is_dir:
                    add(tar, name.as_posix(), 0o755)
                else:
                    mode = 0o755 if os.access(path, os.X_OK) else 0o644
                    installed += add(tar, name.as_posix(), mode, path=path)
    finally:
        if subprocess.run(['hdiutil', 'detach', str(mount)], stdout=subprocess.DEVNULL).returncode:
            subprocess.run(['hdiutil', 'detach', '-force', str(mount)], check=True, stdout=subprocess.DEVNULL)
    print(json.dumps({
        'archive': str(output),
        'sha256': sha256(output),
        'bytes': output.stat().st_size,
        'installed_bytes': installed,
        'version': lock['version'],
        'omitted_links': omitted,
    }, indent=2))


if __name__ == '__main__':
    main()
