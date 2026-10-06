#!/usr/bin/env bash
# Never changes the unsigned reproducibility inputs.
set -euo pipefail
unsigned=$(realpath "${1:?unsigned version directory}")
signed=${2:?separate signed version directory}
[[ ! -e $signed ]] || { echo 'Signed destination exists' >&2; exit 2; }
: "${CONVT_REPO_SIGNING_KEY_ID:?GPG signing key fingerprint required}"
python3 - "$unsigned" <<'PY'
import json,pathlib,sys
p=pathlib.Path(sys.argv[1]); m=json.loads((p/'release-manifest.json').read_text()); a=json.loads((p/'source-audit.json').read_text())
if not m['distribution_ready'] or not a['distribution_ready'] or a['gaps']:sys.exit('Signing blocked by source and notice gates')
PY
mkdir -p "$signed"
cp "$unsigned/"*.* "$signed/"
rm -f "$signed/release-manifest.json" "$signed/update-manifest.json"
for file in "$signed/"*.deb; do debsigs --sign=origin --default-key="$CONVT_REPO_SIGNING_KEY_ID" "$file"; done
for file in "$signed/"*.rpm; do rpmsign --define "_gpg_name $CONVT_REPO_SIGNING_KEY_ID" --addsign "$file"; done
(cd "$signed" && sha256sum ./*.deb ./*.rpm > packages.sha256)
for file in "$signed/"*.tar.gz "$signed/"*.AppImage "$signed/"*.deb "$signed/"*.rpm; do
  gpg --batch --yes --local-user "$CONVT_REPO_SIGNING_KEY_ID" --armor --detach-sign "$file"
done
# Regenerate manifests from the signed copies afterward.
echo "Signed copies: $signed; generate manifests and repository indexes next"
