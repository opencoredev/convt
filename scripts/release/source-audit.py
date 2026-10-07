#!/usr/bin/env python3
"""Validate and retain the exact source closures associated with a Linux payload."""
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile


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


def mac_gaps(tree):
    """What still blocks Mac artifacts, from the Mac owner's status file. A
    missing file blocks, a not-ready file always yields a gap, and a ready
    file may list none."""
    path=tree/'packaging/macos/release-status.json'
    if not path.exists():return ['packaging/macos/release-status.json is missing']
    status=json.loads(path.read_text())
    gaps=[str(g) for g in status.get('gaps',[])]
    if status.get('distribution_ready') is True:
        if gaps:raise ValueError('packaging/macos/release-status.json claims readiness with open gaps')
        return []
    return gaps or ['packaging/macos/release-status.json is not ready but lists no gap']

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
        # Fetch locked inputs first — a clean runner never populated
        # packaging/.cache/native-source (same class of bug as appimage-source).
        extra=(['--payload',str(payload)] if subdir=='baseline' else ['--runtime-file',str(cache/'runtime-source-built-x86_64')])
        subprocess.run([sys.executable,str(tree/'packaging/release'/script),'fetch','--lock',str(lock),'--cache',str(cache),'--rpm-image',image],check=True)
        result=subprocess.run([sys.executable,str(tree/'packaging/release'/script),'collect','--source-only','--lock',str(lock),'--cache',str(cache),'--output',str(destination/subdir),'--rpm-image',image]+extra,check=True)
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
    platform_gaps={'macos-arm64':mac_gaps(tree)}
    # The Mac bundle uses its own source-built FFmpeg 9.0.2. Retain that
    # build's exact sources and recipe before the Mac platform can count as covered.
    if not platform_gaps['macos-arm64']:
        spec=importlib.util.spec_from_file_location('mac_source_verifier',tree/'packaging/release/pdfium-source-verify.py')
        mac=importlib.util.module_from_spec(spec);spec.loader.exec_module(mac)
        mac.CACHE=cache
        build=json.loads((tree/'packaging/release/macos-source-ffmpeg.lock.json').read_text())['source_build_alternative']
        if sha(tree/build['build_recipe']['path'])!=build['build_recipe']['sha256']:raise ValueError('macOS FFmpeg build recipe differs from its lock')
        for item in build['sources']:
            path=mac.cache_path(item)
            if not path.exists():mac.fetch(item,path)
            mac.verify(item,path)
            sources.append(retain(tree,cache,destination/'macos-ffmpeg',item,'cache_filename'))
        checks.append('macOS arm64 FFmpeg 9.0.2 source build: every pinned source archive and the hash-pinned recipe retained')
    # Runs before the Linux derivation removes crates outside the Linux graph.
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
    # The archive keeps only the Linux crate graph, so the Mac-only part of the
    # app's graph is absent from it. Any copyleft crate there leaves macOS
    # without its corresponding source.
    def crate_graph(target):
        with tempfile.TemporaryDirectory(prefix='convt-empty-cargo-') as home:
            result=subprocess.run(['cargo','tree','--offline','--locked','-p','convt-cli','-p','convt-app','--target',target,
                                   '-e','normal,build','--prefix','none','-f','{p}'],cwd=tree,check=True,capture_output=True,text=True,
                                  env={**os.environ,'CARGO_HOME':home})
        return {tuple(line.split(' (')[0].replace(' v',' ',1).split()) for line in result.stdout.splitlines() if line.strip()}
    licences={(c['name'],c['version']):c['license'] for c in json.loads((tree/'third-party/rust-license-inventory-aarch64-apple-darwin.json').read_text())['source_inventory']}
    def copyleft(expression):
        return all(re.search(r'GPL|MPL|EPL|CDDL|OSL|NOASSERTION',alt) for alt in re.split(r'\s+OR\s+|/',expression.strip('() ')))
    mac_only=crate_graph('aarch64-apple-darwin')-crate_graph('x86_64-unknown-linux-gnu')
    blocked=sorted(f'{n} {v} ({licences.get((n,v),"NOASSERTION")})' for n,v in mac_only if copyleft(licences.get((n,v),'NOASSERTION')))
    if blocked:platform_gaps['macos-arm64'].append('Mac-only copyleft or undeclared Rust crates absent from the Linux source archive: '+', '.join(blocked))
    checks.append(f'{len(mac_only)} Mac-only Rust crates outside the Linux source archive are permissively licensed')
    subprocess.run([sys.executable,str(tree/'scripts/release/linux-source-scope.py'),str(tree)],cwd=tree,check=True)
    checks.append('Linux Cargo unit graph, features, pins and source bytes preserved by archived metadata derivation')
    inventory=tree/'third-party/rust-license-inventory.json'
    subprocess.run([sys.executable,str(tree/'packaging/linux/rust-license-map.py'),str(inventory),'--vendor-dir',str(tree/'third-party/rust'),'--target','x86_64-unknown-linux-gnu','--require-complete'],cwd=tree,check=True)
    subprocess.run(['bash',str(tree/'scripts/release/rebuild-linux-source.sh'),str(tree)],cwd=tree,check=True,timeout=1800,env={**os.environ,'CONVT_BUNDLE_CACHE':str(cache),'SOURCE_DATE_EPOCH':str(epoch)})
    checks.append('Derived CLI and app release build passed with empty Cargo/target caches, network disabled, and real SVG conversion')
    rust=json.loads(inventory.read_text())
    if any(not c['notices'] for c in rust['source_inventory']):raise ValueError('Vendored Rust sources lack complete notices')
    checks.append('Complete vendored Linux Rust source licence inventory, including build dependencies')
    return {'sources':sources,'checks':checks,'gaps':gaps,
            'rust_inventory':'third-party/rust-license-inventory.json',
            'rust_inventories': ['third-party/rust-license-inventory-aarch64-apple-darwin.json',
                                 'third-party/rust-license-inventory-x86_64-apple-darwin.json'],
            'platform_gaps':platform_gaps}
