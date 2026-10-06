#!/usr/bin/env bash
set -euo pipefail
root=$(cd "$(dirname "$0")/../../.." && pwd)
context=$(mktemp -d /tmp/convt-sandbox-build.XXXXXX)
trap 'rm -rf "$context"' EXIT
cp -a "$root/packaging/out/convt" "$context/payload"
cp "$root/crates/convt-worker/sandbox/Dockerfile" "$context/Dockerfile"
timeout 600 docker build --label "convt.checkout=$root" -t "convt-sandbox:local" "$context"
