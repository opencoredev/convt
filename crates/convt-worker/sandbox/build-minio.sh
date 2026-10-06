#!/usr/bin/env bash
set -euo pipefail
root=$(cd "$(dirname "$0")/../../.." && pwd -P)
context=$(mktemp -d /tmp/convt-minio-build.XXXXXX)
trap 'rm -rf "$context"' EXIT
curl -fsSL --max-time 60 https://codeload.github.com/minio/minio/tar.gz/refs/tags/RELEASE.2025-04-22T22-12-26Z -o "$context/minio.tar.gz"
mkdir "$context/source"
tar -xzf "$context/minio.tar.gz" -C "$context/source" --strip-components=1
cp "$root/crates/convt-worker/sandbox/Minio.Dockerfile" "$context/Dockerfile"
timeout 900 docker build --label "convt.checkout=$root" -t convt-minio:local "$context"
