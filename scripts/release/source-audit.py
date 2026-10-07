#!/usr/bin/env python3
"""Validate and retain the exact source closures associated with a Linux payload."""
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys


def sha(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream,'sha256').hexdigest()


def retain(tree, cache, destination, item, cache_field='name'):
    relative=Path(item[cache_field])
    if relative.is_absolute() or '..' in relative.parts:raise ValueError('Unsafe source cache path')
    source=cache/relative
    if sha(source)!=item['sha256']:raise ValueError('Missing or corrupt corresponding source: '+str(relative))
    target=destination/relative;target.parent.mkdir(parents=True,exist_ok=True)
    shutil.copyfile(source,target)
    return {'path':str(target.relative_to(tree)),'sha256':item['sha256'],'url':item['url']}


def status_gaps(tree, relative):
    """What still blocks a platform's artifacts, from its owner's status file.
    A missing file blocks, a not-ready file always yields a gap, and a ready
    file may list none."""
    path=tree/relative
    if not path.exists():return [f'{relative} is missing']
    status=json.loads(path.read_text())
    gaps=[str(g) for g in status.get('gaps',[])]
    if status.get('distribution_ready') is True:
        if gaps:raise ValueError(f'{relative} claims readiness with open gaps')
        return []
    return gaps or [f'{relative} is not ready but lists no gap']

def mac_gaps(tree):
    return status_gaps(tree,'packaging/macos/release-status.json')

def windows_gaps(tree):
    """Windows also needs its build lock to be ready. The windows source
    archive itself is checked against the locks by assemble.py."""
    gaps=status_gaps(tree,'packaging/windows/release-status.json')
    lock=json.loads((tree/'packaging/windows/inputs.lock.json').read_text())
    if lock.get('distribution_ready') is not True or lock.get('blockers'):
        gaps+=[str(b) for b in lock.get('blockers',[])] or ['packaging/windows/inputs.lock.json is not ready']
    return gaps

