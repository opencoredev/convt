#!/usr/bin/env python3
"""Build a headless document pack from an installed, trusted LibreOffice.

Release CI runs it through document-pack.sh, in a pinned Ubuntu 22.04 image with
apt pinned to one snapshot. Run directly, it packs this machine's LibreOffice
for local validation. It performs no network requests and never includes
system libc or the loader.
"""
import gzip
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tarfile
import tempfile

out = Path(sys.argv[1]).resolve()
out.mkdir(parents=True, exist_ok=True)
pack = out / 'documents'
if pack.exists():
    sys.exit(f'Already exists: {pack}')
pack.mkdir()
# Jammy ships dangling links to optional LDAP example files, not filters.
shutil.copytree('/usr/lib/libreoffice', pack / 'libreoffice', symlinks=False, ignore_dangling_symlinks=True)
# Ubuntu's bootstrap hard-codes FHS locations. Point it at the private tree.
rc = pack / 'libreoffice/program/fundamentalrc'
rc.write_text(rc.read_text().replace('file:///usr/lib/libreoffice', '${ORIGIN}/..').replace('file:///etc/libreoffice/registry', '${BRAND_BASE_DIR}/share/registry').replace(' dconf:*', ''))
shutil.copy2('/etc/libreoffice/sofficerc', pack / 'libreoffice/program/sofficerc')
(pack / 'lib').mkdir()
# Include every dependency referenced by a shipped ELF, including filter plugins.
deps = {}
dynamic_modules = [next(Path('/usr/lib/x86_64-linux-gnu').rglob(n))
                   for n in ['libsoftokn3.so', 'libfreebl3.so', 'libfreeblpriv3.so']]
for path in [*(pack / 'libreoffice').rglob('*'), *dynamic_modules]:
    if not path.is_file() or path.open('rb').read(4) != b'\x7fELF':
        continue
    output = subprocess.run(['ldd', str(path)], capture_output=True, text=True).stdout
    for name, src in re.findall(r'(\S+) => (/\S+)', output):
        if name in {'libc.so.6', 'libm.so.6', 'libpthread.so.0', 'libdl.so.2', 'librt.so.1'}:
            continue
        deps[name] = src
for name, src in sorted(deps.items()):
    if not (pack / 'libreoffice/program' / name).exists():
        shutil.copy2(src, pack / 'lib' / name)
# NSS loads these modules dynamically, so ldd alone cannot find them.
for module in dynamic_modules:
    shutil.copy2(module, pack / 'lib' / module.name)
shutil.copytree('/usr/share/liblangtag', pack / 'langtag')
shim = Path(__file__).with_name('langtag-path.c')
subprocess.run(['cc', '-shared', '-fPIC', '-O2', str(shim), '-ldl', '-o', str(pack / 'lib/convt-langtag.so')], check=True)
shutil.copy2(shim, pack / 'langtag-path.c')
shutil.copy2(Path(__file__).parents[2] / 'LICENSE', pack / 'convt-shim-LICENSE')
(pack / 'libreoffice/program/javaldx').unlink(missing_ok=True)
shutil.copytree('/usr/share/fonts/truetype/dejavu', pack / 'fonts')
(pack / 'fonts.conf').write_text('''<?xml version="1.0"?>
<!DOCTYPE fontconfig SYSTEM "urn:fontconfig:fonts.dtd">
<fontconfig><dir prefix="relative">fonts</dir><cachedir prefix="xdg">convt/fonts</cachedir></fontconfig>
''')
(pack / 'soffice').write_text('''#!/bin/sh
set -eu
# LibreOffice writes its extension cache inside the pack on first run. Keep
# that free of group and world write bits, or discovery rightly rejects the
# pack under a umask such as Ubuntu's 002.
umask "$(printf '%o' $(( $(umask) | 022 )))"
pack_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
export LD_LIBRARY_PATH="$pack_dir/lib:$pack_dir/libreoffice/program"
# Fontconfig 2.13 resolves relative font directories against the working
# directory. Generate an absolute config without changing the job's cwd.
font_config_dir=$(mktemp -d)
trap 'rm -rf "$font_config_dir"' 0
font_dir=$(printf '%s' "$pack_dir/fonts" | sed 's/&/\\&amp;/g; s/</\\&lt;/g; s/>/\\&gt;/g')
font_cache=$(printf '%s' "${XDG_CACHE_HOME:-$HOME/.cache}/convt/fonts" | sed 's/&/\\&amp;/g; s/</\\&lt;/g; s/>/\\&gt;/g')
printf '<fontconfig><dir>%s</dir><cachedir>%s</cachedir></fontconfig>\\n' "$font_dir" "$font_cache" > "$font_config_dir/fonts.conf"
export SAL_USE_VCLPLUGIN=svp FONTCONFIG_FILE="$font_config_dir/fonts.conf"
export CONVT_LANGTAG_DIR="$pack_dir/langtag" LD_PRELOAD="$pack_dir/lib/convt-langtag.so"
"$pack_dir/libreoffice/program/soffice" "$@"
''')
(pack / 'soffice').chmod(0o755)
versions = subprocess.check_output(['dpkg-query', '-W', '-f=${db:Status-Abbrev} ${Package} ${Version}\n', 'libreoffice*'], text=True)
(pack / 'BUILD.txt').write_text('LibreOffice from these Ubuntu packages:\n' + '\n'.join(l for l in versions.splitlines() if l.startswith('ii'))+'\n')
# Preserve redistribution notices for every installed package owning copied libraries.
(pack / 'licenses').mkdir()
packages = set()
for src in deps.values():
    result = subprocess.run(['dpkg-query','-S',src],capture_output=True,text=True)
    for line in result.stdout.splitlines():
        packages.add(line.split(': ')[0].split(':')[0])
packages.update(l.split()[1] for l in versions.splitlines() if l.startswith('ii'))
for package in sorted(packages):
    copyright = Path('/usr/share/doc') / package / 'copyright'
    if copyright.is_file():
        shutil.copy2(copyright, pack / 'licenses' / (package + '.txt'))
archive = out / 'documents-linux-x86_64.tar.gz'
# No symbolic or hard links: extraction rejects them before publishing executable code.
# Fixed owners, times and gzip header, so the same packages give the same bytes.
installed = 0
with archive.open('wb') as raw, gzip.GzipFile(filename='', fileobj=raw, mode='wb', mtime=0) as zipped, \
        tarfile.open(fileobj=zipped, mode='w', format=tarfile.PAX_FORMAT, dereference=True) as tar:
    for p in sorted(pack.rglob('*')):
        info = tar.gettarinfo(str(p), arcname=str(p.relative_to(pack)))
        info.uid = info.gid = info.mtime = 0
        info.uname = info.gname = ''
        info.pax_headers = {}
        if info.isfile():
            installed += info.size
            with p.open('rb') as stream:
                tar.addfile(info, stream)
        else:
            tar.addfile(info)
digest = hashlib.sha256()
with archive.open('rb') as stream:
    for block in iter(lambda: stream.read(1024 * 1024), b''):
        digest.update(block)
sha = digest.hexdigest()
(out / 'documents.sha256').write_text(sha + '\n')
core = re.search(r'^ii\s+libreoffice-core\s+(?:\d+:)?(\d+(?:\.\d+)*)', versions, re.M)
label = f'LibreOffice {core.group(1)} Linux x86_64' if core else 'LibreOffice Linux x86_64'
print(json.dumps(dict(path=str(pack),archive=str(archive),sha256=sha,bytes=archive.stat().st_size,installed_bytes=installed,version=label), indent=2))
