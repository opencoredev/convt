#!/usr/bin/env python3
"""Derive a Linux CLI/app vendor closure in an archive copy, never a live checkout.

Run after cargo vendor and its source replacement configuration. Cargo 1.95's
unit graph is used only for evidence; the resulting build uses stable Cargo.
"""
import argparse
import hashlib
from functools import cache
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import tomllib
from urllib.parse import unquote, urlparse

TARGET = 'x86_64-unknown-linux-gnu'
ROOTS = ('convt-cli', 'convt-app')
KINDS = {'dependencies', 'build-dependencies', 'dev-dependencies'}


def sha(data):
    return hashlib.sha256(data).hexdigest()


def dump(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + '\n')


def cargo(tree, home, *args, bootstrap=False):
    env = dict(os.environ, CARGO_HOME=str(home), CARGO_NET_OFFLINE='true')
    env.pop('RUSTC_BOOTSTRAP', None)
    if bootstrap:
        env['RUSTC_BOOTSTRAP'] = '1'
    return subprocess.run(['cargo', *args], cwd=tree, env=env, capture_output=True, text=True)


def graph(tree, home):
    result = cargo(tree, home, 'build', '--offline', '--locked', '--release',
                   '--target', TARGET, '-p', ROOTS[0], '-p', ROOTS[1],
                   '-Z', 'unstable-options', '--unit-graph', bootstrap=True)
    if result.returncode:
        raise ValueError('Cargo unit graph failed: ' + result.stderr)
    return json.loads(result.stdout)


def canonical_graph(value, tree):
    # Preserve profiles, feature sets, host/target distinction and every edge.
    def normalize(value):
        if isinstance(value, str):
            return value.replace(str(tree), '$SOURCE')
        if isinstance(value, list):
            return [normalize(v) for v in value]
        if isinstance(value, dict):
            return {k: normalize(v) for k, v in value.items()}
        return value
    return normalize(value)


def table_path(header):
    data = tomllib.loads(header + '\n__linux_scope_marker = true\n')
    path = []
    while '__linux_scope_marker' not in data:
        key, data = next(iter(data.items()))
        path.append(key)
    return tuple(path)


def assignments(body):
    starts = list(re.finditer(r'(?m)^([\w-]+|"[^"\n]+"|\x27[^\x27\n]+\x27)(?:\.[\w-]+)*\s*=', body))
    for i, match in enumerate(starts):
        end = starts[i + 1].start() if i + 1 < len(starts) else len(body)
        text = body[match.start():end]
        parsed = tomllib.loads(text)
        yield next(iter(parsed)), text, parsed


@cache
def linux_target(expression):
    if not expression.startswith('cfg('):
        return expression == TARGET
    cfg = subprocess.check_output(['rustc', '--print', 'cfg', '--target', TARGET], text=True)
    facts = set(cfg.splitlines())
    tokens = re.findall(r'[A-Za-z_][A-Za-z_0-9]*|"[^"\n]*"|[(),=]', expression)
    position = 0
    def parse():
        nonlocal position
        name = tokens[position]
        position += 1
        if position < len(tokens) and tokens[position] == '(':
            position += 1
            values = []
            while tokens[position] != ')':
                values.append(parse())
                if tokens[position] == ',':
                    position += 1
            position += 1
            if name in ('cfg', 'all'):
                return all(values)
            if name == 'any':
                return any(values)
            if name == 'not' and len(values) == 1:
                return not values[0]
            raise ValueError('Unsupported target predicate: ' + expression)
        if position < len(tokens) and tokens[position] == '=':
            position += 1
            name += '=' + tokens[position]
            position += 1
        return name in facts
    result = parse()
    if position != len(tokens):
        raise ValueError('Unsupported target syntax: ' + expression)
    return result


