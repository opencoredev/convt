#!/usr/bin/env python3
"""Audit every ELF file under a finished payload.

usage: elf-audit.py ROOT [--lib DIR]... [--report FILE]
       elf-audit.py --self-test

Each ELF must need no GLIBC symbol version above the 2.28 baseline, and every
dependency must resolve inside ROOT or to the host glibc ABI. ldd runs with
LD_LIBRARY_PATH limited to the --lib directories (default ROOT/lib), as the
launcher sets it. All violations are printed and the exit status is 1.
"""
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile

BASELINE = (2, 28)
ABI = {'libc.so.6', 'libm.so.6', 'libpthread.so.0', 'libdl.so.2', 'librt.so.1',
       'libresolv.so.2', 'libutil.so.1', 'libmvec.so.1'}


def is_elf(path):
    return path.is_file() and path.open('rb').read(4) == b'\x7fELF'


def audit(root, libs):
    root = root.resolve()
    env = {k: v for k, v in os.environ.items() if k not in ('LD_PRELOAD', 'LD_AUDIT')}
    env['LD_LIBRARY_PATH'] = ':'.join(str(p) for p in libs)
    violations, receipts, seen = [], [], set()
    for path in sorted(root.rglob('*')):
        if not is_elf(path) or path.resolve() in seen:
            continue
        seen.add(path.resolve())
        name = path.relative_to(root)
        info = subprocess.run(['readelf', '--version-info', '--wide', str(path)],
                              capture_output=True, text=True, check=True).stdout
        versions = {(int(a), int(b)) for a, b in re.findall(r'\bGLIBC_(\d+)\.(\d+)', info)}
        if versions and max(versions) > BASELINE:
            violations.append(f'{name}: needs GLIBC_{"%d.%d" % max(versions)}, above the 2.28 baseline')
        resolved = subprocess.run(['ldd', str(path)], env=env, capture_output=True, text=True)
        out = resolved.stdout + resolved.stderr
        for line in out.splitlines():
            if 'not found' in line:
                violations.append(f'{name}: unresolved {line.strip()}')
        for dep, source in re.findall(r'(\S+) => (/\S+)', out):
            if dep not in ABI and not Path(source).resolve().is_relative_to(root):
                violations.append(f'{name}: unbundled dependency {dep} from {source}')
        # Drop load addresses, which ASLR changes on every run.
        receipts.append(f'{name}\n' + re.sub(r' \(0x[0-9a-f]+\)', '', out))
    return violations, receipts


def self_test():
    """The audit must reject a newer-glibc need and an outside dependency."""
    def cc(*args):
        subprocess.run(['gcc', '-fPIC', *args], check=True)

    with tempfile.TemporaryDirectory() as tmp:
        tmp = Path(tmp)
        (tmp / 'stub.c').write_text('int convt_stub(void) { return 1; }\n')
        (tmp / 'use.c').write_text('int convt_stub(void);\nint convt_use(void) { return convt_stub(); }\n')
        (tmp / 'main.c').write_text('int main(void) { return 0; }\n')
        (tmp / 'glibc.map').write_text('GLIBC_2.99 { global: convt_stub; local: *; };\n')

        def case(label, build, expect):
            root = tmp / label
            (root / 'lib').mkdir(parents=True)
            build(root)
            violations, _ = audit(root, [root / 'lib'])
            hit = [v for v in violations if expect and expect in v]
            if expect and not hit:
                sys.exit(f'self-test {label}: expected a violation containing {expect!r}, got {violations}')
            if not expect and violations:
                sys.exit(f'self-test {label}: expected no violations, got {violations}')
            print(f'self-test {label}: ' + (hit[0] if expect else 'clean'))

        def clean(root):
            cc('-shared', '-nostdlib', '-o', root / 'lib/libconvt_stub.so', tmp / 'stub.c')
            cc('-shared', '-nostdlib', '-o', root / 'use.so', tmp / 'use.c', '-L', root / 'lib', '-lconvt_stub')

        def newer_glibc(root):
            # A version node named GLIBC_2.99 is exactly what a binary linked
            # against a newer glibc records as a need.
            cc('-shared', '-nostdlib', '-Wl,--version-script=' + str(tmp / 'glibc.map'),
               '-o', root / 'lib/libconvt_stub.so', tmp / 'stub.c')
            cc('-shared', '-nostdlib', '-o', root / 'use.so', tmp / 'use.c', '-L', root / 'lib', '-lconvt_stub')

        def outside(root):
            elsewhere = root.parent / 'outside'
            elsewhere.mkdir()
            cc('-shared', '-nostdlib', '-o', elsewhere / 'libconvt_stub.so', tmp / 'stub.c')
            cc('-shared', '-nostdlib', '-o', root / 'use.so', tmp / 'use.c', '-L', elsewhere,
               '-Wl,-rpath,' + str(elsewhere), '-lconvt_stub')

        def missing(root):
            elsewhere = root.parent / 'gone'
            elsewhere.mkdir()
            cc('-shared', '-nostdlib', '-o', elsewhere / 'libconvt_gone.so', tmp / 'stub.c')
            cc('-shared', '-nostdlib', '-o', root / 'use.so', tmp / 'use.c', '-L', elsewhere, '-lconvt_gone')

        def this_libc(root):
            cc('-o', root / 'main', tmp / 'main.c')

        case('clean', clean, None)
        case('newer-glibc', newer_glibc, 'needs GLIBC_2.99')
        case('unbundled', outside, 'unbundled dependency libconvt_stub.so')
        case('unresolved', missing, 'unresolved libconvt_gone.so')
        # A program linked against this machine's glibc passes only when that
        # glibc is at or below the baseline, as in the builder.
        libc = os.confstr('CS_GNU_LIBC_VERSION').split()[1]
        newer = tuple(map(int, libc.split('.')[:2])) > BASELINE
        case('host-glibc-' + libc, this_libc, 'above the 2.28 baseline' if newer else None)


def main(argv):
    if argv == ['--self-test']:
        self_test()
        return
    root = Path(argv[0]).resolve()
    libs, report, rest = [], None, argv[1:]
    while rest:
        flag, value, rest = rest[0], rest[1], rest[2:]
        if flag == '--lib':
            libs.append((root / value).resolve())
        elif flag == '--report':
            report = Path(value)
        else:
            sys.exit(f'unknown option {flag}')
    violations, receipts = audit(root, libs or [root / 'lib'])
    if report:
        report.write_text('\n'.join(receipts))
    if violations:
        sys.exit('ELF audit failed:\n' + '\n'.join(violations))
    print(f'ELF audit passed: {len(receipts)} ELF files under {root}')


if __name__ == '__main__':
    main(sys.argv[1:])
