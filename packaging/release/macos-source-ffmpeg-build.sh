#!/usr/bin/env bash
# Source-built replacement, never evidence for the historical Riedl binaries.
set -euo pipefail
script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
repo=$(cd -- "$script_dir/../.." && pwd)
cache=${CONVT_BUNDLE_CACHE:-$repo/packaging/.cache}
arch=$(uname -m)
jobs=${CONVT_BUILD_JOBS:-8}
output=
while (($#)); do
  case "$1" in
    --arch) arch=${2:?}; shift 2 ;;
    --output) output=${2:?}; shift 2 ;;
    --jobs) jobs=${2:?}; shift 2 ;;
    --help) echo 'Usage: bash macos-source-ffmpeg-build.sh [--arch arm64|x86_64] [--output CACHE/PATH] [--jobs N]'; exit 0 ;;
    *) echo "Unknown argument: $1" >&2; exit 2 ;;
  esac
done
case "$arch" in arm64|x86_64) ;; *) echo 'Architecture must be arm64 or x86_64' >&2; exit 2 ;; esac
[[ $(uname -s) == Darwin ]] || { echo 'NOT CHECKED: macOS compiler/SDK required; this helper cannot build on Linux.' >&2; exit 3; }
[[ $jobs =~ ^[1-9][0-9]*$ ]] || { echo 'Jobs must be a positive integer' >&2; exit 2; }
for tool in python3 xcrun xcodebuild clang clang++ cmake make pkg-config meson ninja otool; do
  command -v "$tool" >/dev/null || { echo "Missing declared build prerequisite: $tool" >&2; exit 2; }
done
export CONVT_BUNDLE_CACHE=$cache
export MACOSX_DEPLOYMENT_TARGET=${MACOSX_DEPLOYMENT_TARGET:-11.0}
export SOURCE_DATE_EPOCH=${SOURCE_DATE_EPOCH:-1791331200}
output=${output:-$cache/macos-source-built-$arch}
export CONVT_MACOS_SOURCE_OUTPUT=$output CONVT_MACOS_SOURCE_ARCH=$arch CONVT_MACOS_SOURCE_REPO=$repo
# No downloads occur here. Fetch and validate the lock before disabling the builder's network.
python3 - "$script_dir/macos-source-ffmpeg.lock.json" <<'PY'
import hashlib,json,os,pathlib,tarfile
lock=json.load(open(__import__('sys').argv[1]));cache=pathlib.Path(os.environ['CONVT_BUNDLE_CACHE']).resolve();out=pathlib.Path(os.environ['CONVT_MACOS_SOURCE_OUTPUT']).resolve()
if not out.is_relative_to(cache):raise SystemExit('Build output must remain inside CONVT_BUNDLE_CACHE')
if out.exists():raise SystemExit('Refusing existing build output: '+str(out))
recipe=lock['source_build_alternative']['build_recipe'];script=pathlib.Path(os.environ['CONVT_MACOS_SOURCE_REPO'])/recipe['path'];assert hashlib.sha256(script.read_bytes()).hexdigest()==recipe['sha256'],'Build script differs from lock'
inputs=lock['source_build_alternative']['sources'];assert {a['package'] for a in inputs}=={'ffmpeg','x264','x265','libvpx','opus','lame','libogg','libvorbis','zlib','dav1d'}
for a in inputs:
 p=(cache/a['cache_filename']).resolve();assert p.is_relative_to(cache)
 assert hashlib.sha256(p.read_bytes()).hexdigest()==a['sha256'],a['package']
# Validate all before mutating the output. Retain each exact input for source delivery.
(out/'source').mkdir(parents=True);(out/'prefix').mkdir();(out/'bin').mkdir();(out/'licenses').mkdir()
for a in inputs:
 p=cache/a['cache_filename'];dest=out/'source'/a['package'];dest.mkdir()
 with tarfile.open(p) as t:
  for m in t:
   parts=pathlib.PurePosixPath(m.name).parts
   if len(parts)<2:continue
   m.name=str(pathlib.PurePosixPath(*parts[1:]));t.extract(m,dest,filter='data')
