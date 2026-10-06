#!/usr/bin/env python3
"""Collect offline Rust notices for Linux/macOS release graphs and source inventory.

Cargo's workspace-feature-unified normal graph is conservative, not a linker map.
The source inventory includes all resolved packages, including build/dev and other
platform dependencies. Pass --vendor-dir for an actual cargo-vendor tree, including
crates outside the resolved graph. Pinned supplements are verified before use.
"""
import argparse
import hashlib
from functools import lru_cache
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tomllib

NOTICE_NAME = re.compile(r'^(LICEN[CS]E|COPYING|COPYRIGHT|NOTICE)([._-].*)?$', re.I)
RELEASE_ROOT = Path(__file__).resolve().parents[1] / 'release'
DEFAULT_TARGETS = ('x86_64-unknown-linux-gnu', 'x86_64-apple-darwin', 'aarch64-apple-darwin')


def digest(data):
    return hashlib.sha256(data).hexdigest()


def metadata(cargo, target=None):
    scope_receipt = Path('third-party/linux-source-scope/receipt.json')
    if scope_receipt.is_file():
        linux_target = json.loads(scope_receipt.read_text())['target']
        if target and target != linux_target:
            raise ValueError('Linux source derivation cannot inventory a macOS source closure')
        target = linux_target
    command = [cargo, 'metadata', '--offline', '--locked', '--format-version', '1']
    if target:
        command += ['--filter-platform', target]
    return json.loads(subprocess.check_output(command, text=True, timeout=120))


def normal_graph(meta, roots):
    packages = {p['id']: p for p in meta['packages']}
    nodes = {n['id']: n for n in meta['resolve']['nodes']}
    todo = [p['id'] for p in packages.values() if p['name'] in roots and p['source'] is None]
    if {packages[p]['name'] for p in todo} != set(roots):
        raise ValueError('release graph root missing from Cargo metadata')
    seen = set()
    while todo:
        current = todo.pop()
        if current in seen:
            continue
        seen.add(current)
        todo.extend(d['pkg'] for d in nodes[current]['deps']
                    if any(k['kind'] is None for k in d['dep_kinds']))
    return seen


def notice_paths(directory, license_file):
    paths = set()
    if license_file:
        paths.add(directory / license_file)
    # Source redistribution needs notices in tests/examples and nested native
    # code too. Do not discard these just because the binary graph excludes them.
    for path in directory.rglob('*'):
        if path.is_file() and '.git' not in path.relative_to(directory).parts:
            parts = path.relative_to(directory).parts
            if (NOTICE_NAME.match(path.name) or any(p.lower() in ('license', 'licenses', 'licences') for p in parts[:-1])
                    or (path.name == 'AUTHORS' and b'Permission is hereby granted' in path.read_bytes())):
                paths.add(path)
    return paths


@lru_cache(maxsize=1)
def canonical_spdx():
    path = RELEASE_ROOT / 'rust-spdx-sources.json'
    if not path.exists():
        return {}
    lock = json.loads(path.read_text())
    result = {}
    for entry in lock['licenses']:
        source = (RELEASE_ROOT / entry['path']).resolve()
        if not source.is_relative_to(RELEASE_ROOT.resolve()):
            raise ValueError('SPDX source escapes release directory')
        data = source.read_bytes()
        if digest(data) != entry['sha256']:
            raise ValueError(f'SPDX source hash mismatch: {source}')
        details = json.loads(data)
        if details['licenseId'] != entry['license_id'] or digest(details['licenseText'].encode()) != entry['license_text_sha256']:
            raise ValueError(f'SPDX licence identity/text mismatch: {source}')
        result[entry['license_id']] = {'text': details['licenseText'], 'sha256': entry['license_text_sha256'],
                                     'source_path': 'reproduced-declared-SPDX/' + entry['license_id'],
                                     'provenance': {**entry, 'repository': lock['repository'], 'revision': lock['revision']},
                                     'status': 'Canonical declared SPDX terms reproduced verbatim; template placeholders are not upstream copyright notices'}
    return result


