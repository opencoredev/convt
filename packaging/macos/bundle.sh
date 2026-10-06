#!/bin/bash
# Builds convt.app: the GPUI app, the convt CLI, FFmpeg and ffprobe, PDFium and
# the Finder Sync extension, then signs it. HEIC and AVIF use macOS's own
# ImageIO through sips, so libheif is not bundled.
#
#   packaging/macos/bundle.sh
#
# Environment:
#   CONVT_MAC_ARCHS      "arm64" (default on Apple silicon), "x86_64", or
#                        "arm64 x86_64" for a universal app.
#   CONVT_MAC_OUT        output directory (default packaging/macos/out)
#   CONVT_MAC_CACHE      download cache (default packaging/macos/.cache)
#   CARGO_TARGET_DIR     where cargo builds (default target/)
#   CONVT_SKIP_CARGO=1   reuse binaries already built in CARGO_TARGET_DIR
#   CONVT_BUNDLE_CACHE   where macos-source-ffmpeg-build.sh wrote its builds
#                        (default packaging/.cache)
#   CONVT_MAC_FFMPEG_DIR_<arch>  one source build's directory, overriding that
#   CONVT_PYTHON         Python 3.11+ for the PDFium notice installer
#                        (default python3)
#   CONVT_MAC_UNSOURCED_FFMPEG=1  allow an architecture with no source build
#                        to use the prebuilt Riedl FFmpeg; the bundle is
#                        marked NOT READY and release.sh refuses it
#   CONVT_SIGN_IDENTITY  codesign identity. Default: the first "Developer ID
#                        Application" identity, else "Apple Development",
#                        else "-" (ad-hoc).
#   CONVT_TEAM_ID        team for the App Group; read from the certificate
#                        when unset. Ad-hoc builds have none, and naming one
#                        while signing ad-hoc fails unless
#                        CONVT_ADHOC_WITH_TEAM=1 (then run resign.sh).
#   CONVT_HARDENED=1     hardened runtime and a secure timestamp; on by
#                        default for Developer ID identities.
#   CONVT_BUILD          CFBundleVersion. Default: SOURCE_DATE_EPOCH, so a
#                        release build is repeatable, else the current time
#                        as seconds since 1970. Either way it only grows.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
. "$here/inputs.sh"

host=$(uname -m)
archs=${CONVT_MAC_ARCHS:-$host}
out=${CONVT_MAC_OUT:-$here/out}
cache=${CONVT_MAC_CACHE:-$here/.cache}
target_dir=${CARGO_TARGET_DIR:-$root/target}
# Cargo runs from the repository root; resolve a relative directory against
# the caller's, once, so building and packaging agree.
case "$target_dir" in /*) ;; *) target_dir="$PWD/$target_dir" ;; esac
case "$out" in /*) ;; *) out="$PWD/$out" ;; esac
case "$cache" in /*) ;; *) cache="$PWD/$cache" ;; esac
version=$(sed -n 's/^version = "\(.*\)"/\1/p' "$root/Cargo.toml" | head -1)
build=${CONVT_BUILD:-${SOURCE_DATE_EPOCH:-$(date +%s)}}
min_macos=13.0
app="$out/convt.app"
# The icons are rendered with the CLI being bundled, so it must run here.
case " $archs " in
  *" $host "*) ;;
  *) echo "CONVT_MAC_ARCHS=\"$archs\" has no $host slice to run on this Mac; include $host" >&2; exit 1 ;;
esac

triple() { case $1 in arm64) echo aarch64-apple-darwin ;; x86_64) echo x86_64-apple-darwin ;; *) echo "unknown arch $1" >&2; exit 1 ;; esac; }
var() { eval "echo \"\${$1}\""; }

fetch() { # url sha256 dest
  if [ -f "$3" ] && [ "$(shasum -a 256 "$3" | cut -d' ' -f1)" = "$2" ]; then return; fi
  mkdir -p "$(dirname "$3")"
  curl -fsSL --retry 3 "$1" -o "$3.partial"
  local got
  got=$(shasum -a 256 "$3.partial" | cut -d' ' -f1)
  if [ "$got" != "$2" ]; then
    rm -f "$3.partial"
    echo "checksum mismatch for $1: expected $2, got $got" >&2
    exit 1
  fi
  mv "$3.partial" "$3"
}

# Joins one file per arch into $dest (lipo for several).
join_archs() { # dest file...
  local dest=$1; shift
  if [ $# -eq 1 ]; then cp "$1" "$dest"; else lipo -create "$@" -output "$dest"; fi
  chmod 755 "$dest"
}

# --- Signing identity -------------------------------------------------------
identity=${CONVT_SIGN_IDENTITY:-}
if [ -z "$identity" ]; then
  ids=$(security find-identity -v -p codesigning 2>/dev/null || true)
  identity=$(echo "$ids" | sed -n 's/.*"\(Developer ID Application:[^"]*\)".*/\1/p' | head -1)
  [ -n "$identity" ] || identity=$(echo "$ids" | sed -n 's/.*"\(Apple Development:[^"]*\)".*/\1/p' | head -1)
  [ -n "$identity" ] || identity=-
  # Over SSH the login keychain is locked, and codesign fails late with
  # errSecInternalComponent. Probe first and fall back to ad-hoc; an identity
  # named in CONVT_SIGN_IDENTITY is used as given and fails loudly.
  if [ "$identity" != - ]; then
    probe=$(mktemp /tmp/convt-sign-probe.XXXXXX)
    cp /usr/bin/true "$probe"
    if ! codesign --force --sign "$identity" --timestamp=none "$probe" 2>/dev/null; then
      echo "warning: $identity can't sign here (locked keychain?); signing ad-hoc" >&2
      identity=-
    fi
    rm -f "$probe"
  fi
