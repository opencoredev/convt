#!/usr/bin/env python3
"""Check a source-built FFmpeg against its receipt and the source lock, then
install its notices for convt.app.

    ffmpeg-input.py BUILD_DIR ARCH LOCK LICENSES_DIR

BUILD_DIR is the output of packaging/release/macos-source-ffmpeg-build.sh.
Refuses a build whose receipt names another architecture, whose binaries
changed after the receipt was written, whose sources or recipe differ from
the lock, or whose software codec checks did not all pass. Writes notices and
a path-free build record into LICENSES_DIR (absolute builder paths and home
directories are replaced). Runs on the macOS system Python (3.9).
"""
import hashlib
import json
import re
import shutil
import sys
from pathlib import Path

# The encoders convt's FFmpeg engine selects, plus PNG and ffprobe.
REQUIRED = ['libx264', 'libx265', 'libvpx-vp9', 'libmp3lame', 'libopus', 'libvorbis',
            'PNG encode/decode with zlib', 'ffprobe']
# Receipt fields that hold builder paths.
PATHS = {'path', 'notices_directory', 'source_directory', 'build_log'}
# Home directories inside strings, such as the configure line and clang's
# InstalledDir.
HOME = re.compile(r'/(Users|home)/[^/\s\'"]+')


def sha(path):
    h = hashlib.sha256()
    with path.open('rb') as stream:
        for block in iter(lambda: stream.read(1 << 20), b''):
            h.update(block)
    return h.hexdigest()


def fail(message):
    sys.exit('ffmpeg-input: ' + message)


def scrub(value, build):
    if isinstance(value, dict):
        return {k: scrub(v, build) for k, v in value.items() if k not in PATHS}
    if isinstance(value, list):
        return [scrub(v, build) for v in value]
    if isinstance(value, str):
        return HOME.sub('/<home>', value.replace(build, '<build>'))
    return value


def main():
    if len(sys.argv) != 5:
        fail('usage: ffmpeg-input.py BUILD_DIR ARCH LOCK LICENSES_DIR')
    build, arch, lock_path, licenses = Path(sys.argv[1]), sys.argv[2], Path(sys.argv[3]), Path(sys.argv[4])
    receipt_path = build / 'provenance.json'
    if not receipt_path.is_file():
        fail(f'no source build for {arch} at {build}; run packaging/release/macos-source-ffmpeg-build.sh --arch {arch}')
    receipt = json.loads(receipt_path.read_text())
    lock = json.loads(lock_path.read_text())['source_build_alternative']
    if receipt.get('architecture') != arch:
        fail(f'{receipt_path} is for {receipt.get("architecture")}, not {arch}')
    binaries = {b.get('name'): b for b in receipt['binaries']}
    if sorted(binaries) != ['ffmpeg', 'ffprobe'] or len(receipt['binaries']) != 2:
        fail('receipt must list exactly ffmpeg and ffprobe')
    for name, binary in binaries.items():
        path = build / 'bin' / name
        if path.is_symlink() or not path.is_file() or sha(path) != binary['sha256']:
            fail(f'{path} does not match its receipt')
    pinned = {s['package']: s['sha256'] for s in lock['sources']}
    built = {s['package']: s['sha256'] for s in receipt['source_inputs']}
    if built != pinned:
        differ = sorted(p for p in set(built) | set(pinned) if built.get(p) != pinned.get(p))
        fail('receipt sources differ from the lock: ' + ', '.join(differ))
    if receipt['build_recipe'].get('sha256') != lock['build_recipe']['sha256']:
        fail('receipt was made by a different build helper than the lock pins')
    checks = receipt.get('checks', [])
    if any(c.get('status') == 'FAIL' for c in checks):
        fail('receipt records a failed check')
    passed = {c.get('codec') or c.get('tool') for c in checks if c.get('status') == 'PASS'}
    missing = [r for r in REQUIRED if r not in passed]
    if missing:
        fail('receipt lacks passing checks for ' + ', '.join(missing))
    if 'configuration:' not in receipt.get('configuration', ''):
        fail('receipt has no FFmpeg configuration')
    # Every notice the lock names, at <package>/<path without the archive's
    # top directory>, with its pinned hash.
    notices = build / 'licenses'
    for source in lock['sources']:
        members = source.get('notice_members') or []
        if not members:
            fail(f'the lock names no notices for {source["package"]}')
        for member in members:
            inner = member['member'].split('/', 1)[1]
            path = notices / source['package'] / inner
            if path.is_symlink() or not path.is_file() or sha(path) != member['sha256']:
                fail(f'notice {source["package"]}/{inner} is missing or changed')

    licenses.mkdir(parents=True, exist_ok=True)
    components = licenses / 'components'
    if not components.exists():
        # Every architecture builds the same pinned sources, so one copy of
        # their notices covers the bundle.
        shutil.copytree(notices, components)
        (licenses / 'sources.json').write_text(json.dumps(lock['sources'], indent=2) + '\n')
    record = scrub({k: receipt[k] for k in ('architecture', 'deployment_target', 'source_date_epoch',
                                            'build_recipe', 'binaries', 'toolchain', 'checks', 'configuration')},
                   str(build.resolve()))
    (licenses / f'build-{arch}.json').write_text(json.dumps(record, indent=2) + '\n')
    print(f'{arch}: source-built FFmpeg, {len(checks)} receipt checks, ffmpeg {receipt["binaries"][0]["sha256"][:12]}')


if __name__ == '__main__':
    main()
