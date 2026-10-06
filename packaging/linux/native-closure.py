#!/usr/bin/env python3
"""Collect an ELF closure inside the pinned baseline builder.

elf-audit.py checks the finished payload afterwards.
"""
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys

root = Path(sys.argv[1]).resolve()
python = '--python' in sys.argv
libdir = root / ('validator-lib' if python else 'lib')
libdir.mkdir(exist_ok=True)
abi = {'libc.so.6', 'libm.so.6', 'libpthread.so.0', 'libdl.so.2', 'librt.so.1', 'libresolv.so.2', 'libutil.so.1', 'libmvec.so.1'}
env = {k: v for k, v in os.environ.items() if k != 'LD_PRELOAD'}
env['LD_LIBRARY_PATH'] = str(libdir) + ':/work/native/lib'
def elf(p):
    return p.is_file() and p.open('rb').read(4) == b'\x7fELF'
queue = sorted((p for p in root.rglob('*') if elf(p)), reverse=True)
report = root if python else root / 'licenses'
previous = report / 'native-origins.json'
origins = json.loads(previous.read_text()) if previous.exists() else {}
origins = {n: o for n, o in origins.items() if (libdir / n).exists()}
seen = set()
while queue:
    path = queue.pop()
    if path.resolve() in seen:
        continue
    seen.add(path.resolve())
    result = subprocess.run(['ldd', str(path)], env=env, capture_output=True, text=True)
    if 'not found' in result.stdout:
        sys.exit(f'Incomplete native closure: {path}\n{result.stdout}')
    for name, source in re.findall(r'(\S+) => (/\S+)', result.stdout):
        if name in abi or Path(source).is_relative_to(root):
            continue
        dest = libdir / name
        if not dest.exists():
            shutil.copy2(source, dest)
            origins[name] = {'source': source, 'sha256': hashlib.sha256(dest.read_bytes()).hexdigest()}
            owner = subprocess.run(['rpm', '-qf', str(Path(source).resolve())], capture_output=True, text=True)
            if owner.returncode == 0 and not python:
                package = owner.stdout.strip()
                origins[name]['rpm'] = package
            queue.append(dest)
report.mkdir(exist_ok=True)
(report / 'native-origins.json').write_text(json.dumps(origins, indent=2, sort_keys=True) + '\n')

# Also validate reused origins: a prior run must not bypass notice collection.
if not python:
    subprocess.run([sys.executable, str(Path(__file__).with_name('collect-native-notices.py')), str(root)], check=True)
