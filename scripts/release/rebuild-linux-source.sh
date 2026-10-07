#!/usr/bin/env bash
# Prove the complete derived Linux release graph with no host dependency cache.
set -euo pipefail
source_tree=$(realpath "${1:?archived source tree required}")
repo=$(cd "$(dirname "$0")/../.." && pwd)
cache=$(realpath "${CONVT_BUNDLE_CACHE:-$repo/packaging/.cache}")
work=$(mktemp -d /tmp/convt-source-proof-XXXXXX)
name="convt-source-proof-$$"
image="convt-source-proof-builder-$$"
cleanup() { docker rm -f "$name" >/dev/null 2>&1 || true; docker image rm "$image" >/dev/null 2>&1 || true; rm -rf "$work"; }
trap cleanup EXIT
python3 "$source_tree/packaging/linux/fetch.py" "$cache" "$source_tree/packaging/linux/build-rpms.lock.json"
python3 - "$cache" "$work" "$source_tree" <<'PY'
import hashlib,json,pathlib,shutil,sys
cache,work,tree=map(pathlib.Path,sys.argv[1:])
for item in json.load(open(tree/'packaging/linux/build-rpms.lock.json')):
 path=cache/item['name']
 with path.open('rb') as stream:
  assert hashlib.file_digest(stream,'sha256').hexdigest()==item['sha256'],item['name']
 shutil.copyfile(path,work/item['name'])
base=json.load(open(tree/'packaging/linux/build-image.json'))['image']
(work/'Dockerfile').write_text('FROM '+base+'\nCOPY *.rpm /tmp/build-rpms/\nRUN dnf --disablerepo="*" install -y /tmp/build-rpms/*.rpm && rm -rf /tmp/build-rpms\n')
PY
timeout --foreground 120 docker build --network none -t "$image" "$work"
rust=$(dirname "$(dirname "$(rustup which rustc)")")
timeout --foreground 1500 docker run --rm --name "$name" --label app=convt --label purpose=source-proof --network none \
  -v "$source_tree:/repo:ro" -v "$rust:/rust:ro" \
  -e "SOURCE_DATE_EPOCH=${SOURCE_DATE_EPOCH:?}" "$image" bash -c '
    set -euo pipefail
    export PATH=/rust/bin:$PATH CARGO_HOME=/tmp/clean-cargo CARGO_TARGET_DIR=/tmp/clean-target
    export CONVT_LICENSE_STORE=file RUST_FONTCONFIG_DLOPEN=1 CARGO_INCREMENTAL=0
    export CONST_RANDOM_SEED="convt-$SOURCE_DATE_EPOCH"
    cd /repo
    test ! -e "$CARGO_HOME" && test ! -e "$CARGO_TARGET_DIR"
    cargo build --offline --locked --release --target x86_64-unknown-linux-gnu -p convt-cli -p convt-app -j 8
    c=/tmp/clean-target/x86_64-unknown-linux-gnu/release/convt
    test -x /tmp/clean-target/x86_64-unknown-linux-gnu/release/convt-app
    "$c" formats --json > /tmp/formats.json
    printf "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"16\" height=\"16\"><rect width=\"16\" height=\"16\" fill=\"red\"/></svg>" > /tmp/sample.svg
    "$c" /tmp/sample.svg --to png -o /tmp/result
    python3 -c "import pathlib; p=pathlib.Path(\"/tmp/result/sample.png\"); assert p.read_bytes()[:8]==bytes([137,80,78,71,13,10,26,10]); print(\"PASS derived release CLI/app empty-cache build and SVG to PNG\")"
  '
