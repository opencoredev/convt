#!/usr/bin/env python3
"""Audit installed notice inputs and generate deterministic package metadata.

This is a notice inventory, not a declaration of distribution readiness.
Unresolved static FFmpeg and Rust dependencies remain explicit LicenseRefs.
"""
import argparse
import hashlib
import json
from pathlib import Path
import re
import shutil
import subprocess
import sys
import traceback

HERE = Path(__file__).resolve().parent


def notice_text(path):
    data = path.read_bytes()
    try:
        return data.decode('utf-8')
    except UnicodeDecodeError:
        return data.decode('latin-1')


def generate(root, output, release=False):
    rust = json.loads((HERE / 'rust-license-components.lock.json').read_text())
    receipt = root / 'licenses/cargo-lock.sha256'
    if not receipt.is_file():
        raise ValueError('Missing payload Cargo.lock receipt: licenses/cargo-lock.sha256; capture it from the actual source build, never infer it for an existing payload')
    payload_lock = receipt.read_text().strip()
    source_lock = hashlib.sha256((HERE.parent.parent / 'Cargo.lock').read_bytes()).hexdigest()
    if payload_lock != rust['cargo_lock_sha256'] or source_lock != rust['cargo_lock_sha256']:
        raise ValueError('Cargo.lock mismatch between current source, payload receipt and Rust notice map; regenerate the map against the exact payload build snapshot')
    subprocess.run([sys.executable, str(HERE / 'collect-native-notices.py'), str(root)], check=True)
    components = json.loads((HERE / 'license-components.lock.json').read_text())
    native = json.loads((HERE / 'native-notices.lock.json').read_text())
    for item in [entry for main in native for entry in [main] + main.get('supplements', [])]:
        target = root / 'licenses/native' / (item['name'] + '.txt')
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(HERE / item['notice'], target)
    covered = set()
    for item in components:
        matches = {p for pattern in item['files'] for p in root.glob(pattern) if p.is_file()}
        if not matches:
            raise ValueError(f'Shipped component missing: {item["name"]}')
        covered.update(p.resolve() for p in matches)
        for notice, digest in item['notice_sha256'].items():
            path = root / notice
            if not path.is_file() or not path.stat().st_size:
                raise ValueError(f'Missing notice for {item["name"]}: {notice}')
            if hashlib.sha256(path.read_bytes()).hexdigest() != digest:
                raise ValueError(f'Notice hash mismatch for {item["name"]}: {notice}')
    rust_missing = []
    for crate in rust['packages']:
        ident = re.sub(r'[^A-Za-z0-9.-]', '-', crate['name'] + '-' + crate['version'])
        notice_path = 'licenses/rust/' + ident + '.txt'
        target = root / notice_path
        target.parent.mkdir(parents=True, exist_ok=True)
        sections = []
        for notice in crate['notices']:
            if hashlib.sha256(notice['text'].encode()).hexdigest() != notice['sha256']:
                raise ValueError('Corrupt Rust notice lock: ' + ident)
            sections.append('Source notice: ' + notice['source_path'] + '\n' + notice['text'])
        license = re.sub(r'\s*/\s*', ' OR ', crate['license'])
        if not sections:
            rust_missing.append(ident)
            sections = ['INCOMPLETE: ' + ident + ' declares ' + license + ' but cached source has no separate license/notice file. Obtain the exact upstream notice and source holder statements before release.']
            license = '(' + license + ') AND LicenseRef-Unresolved-Rust-' + ident
        target.write_text('\n\n'.join(sections))
        components.append({'name': 'Rust/' + crate['name'], 'version': crate['version'], 'files': ['convt.bin', 'convt-app.bin'], 'license': license, 'holders': crate['holders'], 'notices': [notice_path], 'status': crate['status']})
    # Every ELF in the installed payload must belong to a reviewed component.
    for path in root.rglob('*'):
        if path.is_file():
            with path.open('rb') as stream:
                is_elf = stream.read(4) == b'\x7fELF'
            if is_elf and path.resolve() not in covered:
                raise ValueError(f'No license component mapping for shipped ELF: {path.relative_to(root)}')
    configuration = (root / 'licenses/ffmpeg-configuration.txt').read_text()
    expected = json.loads((HERE / 'ffmpeg-static-components.lock.json').read_text())
    flags = set(re.findall(r'--enable-(lib[\w-]+|fontconfig|frei0r|gnutls|gmp)', configuration))
    if flags != {c['configure_flag'] for c in expected if c['configure_flag']} or '--disable-autodetect' not in configuration:
        raise ValueError('FFmpeg dependency flags differ from reviewed source-built inventory')
    if not configuration.startswith('ffmpeg version 7.0.2'):
        raise ValueError('FFmpeg version differs from pinned source')
    source_inputs = json.loads((HERE / 'ffmpeg-source-inputs.lock.json').read_text())
    if json.loads((root / 'licenses/ffmpeg-source-inputs.lock.json').read_text()) != source_inputs:
        raise ValueError('FFmpeg build/source input receipt mismatch')
    gaps = ('Rust crates without complete notices: ' + ', '.join(rust_missing)) if rust_missing else 'Pinned source-built FFmpeg closure and Rust notices included. Corresponding-source archive audit is a separate required publication gate.'
    if release and rust_missing:
        raise ValueError(gaps)
    output.mkdir(parents=True, exist_ok=True)
    terms = sorted({c['license'] for c in components} | ({'LicenseRef-Unresolved-Rust-dependencies'} if rust_missing else set()))
    aggregate = ' AND '.join('(' + term + ')' if ' AND ' in term or ' OR ' in term else term for term in terms)
    (output / 'rpm-license.txt').write_text(aggregate + '\n')
    shutil.copyfile(HERE / 'rust-license-components.lock.json', output / 'rust-license-components.json')
    (output / 'ffmpeg-static-components.json').write_text(json.dumps(expected, indent=2) + '\n')
    summary = ['Linux payload license component summary', '', 'Aggregate SPDX expression (includes unresolved LicenseRefs):', aggregate, '', gaps, '']
    dep5 = ['Format: https://www.debian.org/doc/packaging-manuals/copyright-format/1.0/', 'Upstream-Name: convt', 'Source: https://convt.app', 'Comment: ' + gaps, '']
    packages = []
    extracted = {}
    license_texts = {}
    for i, c in enumerate(components):
        summary.extend([c['name'] + ': ' + c['version'], '  Notice copyright statements: ' + '; '.join(c['holders']), '  License: ' + c['license'], '  Notices: ' + ', '.join('/opt/convt/' + n for n in c['notices']), ''])
        # Use per-component DEP-5 local names so full variant notices stay intact.
        label = 'component-' + str(i + 1)
        text = '\n\n'.join(notice_text(root / n) for n in c['notices'])
        license_texts[label] = text if not re.search(r'Apache License|GNU (?:GENERAL|LESSER|LIBRARY) PUBLIC LICENSE', text, re.I) else ('SPDX expression: ' + c['license'] + '\n'
                               'The complete upstream terms and notices are installed in:\n' +
                               '\n'.join('/opt/convt/' + n for n in c['notices']) + '\n'
                               'Standard license texts, where applicable, are available under '
                               '/usr/share/common-licenses (Apache-2.0, GPL-2, GPL-3, LGPL-2.1, LGPL-3).')
        dep5.extend(['Files: ' + ' '.join('opt/convt/' + f for f in c['files']), 'Copyright: ' + '\n '.join(c['holders']), 'License: ' + label, 'Comment: SPDX expression: ' + c['license'], ' Notices: ' + ', '.join('/opt/convt/' + n for n in c['notices']), ' Scope: ' + c['status'] + '; notice copyright statements may include license document authors; source-header holder audit incomplete.', ''])
        packages.append({'SPDXID': 'SPDXRef-Component-' + str(i + 1), 'name': c['name'], 'versionInfo': c['version'], 'downloadLocation': 'NOASSERTION', 'filesAnalyzed': False, 'licenseConcluded': 'NOASSERTION', 'licenseDeclared': c['license'], 'copyrightText': '\n'.join(c['holders']), 'comment': 'Reviewed notice terms; per-file binary linkage not independently established. Notice paths: ' + ', '.join(c['notices'])})
        for ref in re.findall(r'LicenseRef-[A-Za-z0-9.-]+', c['license']):
            extracted[ref] = text
    for name in (['Rust-dependencies'] if rust_missing else []):
        ref = 'LicenseRef-Unresolved-' + name
        extracted[ref] = gaps
        dep5.extend(['Files: opt/convt/ffmpeg opt/convt/ffprobe' if name == 'FFmpeg-static' else 'Files: opt/convt/convt.bin opt/convt/convt-app.bin', 'Copyright: NOASSERTION (dependency holder inventory incomplete)', 'License: ' + ref, 'Comment: ' + gaps, ''])
        license_texts[ref] = gaps
        packages.append({'SPDXID': 'SPDXRef-Unresolved-' + name, 'name': 'Remaining audit gaps for ' + name, 'downloadLocation': 'NOASSERTION', 'filesAnalyzed': False, 'licenseConcluded': ref, 'licenseDeclared': 'NOASSERTION', 'copyrightText': 'NOASSERTION', 'comment': gaps})
    for label, text in license_texts.items():
        dep5.extend(['License: ' + label] + [' ' + (line if line else '.') for line in text.splitlines()] + [''])
    (output / 'copyright').write_text('\n'.join(dep5))
    (output / 'component-summary.txt').write_text('\n'.join(summary))
    spdx = {'spdxVersion': 'SPDX-2.3', 'dataLicense': 'CC0-1.0', 'SPDXID': 'SPDXRef-DOCUMENT', 'name': 'convt Linux payload notice audit', 'documentNamespace': 'https://convt.app/spdx/linux-notices-' + hashlib.sha256(json.dumps(components, sort_keys=True).encode()).hexdigest(), 'creationInfo': {'creators': ['Tool: convt-license-metadata'], 'created': '2026-10-04T00:00:00Z'}, 'documentDescribes': [p['SPDXID'] for p in packages], 'packages': packages, 'hasExtractedLicensingInfos': [{'licenseId': ref, 'extractedText': text} for ref, text in sorted(extracted.items())], 'comment': gaps + '\nAggregate: ' + aggregate}
    (output / 'components.spdx.json').write_text(json.dumps(spdx, indent=2, ensure_ascii=False) + '\n')
    print(f'Generated metadata for {len(components)} component records; source archive publication gate remains required')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('payload', type=Path)
    parser.add_argument('output', type=Path)
    parser.add_argument('--release', action='store_true', help='require distribution readiness (fails while gaps remain)')
    args = parser.parse_args()
    try:
        generate(args.payload.resolve(), args.output.resolve(), args.release)
    except (ValueError, OSError, KeyError, subprocess.CalledProcessError) as error:
        traceback.print_exc()
        sys.exit(f'{Path(__file__).name}: {error}')
