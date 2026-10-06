#!/usr/bin/env python3
"""Require published Linux containers to retain every audited payload notice."""
import argparse
import hashlib
from pathlib import Path
import subprocess
import tarfile
import tempfile


def check(output):
    notices = {str(path.relative_to(output / 'convt')): hashlib.sha256(path.read_bytes()).hexdigest()
               for path in (output / 'convt/licenses').rglob('*') if path.is_file()}
    if not any(name.startswith('licenses/rust/') for name in notices):
        raise ValueError('Actual payload has no Rust notices')
    seen = set()
    with tarfile.open(output / 'convt-linux-x86_64.tar.gz', 'r|gz') as archive:
        for member in archive:
            name = member.name.removeprefix('convt/')
            if name not in notices:
                continue
            data = archive.extractfile(member).read()
            if hashlib.sha256(data).hexdigest() != notices[name]:
                raise ValueError('Tarball notice mismatch: ' + name)
            seen.add(name)
    if seen != set(notices):
        raise ValueError('Tarball omits audited payload notices')
    with tempfile.TemporaryDirectory(prefix='convt-artifact-notices-') as work:
        subprocess.run([str(output / 'convt-linux-x86_64.AppImage'), '--appimage-extract'],
                       cwd=work, check=True, stdout=subprocess.DEVNULL, timeout=120)
        for name, expected in notices.items():
            data = (Path(work) / 'squashfs-root' / name).read_bytes()
            if hashlib.sha256(data).hexdigest() != expected:
                raise ValueError('AppImage notice mismatch: ' + name)
    print('PASS actual tarball and AppImage retain all ' + str(len(notices)) + ' audited payload notice files')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('output', type=Path)
    check(parser.parse_args().output.resolve())