def derive_manifest(original, allowed, members=None):
    text = original.decode()
    document = tomllib.loads(text)
    original_optional = set()
    def find_optional(value):
        for key, child in value.items():
            if key in KINDS:
                for alias, dep in child.items():
                    if isinstance(dep, dict) and dep.get('optional'):
                        original_optional.add(alias)
            elif isinstance(child, dict):
                find_optional(child)
    find_optional(document)
    original_features = document.get('features', {})
    explicit_dep = {ref[4:] for refs in original_features.values() for ref in refs if ref.startswith('dep:')}
    implicit_features = original_optional - explicit_dep
    headers = list(re.finditer(r'(?m)^\[(?!\[)[^\n]+\]\s*$', text))
    output = text[:headers[0].start()] if headers else text
    removed = set()
    sections = []
    for i, header in enumerate(headers):
        end = headers[i + 1].start() if i + 1 < len(headers) else len(text)
        path = table_path(header.group())
        body = text[header.end():end]
        if path[0] == 'target' and not linux_target(path[1]):
            section = document
            for key in path:
                section = section[key]
            if path[-1] in KINDS:
                removed.update(section)
            elif len(path) >= 3 and path[-2] in KINDS:
                removed.add(path[-1])
            continue
        section = document
        for key in path:
            section = section[key]
        if 'dev-dependencies' in path:
            removed.update(section if path[-1] == 'dev-dependencies' else [path[-1]])
            continue
        if len(path) >= 2 and path[-2] in KINDS:
            alias = path[-1]
            name = section.get('package', alias)
            if name not in allowed:
                removed.add(alias)
                continue
        elif path[-1] in KINDS:
            kept = []
            for alias, field, parsed in assignments(body):
                dependency = parsed[alias]
                name = dependency.get('package', alias) if isinstance(dependency, dict) else alias
                if name not in allowed:
                    removed.add(alias)
                else:
                    kept.append(field)
            body = '\n' + ''.join(kept)
        elif path == ('workspace',) and members is not None:
            fields = []
            for key, field, _ in assignments(body):
                if key in ('members', 'default-members'):
                    field = key + ' = ' + json.dumps(members) + '\n'
                fields.append(field)
            body = '\n' + ''.join(fields)
        sections.append((header.group(), path, body))
    retained_aliases = set()
    optional_aliases = set()
    for section_header, path, body in sections:
        if path[-1] in KINDS:
            for key, _, parsed in assignments(body):
                retained_aliases.add(key)
                if isinstance(parsed[key], dict) and parsed[key].get('optional'):
                    optional_aliases.add(key)
        elif len(path) >= 2 and path[-2] in KINDS:
            retained_aliases.add(path[-1])
            section_data = tomllib.loads(section_header + body)
            for key in path:
                section_data = section_data[key]
            if section_data.get('optional'):
                optional_aliases.add(path[-1])
    removed -= retained_aliases
    preserved_features = removed & implicit_features
    for header, path, body in sections:
        if path == ('features',):
            fields = []
            for key, field, parsed in assignments(body):
                refs = parsed[key]
                refs = [ref for ref in refs if ref.removeprefix('dep:').split('/')[0].rstrip('?') not in (removed - preserved_features - original_features.keys())
                        and not ('/' not in ref and ref.removeprefix('dep:') in retained_aliases
                                 and ref.removeprefix('dep:') not in optional_aliases and ref not in original_features)
                        and not (ref.startswith('dep:') and ref[4:] in removed)
                        and not ('/' in ref and ref.split('/')[0].rstrip('?') in removed)]
                fields.append(json.dumps(key) + ' = ' + json.dumps(refs) + '\n')
            body = '\n' + ''.join(fields)
            for alias in sorted(preserved_features - original_features.keys()):
                body += json.dumps(alias) + ' = []\n'
        output += header + body
    tomllib.loads(output)
    return output.encode()


