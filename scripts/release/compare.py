#!/usr/bin/env python3
"""Compare published artifact bytes and manifests from two independent runs."""
import hashlib
import json
from pathlib import Path
import sys
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
if failed:sys.exit('Non-reproducible artifacts: '+', '.join(failed))
