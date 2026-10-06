#!/usr/bin/env bash
set -euo pipefail
repo=$(cd "$(dirname "$0")/../.." && pwd)
cache=${CONVT_BUNDLE_CACHE:-$repo/packaging/.cache}
out=${CONVT_BUNDLE_OUT:-$repo/packaging/out}
# Use the bundle's build date so mksquashfs writes reproducible timestamps.
bundle_epoch=$(cat "$out/source-date-epoch" 2>/dev/null) || { echo "Missing $out/source-date-epoch; run build.sh first" >&2; exit 1; }
[[ -z ${SOURCE_DATE_EPOCH:-} || $SOURCE_DATE_EPOCH == "$bundle_epoch" ]] || { echo "SOURCE_DATE_EPOCH differs from the bundle's $bundle_epoch" >&2; exit 1; }
export SOURCE_DATE_EPOCH=$bundle_epoch
# Both the tool and embedded runtime are pinned; appimagetool must not
# silently download its own unpinned runtime.
python3 "$repo/packaging/release/appimage-source-build.py" --cache "$cache"
cp "$cache/appimage-source/rebuilt/runtime-x86_64" "$cache/runtime-source-built-x86_64"
python3 "$repo/packaging/linux/fetch.py" "$cache" "$repo/packaging/linux/appimage-inputs.lock.json"
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
cp "$cache/appimagetool.AppImage" "$work/tool"
chmod +x "$work/tool"
(cd "$work" && ./tool --appimage-extract >/dev/null)
mkdir "$work/convt.AppDir"
cp -a "$out/convt/." "$work/convt.AppDir/"
cp "$repo/packaging/linux/convt.desktop" "$repo/packaging/linux/convt.svg" "$work/convt.AppDir/"
cat > "$work/convt.AppDir/AppRun" <<'EOF'
#!/bin/sh
app_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
exec "$app_dir/convt-app" "$@"
EOF
chmod +x "$work/convt.AppDir/AppRun"
cp "$cache/runtime-LICENSE" "$work/convt.AppDir/licenses/appimage-runtime.txt"
python3 "$repo/packaging/release/install-source-notices.py" "$repo/packaging/release/appimage-source-closure.lock.json" "$cache" "$work/convt.AppDir/licenses/appimage"
cp "$repo/packaging/linux/appimage-inputs.lock.json" "$work/convt.AppDir/licenses/"
# SOURCE_DATE_EPOCH clamps newer mtimes only. Normalize every input too,
# including newly created directories when the release day is in the future.
find "$work/convt.AppDir" -exec touch -h -d "@$SOURCE_DATE_EPOCH" {} +
# appimagetool creates .DirIcon and changes the AppDir root after the find.
# Force all SquashFS inode times, including those generated entries. The tool
# rejects explicit time options together with SOURCE_DATE_EPOCH, so unset it
# only for this process and pass the same epoch through both time options.
env -u SOURCE_DATE_EPOCH ARCH=x86_64 "$work/squashfs-root/AppRun" \
  --mksquashfs-opt -mkfs-time --mksquashfs-opt "$SOURCE_DATE_EPOCH" \
  --mksquashfs-opt -all-time --mksquashfs-opt "$SOURCE_DATE_EPOCH" \
  --runtime-file "$cache/runtime-source-built-x86_64" "$work/convt.AppDir" "$out/convt-linux-x86_64.AppImage"
(cd "$out" && sha256sum convt-linux-x86_64.AppImage > convt-linux-x86_64.AppImage.sha256)