fi
team=${CONVT_TEAM_ID:-}
if [ -z "$team" ] && [ "$identity" != - ]; then
  team=$(security find-certificate -c "$identity" -p | openssl x509 -noout -subject \
    | sed -n 's/.*OU *= *\([A-Z0-9]\{10\}\).*/\1/p')
fi
if [ -n "$team" ] && [ "$identity" = - ]; then
  # macOS only shares a team's App Group with code that team signed. An ad-hoc
  # app that names one gets a Finder menu with no formats whose "Open in
  # convt…" fails. Over SSH, build like this and then run resign.sh in the
  # Mac's own session, where the keychain is unlocked.
  if [ "${CONVT_ADHOC_WITH_TEAM:-}" != 1 ]; then
    echo "error: CONVT_TEAM_ID=$team but signing ad-hoc: the Finder menu would be broken." >&2
    echo "Sign with the team's identity, or set CONVT_ADHOC_WITH_TEAM=1 and run packaging/macos/resign.sh afterwards." >&2
    exit 1
  fi
  echo "warning: ad-hoc build with team $team; run packaging/macos/resign.sh before using the Finder menu" >&2
fi
if [ -n "$team" ]; then
  app_group="$team.app.convt.desktop"
else
  # Ad-hoc: no team, so the extension can't share a container with the app;
  # it falls back to "Open in convt…".
  app_group="app.convt.desktop"
fi
case "$identity" in "Developer ID Application:"*) hardened=${CONVT_HARDENED:-1} ;; *) hardened=${CONVT_HARDENED:-0} ;; esac
echo "identity: $identity; team: ${team:-none}; app group: $app_group; hardened runtime: $hardened; archs: $archs"

# --- Rust binaries ----------------------------------------------------------
bin_dir() { # arch -> release dir
  if [ "$1" = "$host" ] && [ "$archs" = "$host" ]; then echo "$target_dir/release"; else echo "$target_dir/$(triple "$1")/release"; fi
}
if [ "${CONVT_SKIP_CARGO:-}" != 1 ]; then
  for arch in $archs; do
    if [ "$(bin_dir "$arch")" = "$target_dir/release" ]; then
      (cd "$root" && CARGO_TARGET_DIR="$target_dir" cargo build --release --locked -p convt-cli -p convt-app)
    else
      rustup target add "$(triple "$arch")" >/dev/null
      (cd "$root" && CARGO_TARGET_DIR="$target_dir" cargo build --release --locked -p convt-cli -p convt-app --target "$(triple "$arch")")
    fi
  done
fi

rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Frameworks" "$app/Contents/Resources" "$app/Contents/PlugIns"
contents="$app/Contents"
for name in convt-app convt; do
  files=()
  for arch in $archs; do files+=("$(bin_dir "$arch")/$name"); done
  join_archs "$contents/MacOS/$name" "${files[@]}"
done

