#!/bin/bash
# Re-signs a built convt.app with a real identity, keeping each part's
# entitlements. Use it after an ad-hoc build made over SSH, where the login
# keychain is locked: run it in the Mac's own session (Terminal on the Mac),
# where codesign can reach the key.
#
#   packaging/macos/resign.sh [path/to/convt.app]
#
# Environment:
#   CONVT_SIGN_IDENTITY  codesign identity. Default: the first "Developer ID
#                        Application" identity, else "Apple Development".
#
# The identity's team must match the App Group the app was built for, or the
# Finder menu still can't share its container with the app.
set -euo pipefail
app=${1:-/Applications/convt.app}
contents=$app/Contents
appex=$contents/PlugIns/FinderSync.appex

identity=${CONVT_SIGN_IDENTITY:-}
if [ -z "$identity" ]; then
  ids=$(security find-identity -v -p codesigning)
  identity=$(echo "$ids" | sed -n 's/.*"\(Developer ID Application:[^"]*\)".*/\1/p' | head -1)
  [ -n "$identity" ] || identity=$(echo "$ids" | sed -n 's/.*"\(Apple Development:[^"]*\)".*/\1/p' | head -1)
  [ -n "$identity" ] || { echo "error: no signing identity in the keychain" >&2; exit 1; }
fi
team=$(security find-certificate -c "$identity" -p | openssl x509 -noout -subject \
  | sed -n 's/.*OU *= *\([A-Z0-9]\{10\}\).*/\1/p')
group=$(defaults read "$appex/Contents/Info.plist" ConvtAppGroup)
if [ "$group" != "$team.app.convt.desktop" ]; then
  echo "error: $app was built for App Group $group, but $identity is team ${team:-unknown}" >&2
  exit 1
fi

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
codesign -d --entitlements - --xml "$appex" >"$work/ext.plist" 2>/dev/null
codesign -d --entitlements - --xml "$app" >"$work/app.plist" 2>/dev/null

args=(--force --sign "$identity")
case "$identity" in
  "Developer ID Application:"*) args+=(--options runtime --timestamp) ;;
  *) args+=(--timestamp=none) ;;
esac
# Inside out, never --deep, as bundle.sh signs.
codesign "${args[@]}" "$contents/Frameworks/libpdfium.dylib"
for name in ffmpeg ffprobe convt; do
  codesign "${args[@]}" --identifier "app.convt.desktop.$name" "$contents/MacOS/$name"
done
codesign "${args[@]}" --entitlements "$work/ext.plist" "$appex"
codesign "${args[@]}" --entitlements "$work/app.plist" "$app"
codesign --verify --strict --verbose=2 "$app"
echo "re-signed $app as $identity"
