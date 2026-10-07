#!/usr/bin/env bash
# Runs inside the pinned builder image started by build-ffmpeg.sh.
# /recipe holds packaging/windows (read-only), /inputs the verified source
# tarballs (read-only) and /out the result.
set -euo pipefail
trap 'chown -R "${HOST_UID:-0}:${HOST_GID:-0}" /out' EXIT
jobs=${CONVT_BUILD_JOBS:-8}
host=x86_64-w64-mingw32
lock=/recipe/ffmpeg-source.lock.json
export DEBIAN_FRONTEND=noninteractive
apt-get update -qq
apt-get install -y -qq --no-install-recommends python3 >/dev/null
mapfile -t packages < <(python3 -c 'import json,sys;b=json.load(open(sys.argv[1]))["builder"];print("\n".join([f"{k}={v}" for k,v in b["apt_packages"].items()]+b["apt_tools"]))' "$lock")
apt-get install -y -qq --no-install-recommends "${packages[@]}" >/dev/null
# Fail if apt resolved anything other than the pinned linked toolchain.
python3 - "$lock" <<'PY'
import json,subprocess,sys
b=json.load(open(sys.argv[1]))['builder']
for name,version in b['apt_packages'].items():
    got=subprocess.check_output(['dpkg-query','-W','-f=${Version}',name]).decode()
    if got!=version:raise SystemExit(f'{name} {got} differs from pinned {version}')
PY
work=/work
prefix=$work/prefix
mkdir -p "$work/source" "$prefix" /out/bin /out/sources /out/licenses/ffmpeg-source /out/licenses/toolchain
exec > >(tee /out/build.log) 2>&1
python3 - "$lock" <<'PY'
import hashlib,json,pathlib,shutil,sys,tarfile
lock=json.load(open(sys.argv[1]))
for s in lock['sources']:
    p=pathlib.Path('/inputs')/s['name']
    if hashlib.sha256(p.read_bytes()).hexdigest()!=s['sha256']:raise SystemExit('Hash mismatch: '+s['name'])
    shutil.copyfile(p,pathlib.Path('/out/sources')/s['name'])
    dest=pathlib.Path('/work/source')/s['package'];dest.mkdir()
    with tarfile.open(p) as t:
        for m in t:
            parts=pathlib.PurePosixPath(m.name).parts
            if len(parts)<2:continue
            m.name=str(pathlib.PurePosixPath(*parts[1:]));t.extract(m,dest,filter='data')
PY
export PKG_CONFIG_PATH= PKG_CONFIG_LIBDIR=$prefix/lib/pkgconfig
export CC=$host-gcc CXX=$host-g++ AR=$host-ar RANLIB=$host-ranlib STRIP=$host-strip
export CFLAGS="-O2 -I$prefix/include" CXXFLAGS="-O2 -I$prefix/include" LDFLAGS="-L$prefix/lib"
src=$work/source

cd "$src/zlib"
make -f win32/Makefile.gcc -j "$jobs" PREFIX=$host- libz.a
install -D -m644 libz.a "$prefix/lib/libz.a"
install -D -m644 zlib.h zconf.h -t "$prefix/include"
mkdir -p "$prefix/lib/pkgconfig"
printf 'prefix=%s\nlibdir=${prefix}/lib\nincludedir=${prefix}/include\nName: zlib\nDescription: zlib\nVersion: 1.3.1\nLibs: -L${libdir} -lz\nCflags: -I${includedir}\n' "$prefix" > "$prefix/lib/pkgconfig/zlib.pc"

cd "$src/x264"
./configure --prefix="$prefix" --host=$host --cross-prefix=$host- --enable-static --disable-cli --disable-opencl
make -j "$jobs" && make install

# x265 assembly stays off, matching the macOS and MSVC codec builds. CMake
# reads the release tag from x265Version.txt only when git is installed.
cmake -S "$src/x265/source" -B "$src/x265/build" -DCMAKE_SYSTEM_NAME=Windows \
  -DCMAKE_C_COMPILER=$host-gcc -DCMAKE_CXX_COMPILER=$host-g++ -DCMAKE_RC_COMPILER=$host-windres \
  -DCMAKE_BUILD_TYPE=Release -DCMAKE_INSTALL_PREFIX="$prefix" \
  -DENABLE_SHARED=OFF -DENABLE_CLI=OFF -DENABLE_ASSEMBLY=OFF
cmake --build "$src/x265/build" --parallel "$jobs"
cmake --install "$src/x265/build"
# The generated x265.pc lists the C++ runtime for dynamic toolchains; FFmpeg links statically.
sed -i 's/^Libs.private:.*/Libs.private: -lstdc++/' "$prefix/lib/pkgconfig/x265.pc"

cd "$src/libvpx"
CROSS=$host- ./configure --prefix="$prefix" --target=x86_64-win64-gcc --enable-static --disable-shared \
  --disable-examples --disable-tools --disable-docs --disable-unit-tests
make -j "$jobs" && make install

