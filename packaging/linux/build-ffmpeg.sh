#!/usr/bin/env bash
# Run in the pinned baseline builder after x265 and aom installation.
set -euo pipefail
jobs=${CONVT_BUILD_JOBS:-8}
python3 - <<'PY'
import json,pathlib,shutil,subprocess
for i in json.load(open('/repo/packaging/linux/ffmpeg-source-inputs.lock.json')):
 p=pathlib.Path('/work/source')/i['package']
 if p.exists():shutil.rmtree(p)
 p.mkdir()
 subprocess.run(['tar','xf','/inputs/'+i['name'],'--strip-components=1','-C',str(p)],check=True)
PY
export PKG_CONFIG_PATH=/work/native/lib/pkgconfig
export PKG_CONFIG_LIBDIR=/work/native/lib/pkgconfig
export CFLAGS='-O2 -fPIC' CXXFLAGS='-O2 -fPIC'
for package in zlib x264 libvpx opus lame libogg libvorbis; do
 cd "/work/source/$package"
 case $package in
 zlib) ./configure --prefix=/work/native --static ;;
 x264) ./configure --prefix=/work/native --enable-static --enable-pic --disable-cli --disable-opencl ;;
 libvpx) ./configure --prefix=/work/native --enable-static --disable-shared --enable-pic --disable-examples --disable-tools --disable-docs --disable-unit-tests ;;
 *) ./configure --prefix=/work/native --libdir=/work/native/lib --enable-static --disable-shared ;;
 esac
 make -j "$jobs"
 make install
 done
cd /work/source/ffmpeg
./configure --prefix=/work/native --disable-autodetect --disable-debug --disable-doc \
 --pkg-config-flags=--static --disable-shared --enable-static --enable-gpl --enable-version3 \
 --enable-libx264 --enable-libx265 --enable-libvpx --enable-libopus \
 --enable-libmp3lame --enable-libvorbis --enable-libaom --enable-zlib \
 --extra-cflags=-I/work/native/include --extra-ldflags='-L/work/native/lib -Wl,--disable-new-dtags -Wl,-rpath,$ORIGIN/lib' \
 --extra-libs='-lpthread -lm'
make -j "$jobs"
make install
patchelf --force-rpath --set-rpath '$ORIGIN/lib' ffmpeg ffprobe
install -m755 ffmpeg ffprobe /work/convt/
cp COPYING.GPLv3 /work/convt/licenses/ffmpeg-GPLv3.txt
cp /repo/packaging/linux/ffmpeg-source-inputs.lock.json /work/convt/licenses/
/work/convt/ffmpeg -version > /work/convt/licenses/ffmpeg-configuration.txt
# Include every shipped library's own notice, including static subcomponents.
python3 - <<'PY'
import pathlib,re
out=pathlib.Path('/work/convt/licenses/ffmpeg-source');out.mkdir(exist_ok=True)
for package in ['ffmpeg','x264','libvpx','opus','lame','libogg','libvorbis','zlib']:
 root=pathlib.Path('/work/source')/package
 notices=[p for p in sorted(root.iterdir()) if p.is_file() and re.match(r'^(COPYING|LICENSE|PATENTS|AUTHORS)',p.name)]
 if not notices:raise SystemExit('Missing codec notice: '+package)
 (out/(package+'.txt')).write_text('\n\n'.join(p.name+'\n'+p.read_text(errors='replace') for p in notices))
PY
