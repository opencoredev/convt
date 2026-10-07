#!/bin/bash
# The Developer ID release: build, sign with the hardened runtime, notarize
# and staple the app, then the same for the disk image.
#
# Needs, from Leo's paid Apple Developer account:
#   CONVT_SIGN_IDENTITY   "Developer ID Application: <Name> (<TEAMID>)", with
#                         its private key in the login keychain
#   CONVT_NOTARY_PROFILE  a notarytool keychain profile, created once with
#                         xcrun notarytool store-credentials convt-notary \
#                           --key <api-key.p8> --key-id <KEY_ID> --issuer <ISSUER_ID>
#   CONVT_MAC_ARCHS       "arm64 x86_64" for the universal release (default)
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
: "${CONVT_SIGN_IDENTITY:?set CONVT_SIGN_IDENTITY to a Developer ID Application identity}"
: "${CONVT_NOTARY_PROFILE:?set CONVT_NOTARY_PROFILE to a notarytool keychain profile}"
case "$CONVT_SIGN_IDENTITY" in
  "Developer ID Application:"*) ;;
  *) echo "notarization needs a Developer ID Application identity, not $CONVT_SIGN_IDENTITY" >&2; exit 1 ;;
esac
export CONVT_MAC_ARCHS=${CONVT_MAC_ARCHS:-arm64 x86_64} CONVT_HARDENED=1
out=${CONVT_MAC_OUT:-$here/out}
unset CONVT_MAC_UNSOURCED_FFMPEG
"$here/bundle.sh"
app="$out/convt.app"
if [ -e "$app/Contents/Resources/licenses/FFmpeg/NOT-READY" ]; then
  echo "NOT READY: FFmpeg without corresponding source for $(cat "$app/Contents/Resources/licenses/FFmpeg/NOT-READY")" >&2
  exit 1
fi

notarize() { # file
  xcrun notarytool submit "$1" --keychain-profile "$CONVT_NOTARY_PROFILE" --wait --output-format json \
    | tee "$1.notary.json"
  grep -q '"status" *: *"Accepted"' "$1.notary.json" || {
    echo "notarization failed; see xcrun notarytool log <id> --keychain-profile $CONVT_NOTARY_PROFILE" >&2
    exit 1
  }
}

zip="$out/convt-notarize.zip"
ditto -c -k --keepParent "$app" "$zip"
notarize "$zip"
rm -f "$zip" "$zip.notary.json"
xcrun stapler staple "$app"

dmg=$("$here/dmg.sh" "$app" | tail -1)
notarize "$dmg"
xcrun stapler staple "$dmg"
spctl --assess --type execute --verbose=2 "$app"
spctl --assess --type open --context context:primary-signature --verbose=2 "$dmg"
echo "release: $dmg"