for package in opus lame libogg libvorbis; do
  cd "$src/$package"
  case $package in
    opus) ./configure --prefix="$prefix" --host=$host --enable-static --disable-shared --disable-extra-programs --disable-doc --disable-stack-protector ;;
    lame) ./configure --prefix="$prefix" --host=$host --enable-static --disable-shared --disable-frontend --disable-decoder --disable-gtktest ;;
    *) ./configure --prefix="$prefix" --host=$host --enable-static --disable-shared ;;
  esac
  make -j "$jobs" && make install
done

cat > "$work/dav1d-cross.ini" <<EOF
[binaries]
c = '$host-gcc'
ar = '$host-ar'
strip = '$host-strip'
windres = '$host-windres'
pkgconfig = 'pkg-config'
[host_machine]
system = 'windows'
cpu_family = 'x86_64'
cpu = 'x86_64'
endian = 'little'
EOF
meson setup "$src/dav1d/build" "$src/dav1d" --cross-file "$work/dav1d-cross.ini" --prefix="$prefix" --libdir=lib \
  --default-library=static --buildtype=release -Denable_tools=false -Denable_tests=false -Denable_examples=false -Denable_docs=false
ninja -C "$src/dav1d/build" -j "$jobs"
meson install -C "$src/dav1d/build"

# --disable-autodetect keeps undeclared host libraries out; -static keeps the
# GCC runtime and winpthreads out of the DLL closure.
cd "$src/ffmpeg"
# The native gcc builds only FFmpeg's host-side table generators.
./configure --prefix="$prefix" --arch=x86_64 --target-os=mingw32 --cross-prefix=$host- --enable-cross-compile \
  --pkg-config=pkg-config --pkg-config-flags=--static \
  --disable-autodetect --disable-debug --disable-doc --disable-shared --enable-static \
  --enable-gpl --enable-version3 --enable-w32threads --enable-libx264 --enable-libx265 \
  --enable-libdav1d --enable-libvpx --enable-libopus --enable-libmp3lame --enable-libvorbis --enable-zlib \
  --extra-cflags="-I$prefix/include" --extra-ldflags="-L$prefix/lib -static" --extra-libs=-lstdc++
make -j "$jobs"
install -m755 ffmpeg.exe ffprobe.exe /out/bin/
cp ffbuild/config.log /out/ffmpeg-config.log
cp /recipe/ffmpeg-source.lock.json /recipe/build-ffmpeg.sh /recipe/build-ffmpeg-inside.sh /out/
$host-objdump -p /out/bin/ffmpeg.exe | sed -n 's/^\s*DLL Name: //p' | sort -u > /out/dll-imports.txt
dpkg-query -W -f='${Package} ${Version}\n' | sort > /out/builder-packages.txt
$host-gcc --version | head -1 > /out/toolchain.txt
nasm -v >> /out/toolchain.txt
cmake --version | head -1 >> /out/toolchain.txt
echo "meson $(meson --version)" >> /out/toolchain.txt
for doc in gcc-mingw-w64-base mingw-w64-common libgcc-s1 libstdc++6; do
  [[ -f /usr/share/doc/$doc/copyright ]] && cp "/usr/share/doc/$doc/copyright" "/out/licenses/toolchain/$doc-copyright.txt"
done
python3 - <<'PY'
import hashlib,json,os,pathlib,re,shutil
out=pathlib.Path('/out');lock=json.load(open('/recipe/ffmpeg-source.lock.json'))
for s in lock['sources']:
    root=pathlib.Path('/work/source')/s['package'];dest=out/'licenses/ffmpeg-source'/s['package'];count=0
    for p in sorted(root.rglob('*')):
        if '/build/' in str(p) or not p.is_file() or p.is_symlink():continue
        if re.search(r'COPYING|LICENSE|NOTICE|PATENTS',p.name,re.I):
            t=dest/p.relative_to(root);t.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(p,t);count+=1
    if not count:raise SystemExit('No notices: '+s['package'])
# Only Windows system DLLs may be imported.
system={'advapi32.dll','bcrypt.dll','gdi32.dll','kernel32.dll','msvcrt.dll','ole32.dll','oleaut32.dll','psapi.dll','shell32.dll','shlwapi.dll','user32.dll','ws2_32.dll','secur32.dll','crypt32.dll','vfw32.dll','avicap32.dll','strmiids.dll','mfplat.dll','mfuuid.dll'}
imports=(out/'dll-imports.txt').read_text().split()
extra=[d for d in imports if d.lower() not in system and not d.lower().startswith('api-ms-win-')]
if extra:raise SystemExit('Non-system DLL imports: '+', '.join(extra))
receipt={'schema_version':1,'platform':'windows-x86_64','source_inputs':lock['sources'],'builder':lock['builder'],
 'binaries':{n:hashlib.sha256((out/'bin'/n).read_bytes()).hexdigest() for n in ('ffmpeg.exe','ffprobe.exe')},
 'dll_imports':imports}
(out/'receipt.json').write_text(json.dumps(receipt,indent=2)+'\n')
PY
echo 'Windows FFmpeg built from pinned sources'
