"""Build a stable, regular-file-only archive from an extracted pinned MSI."""
import gzip
import hashlib
import pathlib
import sys
import tarfile

source, launcher, output = map(pathlib.Path, sys.argv[1:])
if not (source / 'program/soffice.com').is_file():
    raise SystemExit('LibreOffice MSI tree missing program/soffice.com')
entries = [('soffice.exe', launcher)]
entries += [('libreoffice/' + p.relative_to(source).as_posix(), p)
            for p in sorted(source.rglob('*'))]
with output.open('wb') as raw:
    with gzip.GzipFile(filename='', fileobj=raw, mode='wb', mtime=0) as zipped:
        with tarfile.open(fileobj=zipped, mode='w', format=tarfile.USTAR_FORMAT) as archive:
            for name, path in entries:
                if path.is_symlink() or not (path.is_file() or path.is_dir()):
                    raise SystemExit(f'Unsafe MSI entry: {path}')
                info = archive.gettarinfo(str(path), arcname=name)
                info.uid = info.gid = info.mtime = 0
                info.uname = info.gname = ''
                info.mode = 0o755 if path.is_dir() or path.suffix in ('.exe', '.com') else 0o644
                if path.is_file():
                    with path.open('rb') as stream:
                        archive.addfile(info, stream)
                else:
                    archive.addfile(info)
print(hashlib.file_digest(output.open('rb'), 'sha256').hexdigest())