(out/'source-inputs.json').write_text(json.dumps(inputs,indent=2)+'\n')
PY
output=$(cd -- "$output" && pwd)
prefix=$output/prefix
sdk=$(xcrun --sdk macosx --show-sdk-path)
export SDKROOT=$sdk
export CC=clang CXX=clang++ AR=ar RANLIB=ranlib
export CFLAGS="-O2 -fPIC -arch $arch -isysroot $sdk -mmacosx-version-min=$MACOSX_DEPLOYMENT_TARGET"
export CXXFLAGS=$CFLAGS
export CPPFLAGS="-I$prefix/include"
export LDFLAGS="-arch $arch -isysroot $sdk -mmacosx-version-min=$MACOSX_DEPLOYMENT_TARGET -L$prefix/lib"
export PKG_CONFIG_PATH= PKG_CONFIG_LIBDIR=$prefix/lib/pkgconfig
case "$arch" in arm64) triple=aarch64-apple-darwin; ffarch=aarch64 ;; x86_64) triple=x86_64-apple-darwin; ffarch=x86_64 ;; esac
exec > >(tee "$output/build.log") 2>&1
for package in zlib x264 libvpx opus lame libogg libvorbis; do
  cd -- "$output/source/$package"
  # libvorbis 1.3.7 adds -force_cpusubtype_ALL on Darwin, which Xcode 15+ ld rejects.
  if [[ $package == libvorbis ]]; then sed -i '' 's/-force_cpusubtype_ALL//g' configure; fi
  case "$package" in
    zlib) ./configure --prefix="$prefix" --static ;;
    x264) ./configure --prefix="$prefix" --host="$triple" --enable-static --enable-pic --disable-cli --disable-opencl --disable-asm ;;
    libvpx) ./configure --prefix="$prefix" --target=generic-gnu --enable-static --disable-shared --enable-pic --disable-examples --disable-tools --disable-docs --disable-unit-tests --disable-runtime-cpu-detect ;;
    opus) ./configure --prefix="$prefix" --host="$triple" --enable-static --disable-shared --disable-extra-programs --disable-doc ;;
    lame) ./configure --prefix="$prefix" --host="$triple" --enable-static --disable-shared --disable-frontend --disable-decoder ;;
    *) ./configure --prefix="$prefix" --host="$triple" --enable-static --disable-shared ;;
  esac
  make -j "$jobs"
  make install
done
# x265 3.5 forces CMP0025/CMP0054 to OLD, which CMake 4 rejects; NEW plus a
# MATCHES test keeps AppleClang on x265's Clang path.
sed -i '' -e '/cmake_policy(SET CMP0025 OLD)/d' -e '/cmake_policy(SET CMP0054 OLD)/d' \
  -e 's/if(${CMAKE_CXX_COMPILER_ID} STREQUAL "Clang")/if(${CMAKE_CXX_COMPILER_ID} MATCHES "Clang")/' \
  "$output/source/x265/source/CMakeLists.txt"
cmake -S "$output/source/x265/source" -B "$output/source/x265/build" \
  -DCMAKE_BUILD_TYPE=Release -DCMAKE_INSTALL_PREFIX="$prefix" \
  -DCMAKE_OSX_ARCHITECTURES="$arch" -DCMAKE_OSX_DEPLOYMENT_TARGET="$MACOSX_DEPLOYMENT_TARGET" \
  -DCMAKE_OSX_SYSROOT="$sdk" -DENABLE_SHARED=OFF -DENABLE_CLI=OFF -DENABLE_ASSEMBLY=OFF \
  -DENABLE_PIC=ON -DCMAKE_POLICY_VERSION_MINIMUM=3.5
cmake --build "$output/source/x265/build" --parallel "$jobs"
cmake --install "$output/source/x265/build"
# Build the software AV1 decoder for the selected target, including cross builds.
# Optional assembler discovery is disabled, matching the portable codec build.
python3 - "$output/dav1d-cross.ini" "$ffarch" <<'PYDAV1D'
import os,pathlib,shlex,sys
pathlib.Path(sys.argv[1]).write_text(
    "[binaries]\nc = 'clang'\nar = 'ar'\nstrip = 'strip'\npkgconfig = 'pkg-config'\n"
    "[host_machine]\nsystem = 'darwin'\ncpu_family = " + repr(sys.argv[2]) +
    "\ncpu = " + repr(sys.argv[2]) + "\nendian = 'little'\n"
    "[properties]\nneeds_exe_wrapper = true\n[built-in options]\nc_args = " +
    repr(shlex.split(os.environ['CFLAGS'])) + "\nc_link_args = " +
    repr(shlex.split(os.environ['LDFLAGS'])) + "\n")
