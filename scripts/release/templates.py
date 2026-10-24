#!/usr/bin/env python3
"""Render package-manager templates from the exact website manifest.

Generated repository indexes remain unsigned staging metadata. The publisher
signs apt Release and rpm packages after reproducibility checks.
"""
import gzip
import hashlib
import json
from pathlib import Path
import shutil
import sys
from urllib.parse import urlparse, unquote

manifest_path=Path(sys.argv[1]); output=Path(sys.argv[2]);output.mkdir(parents=True,exist_ok=True)
m=json.loads(manifest_path.read_text());b=m['builds'][-1];version=b['version']
artifacts={(a['platform'],a['kind']):a for a in b['artifacts']}
def text(path,data):
    p=output/path;p.parent.mkdir(parents=True,exist_ok=True);p.write_text(data)
def copy_artifact(a,dest):
    name=unquote(Path(urlparse(a['url']).path).name)
    src=manifest_path.parent/name
    if hashlib.sha256(src.read_bytes()).hexdigest()!=a['sha256']:raise ValueError('manifest hash mismatch: '+name)
    p=output/dest/name;p.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(src,p)
    return p
mac=artifacts.get(('macos-arm64','dmg'))
if mac:
    from importlib.util import module_from_spec, spec_from_file_location
    homebrew_spec=spec_from_file_location('homebrew_cask',Path(__file__).with_name('homebrew_cask.py'))
    homebrew=module_from_spec(homebrew_spec); homebrew_spec.loader.exec_module(homebrew)
    source=Path(__file__).resolve().parents[2]/'Casks'/'convt.rb'
    text(Path('homebrew/convt.rb'),homebrew.bump_cask(source.read_text(),version=version,sha256=mac['sha256'],url=mac['url']))
windows=artifacts.get(('windows-x86_64','msi')) or artifacts.get(('windows-x86_64','exe'))
if windows:
    text(Path('winget/Convt.Convt.installer.yaml'),f'''PackageIdentifier: Convt.Convt
PackageVersion: {version}
InstallerType: {windows['kind']}
Installers:
  - Architecture: x64
    InstallerUrl: {windows['url']}
    InstallerSha256: {windows['sha256'].upper()}
ManifestType: installer
ManifestVersion: 1.9.0
''')
    text(Path('winget/Convt.Convt.yaml'),f'PackageIdentifier: Convt.Convt\nPackageVersion: {version}\nDefaultLocale: en-US\nManifestType: version\nManifestVersion: 1.9.0\n')
    text(Path('winget/Convt.Convt.locale.en-US.yaml'),f'''PackageIdentifier: Convt.Convt
PackageVersion: {version}
PackageLocale: en-US
Publisher: Convt
PackageName: convt
License: AGPL-3.0-only
LicenseUrl: https://github.com/opencoredev/convt/blob/main/LICENSE
ShortDescription: Local file conversion
ManifestType: defaultLocale
ManifestVersion: 1.9.0
''')
deb=artifacts.get(('linux-x86_64','deb'))
if deb:
    p=copy_artifact(deb,Path('apt/pool/main/c/convt'))
    # Read the package's actual control fields, rather than invent dependencies.
    import subprocess
    control=subprocess.check_output(['dpkg-deb','-f',str(p)],text=True).strip()
    packages=control+f"\nFilename: {p.relative_to(output/'apt')}\nSize: {deb['size']}\nSHA256: {deb['sha256']}\n\n"
    text(Path('apt/dists/stable/main/binary-amd64/Packages'),packages)
    pkg=output/'apt/dists/stable/main/binary-amd64/Packages'
    pkg.with_suffix('.gz').write_bytes(gzip.compress(pkg.read_bytes(),mtime=0))
    release='Origin: convt\nLabel: convt\nSuite: stable\nCodename: stable\nArchitectures: amd64\nComponents: main\nSHA256:\n'
    root=output/'apt/dists/stable'
    for f in sorted(root.rglob('Packages*')):
        release+=f" {hashlib.sha256(f.read_bytes()).hexdigest()} {f.stat().st_size} {f.relative_to(root)}\n"
    text(Path('apt/dists/stable/Release'),release)
    text(Path('apt/sign.sh'),'''#!/bin/sh
set -eu
# Run from the apt directory after verifying unsigned artifacts.
gpg --batch --yes --local-user "$CONVT_REPO_SIGNING_KEY_ID" --clearsign -o dists/stable/InRelease dists/stable/Release
gpg --batch --yes --local-user "$CONVT_REPO_SIGNING_KEY_ID" --armor --detach-sign -o dists/stable/Release.gpg dists/stable/Release
''')
rpm=artifacts.get(('linux-x86_64','rpm'))
if rpm:
    copy_artifact(rpm,Path('dnf/packages'))
    text(Path('dnf/convt.repo'),'''[convt]
name=convt
baseurl=https://downloads.convt.app/repos/dnf/
enabled=1
gpgcheck=1
repo_gpgcheck=1
gpgkey=https://downloads.convt.app/repos/convt-release.asc
''')
    text(Path('dnf/sign-and-index.sh'),'''#!/bin/sh
set -eu
# Run from dnf on a signing host, after comparing unsigned builds.
# Signing changes RPM hashes; regenerate manifests from these signed copies.
rpmsign --define "_gpg_name $CONVT_REPO_SIGNING_KEY_ID" --addsign packages/*.rpm
createrepo_c --revision "$SOURCE_DATE_EPOCH" --set-timestamp-to-revision .
gpg --batch --yes --local-user "$CONVT_REPO_SIGNING_KEY_ID" --armor --detach-sign repodata/repomd.xml
''')
text(Path('STATUS.json'),json.dumps({'distribution_ready':m['distribution_ready'],'version':version,
    'homebrew':'rendered' if mac else 'awaiting notarized macos-arm64 dmg',
    'winget':'rendered' if windows else 'awaiting signed Windows installer',
    'apt':'unsigned staging index' if deb else 'missing deb',
    'dnf':'packages and sign/index recipe' if rpm else 'missing rpm'},indent=2)+'\n')