# --- FFmpeg and ffprobe -----------------------------------------------------
# Source-built per architecture (inputs.sh). The build directory defaults to
# the helper's output, $CONVT_BUNDLE_CACHE/macos-source-built-<arch>, and can
# be set with CONVT_MAC_FFMPEG_DIR_<arch>.
source_cache=${CONVT_BUNDLE_CACHE:-$root/packaging/.cache}
ffmpeg_lock="$root/packaging/release/macos-source-ffmpeg.lock.json"
ffmpeg_notices="$out/ffmpeg-notices"; rm -rf "$ffmpeg_notices"
unsourced=()
for tool in ffmpeg ffprobe; do
  files=()
  for arch in $archs; do
    case "$(var "FFMPEG_SOURCE_$arch")" in
      source)
        eval "dir=\${CONVT_MAC_FFMPEG_DIR_$arch:-}"
        dir=${dir:-$source_cache/macos-source-built-$arch}
        if [ "$tool" = ffmpeg ]; then
          /usr/bin/python3 "$here/ffmpeg-input.py" "$dir" "$arch" "$ffmpeg_lock" "$ffmpeg_notices"
        fi
        files+=("$dir/bin/$tool")
        ;;
      riedl)
        if [ "${CONVT_MAC_UNSOURCED_FFMPEG:-}" != 1 ]; then
          echo "NOT READY: $arch FFmpeg has no source build. Build it with packaging/release/macos-source-ffmpeg-build.sh --arch $arch," >&2
          echo "or set CONVT_MAC_UNSOURCED_FFMPEG=1 for a local test bundle that release.sh will refuse." >&2
          exit 1
        fi
        [ "$tool" = ffmpeg ] && unsourced+=("$arch")
        upper=$(echo "$tool" | tr a-z A-Z)
        zip="$cache/$tool-$FFMPEG_VERSION-$arch.zip"
        fetch "$(var "FFMPEG_URL_$arch")/$tool.zip" "$(var "${upper}_SHA256_$arch")" "$zip"
        dir="$cache/$tool-$FFMPEG_VERSION-$arch"
        rm -rf "$dir"; mkdir -p "$dir"
        ditto -x -k "$zip" "$dir"
        files+=("$dir/$tool")
        ;;
      *) echo "inputs.sh names no FFmpeg source for $arch" >&2; exit 1 ;;
    esac
  done
  join_archs "$contents/MacOS/$tool" "${files[@]}"
done

# --- PDFium ------------------------------------------------------------------
case "$archs" in
  arm64) pdfium=arm64; asset=mac-arm64 ;;
  x86_64) pdfium=x86_64; asset=mac-x64 ;;
  *) pdfium=universal; asset=mac-univ ;;
esac
tgz="$cache/pdfium-$PDFIUM_VERSION-$asset.tgz"
fetch "$PDFIUM_URL/pdfium-$asset.tgz" "$(var "PDFIUM_SHA256_$pdfium")" "$tgz"
rm -rf "$cache/pdfium-$asset"; mkdir -p "$cache/pdfium-$asset"
tar -xzf "$tgz" -C "$cache/pdfium-$asset"
cp "$cache/pdfium-$asset/lib/libpdfium.dylib" "$contents/Frameworks/"
# Every notice the PDFium archive carries, plus the compiler runtime notices
# (libc++, libc++abi, LLVM) from the sources pinned in pdfium-source.lock.json,
# which must be in $CONVT_BUNDLE_CACHE.
mkdir -p "$contents/Resources/licenses/PDFium"
cp "$cache/pdfium-$asset/LICENSE" "$contents/Resources/licenses/PDFium/LICENSE.txt"
[ -d "$cache/pdfium-$asset/licenses" ] || { echo "PDFium archive has no licenses/ directory" >&2; exit 1; }
cp -R "$cache/pdfium-$asset/licenses" "$contents/Resources/licenses/PDFium/components"
python=${CONVT_PYTHON:-python3}
"$python" -c 'import sys; sys.exit(sys.version_info < (3, 11))' 2>/dev/null \
  || { echo "installing PDFium runtime notices needs Python 3.11 or newer; set CONVT_PYTHON" >&2; exit 1; }
"$python" "$root/packaging/release/install-pdfium-notices.py" "$root/packaging/release/pdfium-source.lock.json" \
  "$source_cache" "$contents/Resources/licenses/PDFium/runtime" >/dev/null
cp "$root/LICENSE" "$contents/Resources/licenses/convt.txt"
mkdir -p "$contents/Resources/licenses/FFmpeg"
if [ -d "$ffmpeg_notices" ]; then
  cp -R "$ffmpeg_notices/." "$contents/Resources/licenses/FFmpeg/"
  rm -rf "$ffmpeg_notices"