PYDAV1D
meson setup "$output/source/dav1d/build" "$output/source/dav1d" \
  --cross-file "$output/dav1d-cross.ini" --prefix="$prefix" --libdir=lib \
  --default-library=static --buildtype=release -Db_staticpic=true \
  -Denable_asm=false -Denable_tools=false -Denable_tests=false -Denable_examples=false -Denable_docs=false
ninja -C "$output/source/dav1d/build" -j "$jobs"
meson install -C "$output/source/dav1d/build"
# --disable-autodetect prevents undeclared host codecs; zlib preserves PNG decode/encode.
cd -- "$output/source/ffmpeg"
./configure --prefix="$prefix" --arch="$ffarch" --target-os=darwin --enable-cross-compile \
  --cc=clang --cxx=clang++ --pkg-config=pkg-config --pkg-config-flags=--static \
  --disable-autodetect --disable-debug --disable-doc --disable-shared --enable-static \
  --enable-gpl --enable-version3 --disable-asm --enable-libx264 --enable-libx265 \
  --enable-libdav1d --enable-libvpx --enable-libopus --enable-libmp3lame --enable-libvorbis --enable-zlib \
  --enable-videotoolbox --extra-cflags="$CFLAGS -I$prefix/include" \
  --extra-ldflags="$LDFLAGS" --extra-libs=-lpthread
make -j "$jobs"
install -m755 ffmpeg ffprobe "$output/bin/"
# Preserve scripts, full config log, sources, notices and toolchain identity beside the binaries.
cp "$script_dir/macos-source-ffmpeg-build.sh" "$output/"
cp "$script_dir/macos-source-ffmpeg.lock.json" "$output/"
cp config.h ffbuild/config.log ffbuild/config.mak "$output/"
xcodebuild -version > "$output/xcode-version.txt"
xcrun --sdk macosx --show-sdk-version > "$output/sdk-version.txt"
xcrun --sdk macosx --show-sdk-build-version > "$output/sdk-build-version.txt"
clang --version > "$output/clang-version.txt"
cmake --version > "$output/cmake-version.txt"
make --version > "$output/make-version.txt"
pkg-config --version > "$output/pkg-config-version.txt"
meson --version > "$output/meson-version.txt"
ninja --version > "$output/ninja-version.txt"
otool -L "$output/bin/ffmpeg" "$output/bin/ffprobe" > "$output/dynamic-libraries.txt"
python3 - "$output" <<'PY'
import hashlib,json,os,pathlib,re,shutil,subprocess,sys
out=pathlib.Path(sys.argv[1]);inputs=json.load(open(out/'source-inputs.json'));lock=json.load(open(out/'macos-source-ffmpeg.lock.json'))
for a in inputs:
 root=out/'source'/a['package'];dest=out/'licenses'/a['package'];dest.mkdir()
 count=0
 # Include every identified notice verbatim, preserving internal paths and encoding.
 for p in sorted(root.rglob('*')):
  if p.is_file() and not p.is_symlink() and re.search(r'COPYING|LICENSE|NOTICE|PATENTS|FTL\.TXT|README\.IJG',p.name,re.I):
   target=dest/p.relative_to(root);target.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(p,target);count+=1
 if not count:raise SystemExit('No retained component notices: '+a['package'])
for line in (out/'dynamic-libraries.txt').read_text().splitlines():
 if 'compatibility version' in line:
  name=line.strip().split(' (compatibility version')[0]
  if not name.startswith(('/usr/lib/','/System/Library/')):raise SystemExit('Undeclared dynamic library: '+name)
receipt={'schema_version':1,'architecture':os.environ['CONVT_MACOS_SOURCE_ARCH'],'deployment_target':os.environ['MACOSX_DEPLOYMENT_TARGET'],'source_date_epoch':int(os.environ['SOURCE_DATE_EPOCH']),'source_inputs':inputs,'build_recipe':lock['source_build_alternative']['build_recipe'],'binaries':[],'notices_directory':str(out/'licenses'),'source_directory':str(out/'source'),'build_log':str(out/'build.log'),'qualification':'New independent build. Does not close historical Martin Riedl binary source gaps. No readiness flags.'}
for tool in ['ffmpeg','ffprobe']:
 p=out/'bin'/tool;receipt['binaries'].append({'name':tool,'path':str(p),'sha256':hashlib.sha256(p.read_bytes()).hexdigest(),'size_bytes':p.stat().st_size})