def collect(tree, payload, cache, epoch):
    destination=tree/'third-party/native';destination.mkdir(parents=True,exist_ok=True)
    sources=[];gaps=[];checks=[]
    # The disabled-autodetect FFmpeg build has only these explicit dependencies.
    inputs=json.loads((tree/'packaging/linux/ffmpeg-source-inputs.lock.json').read_text())
    receipt=json.loads((payload/'licenses/ffmpeg-source-inputs.lock.json').read_text())
    if inputs!=receipt:raise ValueError('FFmpeg corresponding-source receipt differs from actual build')
    for i in inputs:sources.append(retain(tree,cache,destination/'ffmpeg',i))
    for name in ['build-ffmpeg.sh','container-build.sh']:
        if not (tree/'packaging/linux'/name).is_file():raise ValueError('Missing native build recipe: '+name)
    subprocess.run([sys.executable,str(tree/'packaging/linux/license-metadata.py'),str(payload),str(tree/'third-party/payload-license-inventory'),'--release'],cwd=tree,check=True)
    subprocess.run([sys.executable,str(tree/'scripts/release/audit-artifact-notices.py'),str(payload.parent)],cwd=tree,check=True)
    checks.append('Actual tarball and AppImage retain every audited payload notice')
    checks.append('source-built FFmpeg receipt and all pinned input hashes')
    # Preserve the already validated distro original archives and patches.
    for i in json.loads((tree/'packaging/linux/inputs.lock.json').read_text()):
        if '.orig.tar.' in i['name'] or '.debian.tar.' in i['name'] or i['name']=='gcc-15.2.0.tar.gz':
            sources.append(retain(tree,cache,destination/'codecs',i))
    image=json.loads((tree/'packaging/linux/build-image.json').read_text())['image']
    for filename,script,subdir in [('native-sources.lock.json','native-source-tools.py','baseline'),('appimage-source-closure.lock.json','appimage-source-closure.py','appimage')]:
        lock=tree/'packaging/release'/filename
        if not lock.exists():gaps.append('Missing closure lock: '+filename);continue
        data=json.loads(lock.read_text())
        result=subprocess.run([sys.executable,str(tree/'packaging/release'/script),'collect','--source-only','--lock',str(lock),'--cache',str(cache),'--output',str(destination/subdir),'--rpm-image',image]+(['--payload',str(payload)] if subdir=='baseline' else ['--runtime-file',str(cache/'runtime-source-built-x86_64')]),check=True)
        if not data['closure_complete'] or data.get('blockers'):gaps.extend(data.get('blockers') or ['Incomplete closure: '+filename])
        checks.append(filename+': source/recipe/notice hashes and binary associations validated')
    runtime_lock=json.loads((tree/'packaging/release/appimage-source-closure.lock.json').read_text())
    runtime=cache/'appimage-source/rebuilt/runtime-x86_64'
    if sha(runtime)!=runtime_lock['build']['runtime_sha256']:raise ValueError('Runtime build receipt hash mismatch')
    image_input=next(i for i in json.loads((tree/'packaging/linux/appimage-inputs.lock.json').read_text()) if i.get('built_by'))
    if image_input['sha256']!=sha(runtime):raise ValueError('Runtime build/source pin mismatch')
    # appimagetool changes only the ELF .digest_md5 section before appending SquashFS.
    import re
    sections=subprocess.check_output(['readelf','-SW',str(runtime)],text=True)
    match=re.search(r'\.digest_md5\s+PROGBITS\s+\S+\s+(\S+)\s+(\S+)',sections)
    if not match:raise ValueError('Runtime MD5 section absent')
    offset,size=[int(v,16) for v in match.groups()]
    prefix=bytearray((payload.parent/'convt-linux-x86_64.AppImage').read_bytes()[:runtime.stat().st_size])
    prefix[offset:offset+size]=bytes(size)
    if bytes(prefix)!=runtime.read_bytes():raise ValueError('AppImage does not embed the audited source-built runtime')
    checks.append('Actual AppImage embeds the pinned source-built runtime')
    # PDFium uses its exact release attestation and DEPS checkout, not a version-adjacent tarball.
    lock=tree/'packaging/release/pdfium-source.lock.json'
    if not lock.exists():gaps.append('Missing exact PDFium source lock')
    else:
        spec=importlib.util.spec_from_file_location('pdfium_source_verifier',tree/'packaging/release/pdfium-source-verify.py')
        module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
        module.CACHE=cache
        data=json.loads(lock.read_text())
        for i in module.artifacts(data):
            module.verify(i,module.cache_path(i))
        # General-purpose compiler/SDK/CIPD binaries stay declared prerequisites.
        # Archive the exact linked source checkout and its executed builder scripts.
        for i in module.artifacts({'sources':data['sources'],'build_recipe':data['build_recipe'],'attestation':data.get('attestation',{})}):
            sources.append(retain(tree,cache,destination/'pdfium',i,'cache_filename'))
        module.verify_notice_reproduction(data)
        binary=next(b for b in data['binary_associations'] if 'pdfium-linux-x64.tgz' in b['url'])
        pinned=next(i for i in json.loads((tree/'packaging/linux/inputs.lock.json').read_text()) if i['name']=='pdfium.tgz')
        if binary['sha256']!=pinned['sha256']:raise ValueError('PDFium source association differs from binary pin')
        import tarfile
        native_spec=importlib.util.spec_from_file_location('native_source_verifier',tree/'packaging/release/native-source-tools.py')
        native=importlib.util.module_from_spec(native_spec);native_spec.loader.exec_module(native)
        with tarfile.open(cache/'pdfium.tgz') as archive:
            pdfium=archive.extractfile('lib/libpdfium.so').read()
        expected=native.stripped_digest(pdfium,cache,image)
        if sha(payload/'lib/libpdfium.so')!=expected:raise ValueError('Actual PDFium ELF differs from pinned release after builder stripping')
        checks.append('Actual PDFium ELF hash matches pinned release and builder strip operation')
        # The lock explicitly classifies source gaps separately from untested build environments.
        gaps.extend(g['detail'] if isinstance(g,dict) else g for g in data.get('source_gaps',data.get('remaining_gaps',[])))
        checks.append('PDFium exact revision, dependency archive and notice reproduction hashes')
    platform_gaps={'macos-arm64':mac_gaps(tree),'windows-x86_64':windows_gaps(tree)}
    subprocess.run([sys.executable,str(tree/'scripts/release/linux-source-scope.py'),str(tree)],cwd=tree,check=True)
    checks.append('Linux Cargo unit graph, features, pins and source bytes preserved by archived metadata derivation')
    inventory=tree/'third-party/rust-license-inventory.json'
    subprocess.run([sys.executable,str(tree/'packaging/linux/rust-license-map.py'),str(inventory),'--vendor-dir',str(tree/'third-party/rust'),'--target','x86_64-unknown-linux-gnu','--require-complete'],cwd=tree,check=True)
    subprocess.run(['bash',str(tree/'scripts/release/rebuild-linux-source.sh'),str(tree)],cwd=tree,check=True,timeout=1800,env={**os.environ,'CONVT_BUNDLE_CACHE':str(cache),'SOURCE_DATE_EPOCH':str(epoch)})
    checks.append('Derived CLI and app release build passed with empty Cargo/target caches, network disabled, and real SVG conversion')
    rust=json.loads(inventory.read_text())
    if any(not c['notices'] for c in rust['source_inventory']):raise ValueError('Vendored Rust sources lack complete notices')
    checks.append('Complete vendored Linux Rust source licence inventory, including build dependencies')
    # macOS has a different Cargo feature graph. Keep one target-specific
    # inventory for each slice beside the archive and verify that the known
    # SDK-derived objc2 sources are present with their recorded provenance.
    mac_inventory = []
    blocker_rows = json.loads((tree/'packaging/release/rust-notice-blockers.json').read_text())['blockers']
    for target in ('aarch64-apple-darwin', 'x86_64-apple-darwin'):
        output = tree / f'third-party/rust-license-inventory-{target}.json'
        subprocess.run([sys.executable, str(tree/'packaging/linux/rust-license-map.py'), str(output),
                        '--vendor-dir', str(tree/'third-party/rust'), '--target', target], cwd=tree, check=True)
        data = json.loads(output.read_text())
        if data.get('targets') != [target]:
            raise ValueError(f'macOS Rust inventory target mismatch: {target}')
        inventory = {(p['name'], p['version'], p['manifest_sha256']): p
                     for p in data.get('source_inventory', [])}
        missing = []
        for blocker in blocker_rows:
            identity = (blocker['name'], blocker['version'], blocker['manifest_sha256'])
            record = inventory.get(identity)
            if record is None:
                missing.append(f"{blocker['name']} {blocker['version']}")
                continue
            expected_targets = set(blocker.get('targets', []))
            if target in expected_targets and target not in record.get('targets', []):
                missing.append(f"{blocker['name']} {blocker['version']} (target graph)")
        if missing:
            raise ValueError(f'macOS Rust inventory omits SDK-derived crates for {target}: {", ".join(sorted(missing))}')
        mac_inventory.append(str(output.relative_to(tree)))
    checks.append('Target-specific macOS Rust inventories retain both Apple targets and SDK-derived objc2 sources')
    return {'sources':sources,'checks':checks,'gaps':gaps,
            'rust_inventory':'third-party/rust-license-inventory.json',
            'rust_inventories': ['third-party/rust-license-inventory-aarch64-apple-darwin.json',
                                 'third-party/rust-license-inventory-x86_64-apple-darwin.json'],
            'platform_gaps':platform_gaps}
