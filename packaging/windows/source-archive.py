#!/usr/bin/env python3
"""Write convt-VERSION-windows-source.tar.gz: the corresponding source for the
third-party code built into the Windows MSI.

    python source-archive.py FFMPEG_BUILD CACHE VERSION OUTPUT_DIR

It holds every pinned source tarball byte for byte (FFmpeg and its codecs from
ffmpeg-source.lock.json; x265, libde265, aom and libheif with their Ubuntu
patches from inputs.lock.json), the build recipes and the FFmpeg build record.
convt's own source is the separate convt-VERSION-source.tar.gz asset.
"""
import gzip
import hashlib
import io
import json
import os
import pathlib
import sys
import tarfile

HERE = pathlib.Path(__file__).resolve().parent
REPO = HERE.parents[1]
RECIPES = ['packaging/windows/build.ps1', 'packaging/windows/build-native.ps1',
           'packaging/windows/build-ffmpeg.sh', 'packaging/windows/build-ffmpeg-inside.sh',
           'packaging/windows/ffmpeg-source.lock.json', 'packaging/windows/inputs.lock.json',
           'packaging/windows/build-document-pack.py', 'packaging/windows/document-launcher.rs',
           'packaging/windows/installer.ps1', 'packaging/windows/convt.wxs',
           'packaging/windows/source-archive.py', 'packaging/linux/libheif-explicit-init.patch']
BUILD_RECORD = ['receipt.json', 'build.log', 'ffmpeg-config.log', 'builder-packages.txt',
                'toolchain.txt', 'dll-imports.txt']
README = """convt Windows x86_64 corresponding source

sources/   Every third-party source tarball built into the MSI, unmodified.
           FFmpeg and its codecs: packaging/windows/ffmpeg-source.lock.json.
           x265, libde265, aom, libheif and their Ubuntu patch sets:
           packaging/windows/inputs.lock.json (applied by build-native.ps1).
recipe/    The scripts that built them. FFmpeg is cross-built with mingw-w64
           in the pinned Ubuntu image named in ffmpeg-source.lock.json:
             bash packaging/windows/build-ffmpeg.sh CACHE OUTPUT
ffmpeg-build/  The FFmpeg build log, configure log, toolchain versions and the
           receipt with the SHA-256 of the shipped ffmpeg.exe and ffprobe.exe.

convt itself (AGPL-3.0-only): convt-{version}-source.tar.gz in the same release.
PDFium (BSD-3-Clause and permissive third-party code) is the prebuilt
chromium/8076 release of https://github.com/bblanchon/pdfium-binaries; source:
https://pdfium.googlesource.com/pdfium/+/refs/heads/chromium/8076
LibreOffice 25.8.7 (MPL-2.0) is repackaged unmodified from the official MSI;
source: https://download.documentfoundation.org/libreoffice/src/25.8.7/
"""


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    build, cache, version, output = sys.argv[1:]
    build, cache, output = pathlib.Path(build), pathlib.Path(cache), pathlib.Path(output)
    epoch = int(os.environ.get('SOURCE_DATE_EPOCH', '0'))
    ffmpeg_lock = json.loads((HERE / 'ffmpeg-source.lock.json').read_text())
    inputs_lock = json.loads((HERE / 'inputs.lock.json').read_text())
    receipt = json.loads((build / 'receipt.json').read_text())
    if receipt['source_inputs'] != ffmpeg_lock['sources']:
        raise SystemExit('FFmpeg build receipt differs from ffmpeg-source.lock.json')
    sources = [(s, build / 'sources' / s['name']) for s in ffmpeg_lock['sources']]
    sources += [(s, cache / s['name']) for s in inputs_lock['files'] if s.get('kind') in ('source', 'patches')]
    root = f'convt-{version}-windows-source'
    entries = [(f'{root}/README.txt', README.format(version=version).encode())]
    for item, path in sources:
        if sha(path) != item['sha256']:
            raise SystemExit(f'Hash mismatch: {path}')
        entries.append((f"{root}/sources/{item['name']}", path))
    entries += [(f'{root}/recipe/{name}', REPO / name) for name in RECIPES]
    entries += [(f'{root}/ffmpeg-build/{name}', build / name) for name in BUILD_RECORD]
    output.mkdir(parents=True, exist_ok=True)
    archive_path = output / f'convt-{version}-windows-source.tar.gz'
    with archive_path.open('wb') as raw:
        with gzip.GzipFile(filename='', fileobj=raw, mode='wb', mtime=epoch) as zipped:
            with tarfile.open(fileobj=zipped, mode='w', format=tarfile.PAX_FORMAT) as archive:
                for name, content in sorted(entries, key=lambda e: e[0]):
                    data = content if isinstance(content, bytes) else content.read_bytes()
                    info = tarfile.TarInfo(name)
                    info.size, info.mtime, info.mode = len(data), epoch, 0o644
                    archive.addfile(info, io.BytesIO(data))
    record = {'schema_version': 1, 'platform': 'windows-x86_64', 'archive': archive_path.name,
              'archive_sha256': sha(archive_path),
              'sources': [{'name': i['name'], 'package': i['package'], 'sha256': i['sha256']} for i, _ in sources],
              'ffmpeg_binaries': receipt['binaries']}
    (output / 'windows-source.json').write_text(json.dumps(record, indent=2) + '\n')
    print(f'{archive_path.name}: {len(sources)} pinned sources, {archive_path.stat().st_size} bytes')


if __name__ == '__main__':
    main()
