#!/usr/bin/env bash
set -euo pipefail
mode=${1:---dry-run}
dir=$(realpath "${2:?usage: upload.sh --dry-run|--upload VERSION_DIRECTORY}")
[[ $mode == --dry-run || $mode == --upload ]] || exit 2
# Verification artifacts are never uploaded, even with --upload.
ready=$(python3 - "$dir" <<'PY'
import hashlib,json,pathlib,sys
p=pathlib.Path(sys.argv[1]);m=json.loads((p/'release-manifest.json').read_text())
audit=json.loads((p/'source-audit.json').read_text())
current=[b for b in m['builds'] if b['version']==p.name]
if len(current)!=1:sys.exit('Version directory does not identify exactly one build')
for b in current:
    if b['version']!=p.name: continue
    for a in [b['source']]+b['artifacts']:
        from urllib.parse import urlparse,unquote
        f=p/unquote(pathlib.PurePosixPath(urlparse(a['url']).path).name)
        if f.stat().st_size!=a['size'] or hashlib.sha256(f.read_bytes()).hexdigest()!=a['sha256']:
            sys.exit('Artifact differs from release manifest: '+str(f))
covered=all(a['platform']=='windows-x86_64' or (a['platform'] in audit.get('covered_platforms',[]) and not audit.get('platform_gaps',{}).get(a['platform'])) for b in current for a in b['artifacts'])
print('yes' if m['distribution_ready'] and audit['distribution_ready'] and not audit['gaps'] and covered else 'no')
PY
)
if [[ $mode == --dry-run ]]; then
  echo "R2 dry run only; distribution_ready=$ready; bucket=${CONVT_R2_BUCKET:-NOT_CREATED}"
  find "$dir" -maxdepth 1 -type f -printf '%f\n' | sort
  exit 0
fi
[[ $ready == yes ]] || { echo 'Upload blocked: source/notice publication gates failed' >&2; exit 1; }
bun "$(dirname "$0")/manifest.ts" verify "$dir/release-manifest.json" "$dir/update-manifest.json"
: "${CONVT_R2_BUCKET:?Leo must create the release bucket}"
: "${CONVT_R2_ACCOUNT_ID:?Set the Cloudflare account ID}"
# Account and bucket management use cf. S3 data transfer is required for
# source archives above the cf REST object's 300 MB limit.
bun "$(dirname "$0")/upload-r2.ts" "$dir"