receipt['toolchain']={n:(out/n).read_text() for n in ['xcode-version.txt','sdk-version.txt','sdk-build-version.txt','clang-version.txt','cmake-version.txt','make-version.txt','pkg-config-version.txt','meson-version.txt','ninja-version.txt']}
ffmpeg=out/'bin/ffmpeg';ffprobe=out/'bin/ffprobe';smoke=out/'smoke';smoke.mkdir();checks=[]
def run(args):return subprocess.run([str(ffmpeg),'-hide_banner','-loglevel','error','-y',*args],check=True,capture_output=True,text=True,timeout=120)
try:
 version=subprocess.run([str(ffmpeg),'-version'],check=True,capture_output=True,text=True,timeout=30).stdout;assert 'ffmpeg version 9.0.2' in version;receipt['configuration']=version
 decoders=subprocess.run([str(ffmpeg),'-hide_banner','-decoders'],check=True,capture_output=True,text=True,timeout=30).stdout
 assert 'libdav1d' in decoders, 'Pinned software AV1 decoder absent'
 checks.append({'decoder':'libdav1d','status':'PASS'})
 encoders=subprocess.run([str(ffmpeg),'-hide_banner','-encoders'],check=True,capture_output=True,text=True,timeout=30).stdout
 for encoder in ['libx264','libx265','libvpx-vp9','libopus','libmp3lame','libvorbis','png','h264_videotoolbox','hevc_videotoolbox']:assert encoder in encoders,encoder
 for codec,ext in [('libx264','mp4'),('libx265','mp4'),('libvpx-vp9','webm')]:
  p=smoke/(codec+'.'+ext);run(['-f','lavfi','-i','testsrc2=size=64x64:rate=5','-t','0.4','-an','-c:v',codec,str(p)]);assert p.stat().st_size>0;checks.append({'codec':codec,'status':'PASS','path':str(p)})
 for codec,ext in [('libmp3lame','mp3'),('libopus','opus'),('libvorbis','ogg')]:
  p=smoke/(codec+'.'+ext);run(['-f','lavfi','-i','sine=frequency=440:sample_rate=48000','-t','0.4','-c:a',codec,str(p)]);assert p.stat().st_size>0;checks.append({'codec':codec,'status':'PASS','path':str(p)})
 p=smoke/'zlib-png.png';run(['-f','lavfi','-i','color=size=16x16','-frames:v','1','-threads','1',str(p)]);run(['-i',str(p),'-f','null','-']);checks.append({'codec':'PNG encode/decode with zlib','status':'PASS','path':str(p)})
 subprocess.run([str(ffprobe),'-v','error','-show_streams',str(smoke/'libx264.mp4')],check=True,capture_output=True,text=True,timeout=30);checks.append({'tool':'ffprobe','status':'PASS'})
 for codec in ['h264_videotoolbox','hevc_videotoolbox']:
  try:run(['-f','lavfi','-i','testsrc2=size=128x128:rate=5','-t','0.4','-an','-c:v',codec,'-allow_sw','1',str(smoke/(codec+'.mp4'))]);checks.append({'codec':codec,'status':'PASS'})
  except subprocess.CalledProcessError as e:checks.append({'codec':codec,'status':'NOT CHECKED','detail':e.stderr[-1000:]})
except OSError as e:
 checks.append({'status':'NOT CHECKED','detail':'Target architecture cannot execute on this Mac: '+str(e)})
except (subprocess.SubprocessError,AssertionError) as e:
 receipt['checks']=checks+[{'status':'FAIL','detail':str(e),'stderr':getattr(e,'stderr','')}];(out/'provenance.json').write_text(json.dumps(receipt,indent=2)+'\n');raise
receipt['checks']=checks;(out/'provenance.json').write_text(json.dumps(receipt,indent=2)+'\n')
print(json.dumps({'receipt':str(out/'provenance.json'),'binaries':receipt['binaries'],'checks':checks},indent=2))
PY
