#!/usr/bin/env bash
# Build the docs and deploy them as the convt-docs Worker at convt.app/docs.
# Pass --dry-run to build and stage without uploading.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."

bun run build
# Blume writes a /docs-based site to dist/. Workers serve assets by request path, so
# the pages move under docs/, while _headers and _redirects stay at the asset root.
rm -rf .deploy
mkdir -p .deploy
cp -R dist .deploy/docs
for file in _headers _redirects; do
  if [[ -f .deploy/docs/$file ]]; then mv ".deploy/docs/$file" .deploy/; fi
done
# Host files for other platforms. Workers ignores them; keep them out of the upload.
rm -f .deploy/docs/vercel.json .deploy/docs/blume-redirects.json

if [[ ${1:-} == --dry-run ]]; then
  bunx wrangler deploy --dry-run
else
  bunx wrangler deploy
fi