fi
{
  echo "FFmpeg $FFMPEG_VERSION, GNU General Public License version 3 (--enable-gpl --enable-version3)."
  echo "Built from the sources listed in sources.json with the recipe and toolchain"
  echo "recorded in build-<arch>.json. The source archives are published with convt's"
  echo "corresponding source. components/ holds each component's notices."
  if [ ${#unsourced[@]} -gt 0 ]; then
    echo
    echo "NOT READY FOR DISTRIBUTION: the ${unsourced[*]} slice is Martin Riedl's prebuilt"
    echo "FFmpeg (https://ffmpeg.martin-riedl.de), whose exact corresponding source is unknown."
  fi
} > "$contents/Resources/licenses/FFmpeg/README.txt"
if [ ${#unsourced[@]} -gt 0 ]; then
  # release.sh refuses a bundle with this file.
  echo "${unsourced[*]}" > "$contents/Resources/licenses/FFmpeg/NOT-READY"
fi

# --- Finder Sync extension ---------------------------------------------------
ext_src="$root/integrations/macos/FinderSync"
appex="$contents/PlugIns/FinderSync.appex"
mkdir -p "$appex/Contents/MacOS" "$appex/Contents/Resources"
sdk=$(xcrun --sdk macosx --show-sdk-path)
files=()
for arch in $archs; do
  obj="$out/FinderSync-$arch"
  xcrun --sdk macosx swiftc -O -target "$arch-apple-macos$min_macos" -sdk "$sdk" \
    -module-name ConvtFinderSync -parse-as-library -application-extension \
    -Xlinker -e -Xlinker _NSExtensionMain -framework FinderSync -framework Cocoa \
    "$ext_src/FinderSync.swift" -o "$obj"
  files+=("$obj")
done
join_archs "$appex/Contents/MacOS/FinderSync" "${files[@]}"
rm -f "${files[@]}"

fill() { # template dest
  sed -e "s/@VERSION@/$version/g" -e "s/@BUILD@/$build/g" -e "s/@APP_GROUP@/$app_group/g" \
    -e "s/@MIN_MACOS@/$min_macos/g" "$1" > "$2"
}
fill "$ext_src/Info.plist" "$appex/Contents/Info.plist"
fill "$here/Info.plist" "$contents/Info.plist"
printf 'APPL????' > "$contents/PkgInfo"

# --- Icons (rendered with the CLI just built) ---------------------------------
convt="$contents/MacOS/convt"
icons="$out/icons"; rm -rf "$icons"; mkdir -p "$icons/convt.iconset" "$icons/config" "$icons/data"
# Private config and data, so the builder's own license or trial never
# decides whether packaging can render the icons.
render() { # svg dpi
  CONVT_LICENSE_STORE=file CONVT_CONFIG_DIR="$icons/config" CONVT_DATA_DIR="$icons/data" \
    "$convt" "$1" --to png --dpi "$2" >/dev/null
}
cp "$root/packaging/linux/convt.svg" "$icons/convt.svg"
# The source is 128 px at 96 DPI; 768 DPI renders 1024 px.
render "$icons/convt.svg" 768
for size in 16 32 128 256 512; do
  sips -z $size $size "$icons/convt.png" --out "$icons/convt.iconset/icon_${size}x${size}.png" >/dev/null
  double=$((size * 2))
  sips -z $double $double "$icons/convt.png" --out "$icons/convt.iconset/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns "$icons/convt.iconset" -o "$contents/Resources/convt.icns"
cp "$ext_src/MenuIconTemplate.svg" "$icons/menu.svg"
render "$icons/menu.svg" 192
cp "$icons/menu.png" "$appex/Contents/Resources/MenuIconTemplate@2x.png"
sips -z 16 16 "$icons/menu.png" --out "$appex/Contents/Resources/MenuIconTemplate.png" >/dev/null
rm -rf "$icons"

# --- Signing (inside out, never --deep) -------------------------------------
entitlements="$out/entitlements"; mkdir -p "$entitlements"
fill "$here/convt.entitlements" "$entitlements/convt.entitlements"
fill "$ext_src/FinderSync.entitlements" "$entitlements/FinderSync.entitlements"
sign_args=(--force --sign "$identity")
if [ "$hardened" = 1 ]; then
  sign_args+=(--options runtime --timestamp)
else
  sign_args+=(--timestamp=none)
fi
codesign "${sign_args[@]}" "$contents/Frameworks/libpdfium.dylib"
for name in ffmpeg ffprobe convt; do
  codesign "${sign_args[@]}" --identifier "app.convt.desktop.$name" "$contents/MacOS/$name"
done
codesign "${sign_args[@]}" --entitlements "$entitlements/FinderSync.entitlements" "$appex"
codesign "${sign_args[@]}" --entitlements "$entitlements/convt.entitlements" "$app"
rm -rf "$entitlements"
codesign --verify --strict --verbose=2 "$app"
echo "built $app ($(du -sh "$app" | cut -f1))"
