#!/usr/bin/env bash
# Clean container, no host Cargo registry or target directory.
set -euo pipefail
archive=$(realpath "${1:?source archive required}")
repo=$(cd "$(dirname "$0")/../.." && pwd)
work=$(mktemp -d /tmp/convt-source-rebuild-XXXXXX)
name="convt-source-rebuild-$$"
cleanup() { docker rm -f "$name" >/dev/null 2>&1 || true; rm -rf "$work"; }
trap cleanup EXIT
# Reject unsafe archive names before extracting into the owned directory.
python3 - "$archive" <<'PY'
import sys,tarfile,pathlib
with tarfile.open(sys.argv[1]) as t:
 for m in t.getmembers():
  p=pathlib.PurePosixPath(m.name)
  if p.is_absolute() or '..' in p.parts:sys.exit('unsafe archive path')
  if m.issym() or m.islnk():
   import posixpath
   link=pathlib.PurePosixPath(m.linkname)
   resolved=posixpath.normpath(str(p.parent/link) if m.issym() else str(link))
   if link.is_absolute() or not resolved.startswith('convt-source/'):sys.exit('unsafe archive link')
PY
tar --warning=no-timestamp -xf "$archive" -C "$work"
base=$(python3 -c 'import json,sys;print(json.load(open(sys.argv[1]))["image"])' "$repo/packaging/linux/build-image.json")
rust=$(dirname "$(dirname "$(rustup which rustc)")")
docker run --rm --name "$name" --label app=convt --label purpose=source-rebuild --network none \
  -v "$work:/work" -v "$rust:/rust:ro" "$base" bash -c '
    set -euo pipefail
    export PATH=/rust/bin:$PATH CARGO_HOME=/tmp/clean-cargo CARGO_TARGET_DIR=/tmp/clean-target CONVT_LICENSE_STORE=file
    cd /work/convt-source
    cargo build --offline --locked -p convt-cli -j 8
    /tmp/clean-target/debug/convt formats --json > /work/formats.json
    /tmp/clean-target/debug/convt engines
    /tmp/clean-target/debug/convt targets sample.png
    printf "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"16\" height=\"16\"><rect width=\"16\" height=\"16\" fill=\"red\"/></svg>" > /tmp/sample.svg
    /tmp/clean-target/debug/convt /tmp/sample.svg --to png -o /tmp/result
    test -s /tmp/result/sample.png
    python3 -c "import pathlib; p=pathlib.Path(\"/tmp/result/sample.png\"); assert p.read_bytes()[:8]==bytes([137,80,78,71,13,10,26,10]); print(\"PASS archived CLI SVG to PNG\")"
  '
echo 'PASS source archive rebuilt and ran the CLI offline in a clean container'
