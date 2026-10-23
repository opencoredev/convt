#!/usr/bin/env bash
# Builds the Linux document pack from Ubuntu 22.04's LibreOffice, in the
# digest-pinned image from package-images.lock.json, with apt pinned to one
# snapshot.ubuntu.com moment so every run resolves the same package versions.
#
#   packaging/linux/document-pack.sh OUTPUT_DIR
#
# Writes OUTPUT_DIR/documents-linux-x86_64.tar.gz and prints the builder's JSON
# receipt (archive, sha256, bytes, installed_bytes) on stdout. This needs the
# network for apt; the release build container itself stays offline.
set -euo pipefail
repo=$(cd "$(dirname "$0")/../.." && pwd)
out=${1:?output directory required}
mkdir -p "$out"
out=$(realpath "$out")
[[ ! -e $out/documents ]] || { echo "Output already exists: $out/documents" >&2; exit 1; }
image=$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["tests"]["ubuntu-22.04"])' "$repo/packaging/linux/package-images.lock.json")
# Bump together with the pack's LibreOffice when refreshing it.
snapshot=${CONVT_DOCUMENT_PACK_SNAPSHOT:-20261020T000000Z}
[[ $snapshot =~ ^[0-9]{8}T[0-9]{6}Z$ ]] || { echo "Bad snapshot: $snapshot" >&2; exit 1; }
name="convt-document-pack-$$"
trap 'docker rm -f "$name" >/dev/null 2>&1 || true' EXIT
docker run --rm --name "$name" --label app=convt --label purpose=document-pack \
  -v "$repo/packaging/linux:/recipe:ro" -v "$repo/LICENSE:/LICENSE:ro" -v "$out:/out" \
  -e "SNAPSHOT=$snapshot" -e "SOURCE_DATE_EPOCH=${SOURCE_DATE_EPOCH:-0}" \
  -e "OWNER=$(id -u):$(id -g)" "$image" bash -c '
    set -euo pipefail
    trap "chown -R \"$OWNER\" /out" EXIT
    export DEBIAN_FRONTEND=noninteractive
    # snapshot.ubuntu.com is HTTPS only and the base image has no CA bundle:
    # take it from the image'"'"'s own signed archive first.
    apt-get update -qq
    apt-get install -y -qq --no-install-recommends ca-certificates >/dev/null
    printf "deb https://snapshot.ubuntu.com/ubuntu/%s jammy main universe\ndeb https://snapshot.ubuntu.com/ubuntu/%s jammy-updates main universe\ndeb https://snapshot.ubuntu.com/ubuntu/%s jammy-security main universe\n" "$SNAPSHOT" "$SNAPSHOT" "$SNAPSHOT" > /etc/apt/sources.list
    rm -f /etc/apt/sources.list.d/*
    apt-get update -qq
    apt-get install -y -qq --no-install-recommends \
      libreoffice-writer-nogui libreoffice-calc-nogui libreoffice-impress-nogui \
      fonts-dejavu-core liblangtag-common libnss3 gcc libc6-dev python3 >/dev/null
    mkdir -p /build/packaging/linux
    cp /recipe/build-document-pack.py /recipe/langtag-path.c /build/packaging/linux/
    cp /LICENSE /build/LICENSE
    python3 /build/packaging/linux/build-document-pack.py /out
  '
