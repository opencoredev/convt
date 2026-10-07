#!/usr/bin/env python3
"""Compare published artifact bytes and manifests from two independent runs."""
import hashlib
import json
from pathlib import Path
import sys
import tarfile
from urllib.parse import urlparse,unquote
left,right=map(Path,sys.argv[1:3])
m=json.loads((left/'release-manifest.json').read_text())
names=['release-manifest.json','update-manifest.json','source-audit.json']
for b in m['builds']:
    if b['version']==left.name:
        names.extend(unquote(Path(urlparse(a['url']).path).name) for a in b['artifacts']+[b['source']])
failed=[]
for name in names:
    a=hashlib.sha256((left/name).read_bytes()).hexdigest()
    b=hashlib.sha256((right/name).read_bytes()).hexdigest()
    print(('PASS' if a==b else 'FAIL')+' '+name+' '+a)
    if a!=b:failed.append(name)
def members(path):
    with tarfile.open(path) as t:
        return {m.name:(hashlib.sha256(t.extractfile(m).read()).hexdigest() if m.isfile() else m.type.decode(),m.mtime,m.mode,m.uid,m.gid)
                for m in t.getmembers()}
for name in failed:
    if not name.endswith('.tar.gz'):continue
    # Name the members that differ so a failed run says why.
    a,b=members(left/name),members(right/name)
    diff=sorted(k for k in a.keys()|b.keys() if a.get(k)!=b.get(k))
    print(f'{name}: {len(diff)} differing members')
    for k in diff[:40]:print('  ',k,a.get(k),b.get(k))
if failed:sys.exit('Non-reproducible artifacts: '+', '.join(failed))
