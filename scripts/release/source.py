#!/usr/bin/env python3
"""Freeze the working tree and build a deterministic corresponding-source archive.

Unresolved vendor source is recorded explicitly. No local verification archive
may be published while the audit lists a gap.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import shutil
import subprocess
import sys
import stat
import tarfile
import traceback

REPO = Path(__file__).resolve().parents[2]
def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def snapshot(destination):
    destination.mkdir(parents=True, exist_ok=False)
    names = subprocess.check_output(['git', 'ls-files', '-z', '--cached', '--others', '--exclude-standard'], cwd=REPO).decode().split('\0')
    for name in sorted(set(filter(None, names))):
        relative = Path(name)
        if any(p in ('.git', 'node_modules', '__pycache__', 'target', '.convt-dev', '.wrangler', '.cache', 'out') for p in relative.parts):
            continue
        if relative.name.startswith(('.env', '.dev.vars')) and not relative.name.endswith('.example'):
            continue
        path = REPO / relative
        if not path.exists():
            continue
        target = destination / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        if path.is_symlink():
            if not path.resolve().is_relative_to(REPO):
                raise ValueError('external source symlink: ' + name)
            target.symlink_to(os.readlink(path))
        elif path.is_file():
            shutil.copy2(path, target)
    # A live shared checkout can change, so all subsequent stages use this copy.
    subprocess.run(['python3', 'packaging/linux/rust-license-map.py', 'packaging/linux/rust-license-components.lock.json', '--target', 'x86_64-unknown-linux-gnu'], cwd=destination, check=True)
    files = {str(p.relative_to(destination)): sha(p) for p in sorted(destination.rglob('*')) if p.is_file() and not p.is_symlink()}
    links={str(p.relative_to(destination)):os.readlink(p) for p in sorted(destination.rglob('*')) if p.is_symlink()}
    modes={str(p.relative_to(destination)):stat.S_IMODE(p.lstat().st_mode) for p in sorted(destination.rglob('*'))}
    identity=hashlib.sha256(json.dumps({'files':files,'symlinks':links,'modes':modes},sort_keys=True).encode()).hexdigest()
    (destination / 'release-tree.json').write_text(json.dumps({'tree_sha256': identity, 'files': files, 'symlinks':links, 'modes':modes}, indent=2)+'\n')
    print('Frozen source tree: ' + identity)

def check(tree):
    receipt=json.loads((tree/'release-tree.json').read_text())
    for name,digest in receipt['files'].items():
        if sha(tree/name)!=digest:raise ValueError('Frozen tree changed: '+name)
    links={str(p.relative_to(tree)):os.readlink(p) for p in tree.rglob('*') if p.is_symlink()}
    if links!=receipt.get('symlinks',{}):raise ValueError('Frozen source symlinks changed')
    modes={str(p.relative_to(tree)):stat.S_IMODE(p.lstat().st_mode) for p in tree.rglob('*') if str(p.relative_to(tree))!='release-tree.json'}
    if modes!=receipt.get('modes',{}):raise ValueError('Frozen source modes or directory layout changed')
    actual={str(p.relative_to(tree)) for p in tree.rglob('*') if p.is_file() and not p.is_symlink()}
    if actual-set(receipt['files'])-{'release-tree.json'}:raise ValueError('Unexpected files in frozen tree')

def source_archive_build_output_members(names, root):
    """Names that are Cargo or Python build output, not corresponding source."""
    bad=[]
    for name in names:
        parts=PurePosixPath(name).parts
        if not parts:
            continue
        if '__pycache__' in parts:
            bad.append(name)
            continue
        if parts[0]==root and len(parts)>1 and parts[1]=='target':
            bad.append(name)
            continue
        if 'target' in parts and any(p in {'.rustc_info.json','.fingerprint','incremental'} for p in parts):
            bad.append(name)
    return bad

def refuse_build_output_in_archive(archive_path, root):
    with tarfile.open(archive_path) as archive:
        names=[member.name for member in archive.getmembers()]
    bad=source_archive_build_output_members(names, root)
    if bad:
        preview=', '.join(bad[:8])
        extra=f' (+{len(bad)-8} more)' if len(bad)>8 else ''
        raise ValueError(f'source archive contains build output ({len(bad)} members): {preview}{extra}')
    return names

def pack_source_archive(tree, archive_path, epoch):
    """Write the public source checkout without audit staging data.

    The audit runs in the frozen tree and materializes corresponding native
    sources, Cargo inventories, and other evidence under ``third-party``.
    Those generated inputs are validated through ``source-audit.json`` but are
    not part of the public checkout archive. Including them made the archive
    repeat the separately delivered native/document inputs and pushed the
    v0.3.0 asset over a gigabyte.
    """
    tree=tree.resolve()
    tar=subprocess.Popen(['tar','--sort=name',f'--mtime=@{epoch}','--owner=0','--group=0','--numeric-owner',
                            '--exclude=*/__pycache__','--exclude=*/.cache',
                            # Source-audit evidence and fetched corresponding
                            # sources are validated separately and must not be
                            # copied into the compact source checkout archive.
                            f'--exclude={tree.name}/third-party',
                            # Build output from audits run inside the tree is not source.
                            f'--exclude={tree.name}/target','-C',str(tree.parent),'-cf','-',tree.name], stdout=subprocess.PIPE)
    with Path(archive_path).open('wb') as stream:
        gzip=subprocess.run(['gzip','-n'], stdin=tar.stdout, stdout=stream)
    tar.stdout.close()
    if tar.wait() or gzip.returncode:
        raise ValueError('source archive failed')
    refuse_build_output_in_archive(archive_path, tree.name)

def pack_source_closure_archive(tree, archive_path, epoch):
    """Write the audited generated closure as a separate deterministic asset."""
    tree=tree.resolve()
    tar=subprocess.Popen(['tar','--sort=name',f'--mtime=@{epoch}','--owner=0','--group=0','--numeric-owner',
                            '--exclude=*/__pycache__','--exclude=*/.cache',
                            '-C',str(tree.parent),'-cf','-',f'{tree.name}/third-party'], stdout=subprocess.PIPE)
    with Path(archive_path).open('wb') as stream:
        gzip=subprocess.run(['gzip','-n'], stdin=tar.stdout, stdout=stream)
    tar.stdout.close()
    if tar.wait() or gzip.returncode:
        raise ValueError('source closure archive failed')

def archive(tree, output, version, epoch, verification):
    # Full lockfile vendoring makes the CLI rebuild independent of a host cache.
    subprocess.run(['cargo', 'vendor', '--offline', '--locked', '--versioned-dirs', 'third-party/rust'], cwd=tree, check=True, stdout=subprocess.DEVNULL)
    config = tree / '.cargo/config.toml'
    config.parent.mkdir(exist_ok=True)
    with config.open('a') as stream:
        stream.write('\n[source.crates-io]\nreplace-with = "release-vendor"\n[source.release-vendor]\ndirectory = "third-party/rust"\n')
    cache = Path(os.environ.get('CONVT_BUNDLE_CACHE', str(REPO / 'packaging/.cache'))).resolve()
    import importlib.util
    spec=importlib.util.spec_from_file_location('source_audit',tree/'scripts/release/source-audit.py')
    helper=importlib.util.module_from_spec(spec);spec.loader.exec_module(helper)
    closure=helper.collect(tree,output/'convt',cache,epoch)
    gaps=closure['gaps']
    components=json.loads((tree/'packaging/linux/license-components.lock.json').read_text())
    platform_gaps = closure.get('platform_gaps', {})
    covered_platforms = ['linux-x86_64']
    if not platform_gaps.get('macos-arm64'):
        covered_platforms.append('macos-arm64')
    audit={'schema_version':1,'distribution_ready':not gaps,
           'tree_sha256':json.loads((tree/'release-tree.json').read_text())['tree_sha256'],
           'covered_platforms':covered_platforms,'platform_gaps':platform_gaps,
           'native_sources':closure['sources'],'build_recipes':['packaging/linux/container-build.sh','packaging/linux/build-ffmpeg.sh','packaging/linux/libheif-explicit-init.patch'],
           'rust_sources':'third-party/rust','rust_inventory':closure['rust_inventory'],
           'rust_inventories':closure.get('rust_inventories', []),
           'validated_closures':closure['checks'],'components':components,'gaps':gaps}
    (tree/'corresponding-source.json').write_text(json.dumps(audit,indent=2)+'\n')
    (output/'source-audit.json').write_text(json.dumps(audit,indent=2)+'\n')
    # Retain the full source of every archived component. Rust crates retain
    # their original notices, source headers and Cargo checksums.
    archive_path = output/f'convt-{version}-source.tar.gz'
    pack_source_archive(tree, archive_path, epoch)
    pack_source_closure_archive(tree, output/f'convt-{version}-source-closure.tar.gz', epoch)
    if gaps and not verification:
        raise ValueError('Publication blocked: ' + '; '.join(gaps))
    print(f'Source archive: {archive_path}; {len(gaps)} publication gaps')

if __name__ == '__main__':
    p=argparse.ArgumentParser(description=__doc__)
    sub=p.add_subparsers(dest='command',required=True)
    s=sub.add_parser('snapshot');s.add_argument('destination',type=Path)
    c=sub.add_parser('check');c.add_argument('tree',type=Path)
    a=sub.add_parser('archive');a.add_argument('tree',type=Path);a.add_argument('output',type=Path);a.add_argument('version');a.add_argument('epoch',type=int);a.add_argument('--verification-only',action='store_true')
    args=p.parse_args()
    try:
        if args.command=='snapshot':snapshot(args.destination)
        elif args.command=='check':check(args.tree)
        else:archive(args.tree,args.output,args.version,args.epoch,args.verification_only)
    except (OSError,ValueError,subprocess.CalledProcessError) as error:
        traceback.print_exc()
        sys.exit(f'{Path(__file__).name}: {error}')
