#!/usr/bin/env python3
import json,pathlib,zipfile,subprocess,shutil,sys,tarfile
stage=pathlib.Path(sys.argv[1])
for item in json.load(open('packaging/linux/inputs.lock.json')):
    if item.get('package') not in ['libstdcxx','libgcc']: continue
    root=pathlib.Path('/work/runtime')/item['package']
    if root.exists(): shutil.rmtree(root)
    root.mkdir(parents=True)
    with zipfile.ZipFile('/inputs/'+item['name']) as archive:
        for member in archive.namelist():
            if member.endswith('.tar.zst'):
                archive.extract(member,root)
                subprocess.run(['tar','--use-compress-program=python3 /repo/packaging/linux/zstd-decompress.py','-xf',str(root/member),'-C',str(root)],check=True)
    for name in ['libstdc++.so.6','libgcc_s.so.1']:
        source=root/'lib'/name
        if source.exists(): shutil.copy2(source,stage/'lib'/name)
    # Retain package provenance and its exact GCC source/build recipe.
    shutil.copytree(root/'info',stage/'licenses/compiler'/item['package'],dirs_exist_ok=True)

# The binary packages carry metadata but omit the licence texts. Use the
# matching, hash-verified GCC source input for the GPL and runtime exception.
notices=stage/'licenses/compiler'
with tarfile.open('/inputs/gcc-15.2.0.tar.gz') as archive:
    for name in ['COPYING3','COPYING.RUNTIME']:
        member=archive.extractfile('gcc-15.2.0/'+name)
        if member is None: raise RuntimeError('Missing GCC notice: '+name)
        (notices/name).write_bytes(member.read())