def declared_terms(expression):
    # Cargo historically accepted '/' as the dual-licence OR separator.
    normalized = re.sub(r'\s*/\s*', ' OR ', expression)
    tokens = re.findall(r'[A-Za-z0-9.+-]+|[()]', normalized)
    if ''.join(tokens) != re.sub(r'\s+', '', normalized):
        return None
    terms = canonical_spdx()
    index = 0
    identifiers = set()

    def atom():
        nonlocal index
        if index >= len(tokens):
            return False
        token = tokens[index]
        index += 1
        if token == '(':
            if not disjunction() or index >= len(tokens) or tokens[index] != ')':
                return False
            index += 1
            return True
        if token not in terms:
            # Unknown IDs, LicenseRef values and unpinned WITH exceptions
            # remain unresolved. Never guess their terms from a nearby crate.
            return False
        identifiers.add(token)
        return True

    def conjunction():
        nonlocal index
        if not atom():
            return False
        while index < len(tokens) and tokens[index] == 'AND':
            index += 1
            if not atom():
                return False
        return True

    def disjunction():
        nonlocal index
        if not conjunction():
            return False
        while index < len(tokens) and tokens[index] == 'OR':
            index += 1
            if not conjunction():
                return False
        return True

    if not disjunction() or index != len(tokens):
        return None
    return normalized, [terms[ident] for ident in sorted(identifiers)]


