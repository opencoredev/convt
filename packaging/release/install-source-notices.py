#!/usr/bin/env python3
"""Install the exact dependency notices referenced by a validated closure lock."""
import argparse
import importlib.util
import json
from pathlib import Path
import hashlib

HERE=Path(__file__).resolve().parent
spec=importlib.util.spec_from_file_location('native_sources',HERE/'native-source-tools.py')
native=importlib.util.module_from_spec(spec);spec.loader.exec_module(native)
p=argparse.ArgumentParser(description=__doc__)
p.add_argument('lock',type=Path);p.add_argument('cache',type=Path);p.add_argument('destination',type=Path)
a=p.parse_args();lock=json.loads(a.lock.read_text())
if not lock['closure_complete'] or lock.get('blockers'):raise SystemExit('Source closure incomplete')
sources={s['name']:s for s in lock['sources']}
for s in sources.values():
 path=a.cache/s['cache']
 if hashlib.file_digest(path.open('rb'),'sha256').hexdigest()!=s['sha256']:raise SystemExit('Source hash mismatch: '+s['name'])
for c in lock['components']:
 if not c['notices']:raise SystemExit('No notice: '+c['name'])
 for i,n in enumerate(c['notices']):
  data=native.member_data(n,sources,a.cache)
  target=a.destination/c['name']/(str(i)+'-'+Path(n['member']).name)
  target.parent.mkdir(parents=True,exist_ok=True);target.write_bytes(data)
print('Installed notices for '+str(len(lock['components']))+' source components')
