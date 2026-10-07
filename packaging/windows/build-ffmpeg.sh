#!/usr/bin/env bash
# Cross-build the Windows x86_64 FFmpeg from packaging/windows/ffmpeg-source.lock.json.
#
#   bash packaging/windows/build-ffmpeg.sh CACHE OUTPUT
#
# Runs on any Linux host with Docker. It downloads missing sources into CACHE,
# checks every SHA-256 against the lock, and builds inside the pinned Ubuntu
# image with the lock's apt-pinned mingw-w64 toolchain. OUTPUT receives
# bin/ffmpeg.exe, bin/ffprobe.exe, the untouched source tarballs, every
# component notice, the toolchain identity and receipt.json.
set -euo pipefail
script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
repo=$(cd -- "$script_dir/../.." && pwd)
lock=$script_dir/ffmpeg-source.lock.json
cache=${1:?usage: build-ffmpeg.sh CACHE OUTPUT}
output=${2:?usage: build-ffmpeg.sh CACHE OUTPUT}
mkdir -p "$cache"
[[ ! -e $output ]] || { echo "Refusing existing output: $output" >&2; exit 2; }
mkdir -p "$output"
cache=$(cd -- "$cache" && pwd)
output=$(cd -- "$output" && pwd)
python3 - "$lock" "$cache" <<'PY'
import hashlib,json,pathlib,sys,urllib.request
lock=json.load(open(sys.argv[1]));cache=pathlib.Path(sys.argv[2])
for s in lock['sources']:
    p=cache/s['name']
    if not p.exists():
        tmp=p.with_suffix(p.suffix+'.part')
        urllib.request.urlretrieve(s['url'],tmp);tmp.rename(p)
    if hashlib.sha256(p.read_bytes()).hexdigest()!=s['sha256']:
        raise SystemExit(f"Hash mismatch: {p}; remove it before retrying")
print(f"{len(lock['sources'])} pinned FFmpeg sources verified")
PY
image=$(python3 -c 'import json,sys;print(json.load(open(sys.argv[1]))["builder"]["image"])' "$lock")
docker run --rm --network host \
  -e "SOURCE_DATE_EPOCH=${SOURCE_DATE_EPOCH:-1791331200}" -e "CONVT_BUILD_JOBS=${CONVT_BUILD_JOBS:-$(nproc)}" \
  -e "HOST_UID=$(id -u)" -e "HOST_GID=$(id -g)" \
  -v "$repo/packaging/windows:/recipe:ro" -v "$cache:/inputs:ro" -v "$output:/out" \
  "$image" bash /recipe/build-ffmpeg-inside.sh