def collect(package, supplements):
    directory = Path(package['manifest_path']).parent
    manifest_hash = digest(Path(package['manifest_path']).read_bytes())
    original_manifest_hash = manifest_hash
    scope = Path('third-party/linux-source-scope')
    if (scope / 'receipt.json').is_file():
        receipt = json.loads((scope / 'receipt.json').read_text())
        relative = str(Path(package['manifest_path']).resolve().relative_to(Path.cwd().resolve()))
        patch = next((p for p in receipt['patches'] if p['path'] == relative), None)
        if patch:
            original = json.loads((scope / 'original-metadata.json').read_text())[relative].encode()
            if digest(original) != patch['original_sha256'] or manifest_hash != patch['derived_sha256']:
                raise ValueError('Linux manifest derivation identity mismatch: ' + relative)
            if tomllib.loads(original.decode())['package'] != tomllib.loads(Path(package['manifest_path']).read_text())['package']:
                raise ValueError('Linux derivation changed upstream package/licence metadata: ' + relative)
            original_manifest_hash = patch['original_sha256']
    notices = []
    for path in sorted(notice_paths(directory, package.get('license_file'))):
        if not path.is_file():
            continue
        data = path.read_bytes()
        try:
            content = data.decode('utf-8')
        except UnicodeDecodeError:
            continue
        if content.strip():
            notices.append({'source_path': str(path.relative_to(directory)), 'sha256': digest(data), 'text': content})
    supplement = supplements.get((package['name'], package['version']))
    if supplement:
        if supplement['manifest_sha256'] != original_manifest_hash or supplement['source'] != package['source']:
            raise ValueError(f"stale supplemental source identity: {package['name']} {package['version']}")
        vcs_path = directory / '.cargo_vcs_info.json'
        if vcs_path.exists() and supplement.get('vcs_sha1'):
            if json.loads(vcs_path.read_text())['git']['sha1'] != supplement['vcs_sha1']:
                raise ValueError(f"upstream revision mismatch: {package['name']}")
        for matched in supplement.get('source_match', []):
            source_file = directory / matched['path']
            if not source_file.resolve().is_relative_to(directory.resolve()) or digest(source_file.read_bytes()) != matched['sha256']:
                raise ValueError(f"upstream source match changed: {package['name']} {matched['path']}")
        for notice in supplement['notices']:
            path = (RELEASE_ROOT / notice['path']).resolve()
            if not path.is_relative_to(RELEASE_ROOT.resolve()):
                raise ValueError('supplemental notice escapes release directory')
            data = path.read_bytes()
            if digest(data) != notice['sha256']:
                raise ValueError(f'notice hash mismatch: {path}')
            notices.append({'source_path': 'supplemental/' + notice['upstream_path'],
                            'sha256': notice['sha256'], 'text': data.decode('utf-8'),
                            'provenance': {k: v for k, v in supplement.items() if k not in ('notices', 'source_match')},
                            'upstream_url': notice.get('url')})
    headers = []
    for path in sorted(directory.rglob('*')):
        if not path.is_file() or path.suffix.lower() not in ('.rs', '.c', '.h', '.cpp', '.hpp', '.m', '.mm', '.s'):
            continue
        data = path.read_bytes()
        try:
            text = data.decode('utf-8')
        except UnicodeDecodeError:
            continue
        # Keep original comment blocks, including notices inside nested native
        # sources. A short licence identifier alone does not close a missing file.
        blocks = [match.group(0) for match in re.finditer(r'/\*[\s\S]*?\*/|(?:^[ \t]*//[^\n]*(?:\n|$))+', text, re.M)
                  if re.search(r'copyright|permission is hereby granted|SPDX-License-Identifier|This Source Code Form|licensed under|terms of (?:the|this).*licen[sc]e', match.group(0), re.I)]
        if blocks:
            headers.append({'source_path': str(path.relative_to(directory)), 'source_sha256': digest(data),
                            'text': '\n'.join(blocks), 'sha256': digest('\n'.join(blocks).encode('utf-8'))})
    holders = sorted({line.strip(' /*#\t') for notice in notices + headers for line in notice['text'].splitlines()
                      if re.search(r'copyright\s*(?:\(c\)|©|[12][0-9]{3})', line, re.I)})
    incomplete_notices = []
    if supplement and not supplement.get('notice_complete', True):
        incomplete_notices, notices = notices, []
    declaration = None
    reproduced = False
    # Explicit upstream ambiguity overrides a valid manifest identifier.
    if not notices and not incomplete_notices and package.get('license'):
        terms = declared_terms(package['license'])
        if terms:
            manifest = Path(package['manifest_path']).read_bytes()
            upstream = tomllib.loads(manifest.decode())['package'].get('license')
            if upstream != package['license']:
                raise ValueError(f"SPDX declaration differs from manifest: {package['name']}")
            normalized, canonical = terms
            declaration = {'source_path': 'Cargo.toml', 'sha256': digest(manifest),
                           'field': 'package.license', 'expression': upstream, 'normalized_expression': normalized,
                           'text': manifest.decode('utf-8')}
            notices.append({'source_path': 'Cargo.toml (exact upstream SPDX licence declaration)',
                            'sha256': digest(manifest), 'text': manifest.decode('utf-8'),
                            'status': 'Exact upstream manifest; licence grant identifies reproduced canonical SPDX terms'})
            notices.extend(canonical)
            reproduced = True
    complete = bool(notices)
    if complete:
        notices.extend({**header, 'source_path': header['source_path'] + ' (notice comment blocks)'} for header in headers)
    return {'name': package['name'], 'version': package['version'], 'source': package['source'],
            'license': package.get('license') or 'NOASSERTION', 'manifest_sha256': manifest_hash,
            'original_manifest_sha256': original_manifest_hash,
            'holders': holders or ['NOASSERTION: no holder statement identified; inspect notices and source headers'],
            'notices': notices, 'incomplete_notices': incomplete_notices, 'source_headers': headers, 'notice_complete': complete, 'license_declaration': declaration,
            'status': ('reproduced declared SPDX terms from hash-pinned license-list-data; exact manifest and actual source notices retained; no upstream holder inferred'
                       if reproduced else 'collected source notices; manifest-declared license; per-file licence audit still required')
                      if complete else 'unresolved: source has no complete license/notice file'}


