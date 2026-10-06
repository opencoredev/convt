#!/bin/bash
# Packs convt.app into a compressed disk image with an Applications link.
#
#   packaging/macos/dmg.sh [path/to/convt.app]
#
# Signs the image with CONVT_SIGN_IDENTITY when it names a real identity.
# Writes convt-<version>-<arch>.dmg next to the app and prints its path.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
app=${1:-${CONVT_MAC_OUT:-$here/out}/convt.app}
out=$(cd "$(dirname "$app")" && pwd)
app="$out/$(basename "$app")"
version=$(defaults read "$app/Contents/Info.plist" CFBundleShortVersionString)
archs=$(lipo -archs "$app/Contents/MacOS/convt-app")
case "$archs" in *" "*) arch=universal ;; *) arch=$archs ;; esac
dmg="$out/convt-$version-$arch.dmg"
stage=$(mktemp -d "$out/dmg.XXXXXX")
trap 'rm -rf "$stage"' EXIT
ditto "$app" "$stage/convt.app"
ln -s /Applications "$stage/Applications"
rm -f "$dmg"
hdiutil create -quiet -volname convt -srcfolder "$stage" -fs HFS+ -format UDZO -ov "$dmg"
identity=${CONVT_SIGN_IDENTITY:-}
if [ -n "$identity" ] && [ "$identity" != - ]; then
  codesign --force --sign "$identity" --timestamp "$dmg"
fi
hdiutil verify -quiet "$dmg"
echo "built $(du -h "$dmg" | cut -f1)" >&2
# The path alone on stdout, for release.sh.
echo "$dmg"