def derive(tree, vendor):
    tree = tree.resolve()
    vendor = vendor.resolve()
    if (tree / '.git').exists() or not vendor.is_relative_to(tree):
        raise ValueError('Use an archive copy without .git and a vendor directory inside it')
    receipt_dir = tree / 'third-party/linux-source-scope'
    if receipt_dir.exists():
        raise ValueError('Linux derivation already exists; start with a fresh archive copy')
    blockers = json.loads((tree / 'packaging/release/rust-notice-blockers.json').read_text())['blockers']
    # Keep resolved Mac notice decisions in the audit ledger without treating
    # them as removable Linux source blockers.
    blocked = {(p['name'], p['version']) for p in blockers if not p.get('resolved')}
    packages = {}
    for manifest in sorted(vendor.glob('*/Cargo.toml')):
        package = tomllib.loads(manifest.read_text())['package']
        packages[(package['name'], package['version'])] = manifest.parent
    with tempfile.TemporaryDirectory(prefix='convt-linux-empty-cargo-') as temporary:
        home = Path(temporary)
        before = graph(tree, home)
        units = before['units']
        retained = {tuple(u['pkg_id'].rsplit('#', 1)[1].rsplit('@', 1))
                    for u in units if u['pkg_id'].startswith('registry+')}
        if retained & blocked:
            raise ValueError('SDK legal blocker is in the actual Linux compilation graph')
        if not retained <= packages.keys():
            raise ValueError('Compilation graph includes sources absent from the vendor tree')
        # First attempt the requested minimal removal with the ORIGINAL lock.
        stash = Path(temporary) / 'removed-sdk'
        stash.mkdir()
        moved = []
        for identity in sorted(blocked & packages.keys()):
            directory = packages[identity]
            shutil.move(str(directory), stash / directory.name)
            moved.append(directory)
        probe = cargo(tree, home, 'build', '--offline', '--locked', '--release',
                      '--target', TARGET, '-p', ROOTS[0], '-p', ROOTS[1])
        for directory in moved:
            shutil.move(str(stash / directory.name), directory)
        receipt_dir.mkdir(parents=True)
        dump(receipt_dir / 'remove-sdk-only-probe.json', {
            'command': 'cargo build --offline --locked --release --target ' + TARGET + ' -p convt-cli -p convt-app',
            'empty_cargo_home': True, 'exit_code': probe.returncode, 'stderr': probe.stderr.replace(str(tree), '$SOURCE').replace(str(home), '$EMPTY_CARGO_HOME')})
        dump(receipt_dir / 'original-unit-graph.json', canonical_graph(before, tree))
        pins = tomllib.loads((tree / 'Cargo.lock').read_text())['package']
        originals = {}
        patches = []
        def record(path, updated):
            original = path.read_bytes()
            if original == updated:
                return
            relative = str(path.relative_to(tree))
            originals[relative] = original.decode()
            patches.append({'path': relative, 'original_sha256': sha(original), 'derived_sha256': sha(updated)})
            path.write_bytes(updated)
        # Only packaging metadata changes. Rust/native source files stay byte-identical.
        members = sorted({str(Path(unquote(urlparse(u['pkg_id'][5:].split('#')[0]).path)).relative_to(tree))
                          for u in units if u['pkg_id'].startswith('path+')})
        allowed = {name for name, _ in retained} | {tomllib.loads((tree / m / 'Cargo.toml').read_text())['package']['name'] for m in members}
        manifests = [tree / 'Cargo.toml'] + [tree / m / 'Cargo.toml' for m in members]
        manifests += [packages[p] / 'Cargo.toml' for p in sorted(retained)]
        # Hash every retained vendor file, not merely Rust entry points.
        content = {str(path.relative_to(tree)): sha(path.read_bytes())
                   for identity in sorted(retained) for path in sorted(packages[identity].rglob('*'))
                   if path.is_file() and path.name not in ('Cargo.toml', '.cargo-checksum.json')}
        for manifest in manifests:
            record(manifest, derive_manifest(manifest.read_bytes(), allowed, members if manifest == tree / 'Cargo.toml' else None))
            checksum = manifest.parent / '.cargo-checksum.json'
            if manifest.is_relative_to(vendor):
                data = json.loads(checksum.read_text())
                data['files']['Cargo.toml'] = sha(manifest.read_bytes())
                record(checksum, (json.dumps(data, sort_keys=True, separators=(',', ':')) + '\n').encode())
        removed = []
        for identity, directory in sorted(packages.items()):
            if identity not in retained:
                removed.append({'name': identity[0], 'version': identity[1], 'sdk_blocker': identity in blocked,
                                'original_manifest_sha256': sha((directory / 'Cargo.toml').read_bytes())})
                # Manifest metadata is safe to retain; never copy SDK-generated sources.
                originals[str((directory / 'Cargo.toml').relative_to(tree))] = (directory / 'Cargo.toml').read_text()
                shutil.rmtree(directory)
        original_lock = (tree / 'Cargo.lock').read_bytes()
        lock_result = cargo(tree, home, 'update', '--offline')
        if lock_result.returncode:
            raise ValueError('Derived offline lock failed: ' + lock_result.stderr)
        derived_lock = (tree / 'Cargo.lock').read_bytes()
        if derived_lock != original_lock:
            originals['Cargo.lock'] = original_lock.decode()
            patches.append({'path': 'Cargo.lock', 'original_sha256': sha(original_lock), 'derived_sha256': sha(derived_lock)})
        original_pins = {(p['name'], p['version'], p.get('source')): p.get('checksum') for p in pins}
        for pin in tomllib.loads(derived_lock.decode())['package']:
            identity = (pin['name'], pin['version'], pin.get('source'))
            if identity not in original_pins or original_pins[identity] != pin.get('checksum'):
                raise ValueError('Derivation changed an original package pin: ' + repr(identity))
        after = graph(tree, home)
        if canonical_graph(before, tree) != canonical_graph(after, tree):
            dump(receipt_dir / 'failed-derived-unit-graph.json', canonical_graph(after, tree))
            raise ValueError('Linux compilation units/features/edges changed')
        for relative, expected in content.items():
            if sha((tree / relative).read_bytes()) != expected:
                raise ValueError('Retained upstream source changed: ' + relative)
        dump(receipt_dir / 'original-metadata.json', originals)
        dump(receipt_dir / 'retained-file-hashes.json', content)
        receipt = {'schema_version': 1, 'scope': 'Linux CLI/app release compilation closure only; macOS is separately gated',
                   'target': TARGET, 'roots': list(ROOTS), 'cargo_version': cargo(tree, home, '--version').stdout.strip(),
                   'unit_count': len(units), 'package_count': len({u['pkg_id'] for u in units}),
                   'retained_vendor_count': len(retained), 'original_lock_sha256': sha(original_lock),
                   'derived_lock_sha256': sha(derived_lock), 'original_pins': pins,
                   'removed_packages': removed, 'patches': patches,
                   'unit_graph_identical': True, 'retained_source_bytes_identical': True,
                   'blocked_sdk_packages_retained': []}
        dump(receipt_dir / 'receipt.json', receipt)
        print(json.dumps({k: receipt[k] for k in ('unit_count', 'package_count', 'retained_vendor_count', 'unit_graph_identical')}))
        return receipt


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('tree', type=Path)
    parser.add_argument('--vendor-dir', default='third-party/rust')
    args = parser.parse_args()
    derive(args.tree, args.tree / args.vendor_dir)


if __name__ == '__main__':
    main()