def vendor_packages(root):
    # cargo vendor puts each crate manifest beside .cargo-checksum.json.
    for checksum in sorted(root.rglob('.cargo-checksum.json')):
        manifest = checksum.parent / 'Cargo.toml'
        package = tomllib.loads(manifest.read_text())['package']
        yield {'id': 'vendor:' + str(manifest.relative_to(root)), 'name': package['name'],
               'version': package['version'], 'source': 'registry+https://github.com/rust-lang/crates.io-index'
               if json.loads(checksum.read_text()).get('package') else None,
               'manifest_path': str(manifest), 'license': package.get('license'),
               'license_file': package.get('license-file')}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('output', type=Path)
    parser.add_argument('--target', action='append', help='repeat to override the Linux/macOS target union')
    parser.add_argument('--root', action='append', help='repeat to override convt-cli/convt-app roots')
    parser.add_argument('--vendor-dir', type=Path, help='also inventory every crate in this cargo-vendor tree')
    parser.add_argument('--require-complete', action='store_true', help='fail if either inventory has a missing notice')
    args = parser.parse_args()
    cargo = shutil.which('cargo') or str(Path(os.environ.get('CARGO_HOME', Path.home() / '.cargo')) / 'bin/cargo')
    all_meta = metadata(cargo)
    packages = {p['id']: p for p in all_meta['packages']}
    targets = args.target or DEFAULT_TARGETS
    roots = args.root or ('convt-cli', 'convt-app')
    graphs = {target: normal_graph(metadata(cargo, target), roots) for target in targets}
    seen = set().union(*graphs.values())
    source_path = RELEASE_ROOT / 'rust-notice-sources.json'
    entries = json.loads(source_path.read_text())['packages'] if source_path.exists() else []
    supplements = {(p['name'], p['version']): p for p in entries}
    if len(supplements) != len(entries):
        raise ValueError('duplicate supplemental crate identity')
    vendored = []
    if args.vendor_dir:
        if not args.vendor_dir.is_dir():
            raise ValueError('vendor directory does not exist')
        vendored = list(vendor_packages(args.vendor_dir))
        if not vendored:
            raise ValueError('vendor directory contains no cargo-vendor crates')
        vendor_index = {(p['name'], p['version'], digest(Path(p['manifest_path']).read_bytes())): p for p in vendored}
        for package in packages.values():
            identity = (package['name'], package['version'], digest(Path(package['manifest_path']).read_bytes()))
            if identity in vendor_index:
                # Read the supplied distribution sources, not a potentially
                # more complete Cargo cache with the same manifest.
                package['manifest_path'] = vendor_index[identity]['manifest_path']
    inventory = []
    records = {}
    for key, package in sorted(packages.items(), key=lambda item: (item[1]['name'], item[1]['version'], item[0])):
        if package['source'] is None and package['name'].startswith('convt-'):
            continue
        record = collect(package, supplements)
        record['targets'] = sorted(t for t, graph in graphs.items() if key in graph)
        records[key] = record
        inventory.append(record)
    if args.vendor_dir:
        known = {(p['name'], p['version'], p['manifest_sha256']) for p in inventory}
        for package in vendored:
            identity = (package['name'], package['version'], digest(Path(package['manifest_path']).read_bytes()))
            if identity in known:
                continue
            record = collect(package, supplements)
            identity = (record['name'], record['version'], record['manifest_sha256'])
            if identity not in known:
                record['targets'] = []
                record['inventory_origin'] = 'additional vendored source'
                inventory.append(record)
                known.add(identity)
    result = [record for key, record in records.items() if key in seen]
    lock = {'cargo_lock_sha256': digest(Path('Cargo.lock').read_bytes()),
            'scope': 'Conservative normal transitive graph rooted at ' + ', '.join(roots) +
                     ' for ' + ', '.join(targets) + '. Excludes dev/build edges; workspace feature unification and proc macros may overinclude. This is not a per-file licence or linkage audit.',
            'targets': list(targets), 'packages': result, 'vendor_crate_count': len(vendored),
            'source_inventory_scope': 'All Cargo-resolved external/local dependency source trees, all platforms and dependency kinds, including nested notices and license directories.' +
                                      (' Also every crate in the supplied cargo-vendor tree.' if args.vendor_dir else ' No extra cargo-vendor tree supplied.'),
            'source_inventory': sorted(inventory, key=lambda p: (p['name'], p['version'], p['manifest_sha256']))}
    args.output.write_text(json.dumps(lock, indent=2, ensure_ascii=False) + '\n')
    for label, rows in [('release graph', result), ('source inventory', inventory)]:
        missing = [p for p in rows if not p['notice_complete']]
        print(f'{label}: {len(rows)} Rust components, {len(rows)-len(missing)} with notices, {len(missing)} unresolved')
        for package in missing:
            print(f"  unresolved: {package['name']} {package['version']}")
    if args.require_complete and any(not p['notices'] for p in inventory):
        raise SystemExit(1)


if __name__ == '__main__':
    main()
