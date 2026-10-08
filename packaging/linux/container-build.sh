#!/usr/bin/env bash
# Runs inside the pinned manylinux_2_28 image, with networking disabled.
set -euo pipefail
# Hand everything back to the caller, including after a failed build.
trap 'chown -R "${CONVT_BUILD_UID:-0}:${CONVT_BUILD_GID:-0}" /work' EXIT
export PATH=/rust/bin:$PATH CARGO_HOME=/cargo CARGO_TARGET_DIR=/work/target
export RUST_FONTCONFIG_DLOPEN=1
# Empty build options use the source defaults rather than baking an empty URL.
[[ -n ${CONVT_DOCUMENT_PACK_URL:-} ]] || unset CONVT_DOCUMENT_PACK_URL
[[ -n ${CONVT_DOCUMENT_PACK_SHA256:-} ]] || unset CONVT_DOCUMENT_PACK_SHA256
[[ -n ${CONVT_LICENSE_PUBKEY:-} ]] || unset CONVT_LICENSE_PUBKEY
[[ -n ${CONVT_LICENSE_ENFORCE:-} ]] || unset CONVT_LICENSE_ENFORCE
jobs=${CONVT_BUILD_JOBS:-8}
cd /repo
# build.sh resolves one build date. It is the licence update cutoff and the
# timestamp of everything reproducible, so it must arrive from outside.
[[ ${SOURCE_DATE_EPOCH:-} =~ ^[0-9]+$ && ${CONVT_BUILD_DATE:-} =~ ^[0-9]{4}-[0-9]{2}-[0-9]{2}$ ]] || { echo 'SOURCE_DATE_EPOCH and CONVT_BUILD_DATE are required' >&2; exit 1; }
[[ $(date -u -d "@$SOURCE_DATE_EPOCH" +%F) == "$CONVT_BUILD_DATE" ]] || { echo 'CONVT_BUILD_DATE disagrees with SOURCE_DATE_EPOCH' >&2; exit 1; }
# gpui pulls in ahash with compile-time-rng, whose const-random seeds
# from getrandom on every build unless CONST_RANDOM_SEED is set.
export SOURCE_DATE_EPOCH CONVT_BUILD_DATE CARGO_INCREMENTAL=0 CONST_RANDOM_SEED="convt-$SOURCE_DATE_EPOCH"
[[ $(rustc --version) == 'rustc 1.95.0 '* ]]
# Reset only generated payload directories when reusing the compilation cache.
python3 - <<'RESET'
import pathlib,shutil
for name in ['convt','validation-tools','ffmpeg','native']:
    path=pathlib.Path('/work')/name
    if path.exists(): shutil.rmtree(path)
RESET
mkdir -p /work/source /work/native /work/convt/lib/libheif/plugins /work/convt/licenses /work/convt/share
python3 - <<'SOURCES'
import json,pathlib,subprocess,shutil
items=json.load(open('packaging/linux/inputs.lock.json'))
for package in ['x265','libde265','aom','libheif']:
    original=next(i for i in items if i.get('package')==package and '.orig.tar.' in i['name'])
    patches=next(i for i in items if i.get('package')==package and '.debian.tar.' in i['name'])
    root=pathlib.Path('/work/source')/package
    if root.exists(): shutil.rmtree(root)
    root.mkdir()
    subprocess.run(['tar','xf','/inputs/'+original['name'],'--strip-components=1','-C',str(root)],check=True)
    subprocess.run(['tar','xf','/inputs/'+patches['name'],'-C',str(root)],check=True)
    series=root/'debian/patches/series'
    if series.exists():
        for line in series.read_text().splitlines():
            line=line.split('#')[0].strip()
            if line: subprocess.run(['patch','-p1','-i',str(root/'debian/patches'/line.split()[0])],cwd=root,check=True)
SOURCES
# x265 3.5 explicitly selects two policies removed by CMake 4. Use their
# supported NEW behaviour; this recipe records the source modification.
sed -i -e 's/SET CMP0025 OLD/SET CMP0025 NEW/' -e 's/SET CMP0054 OLD/SET CMP0054 NEW/' /work/source/x265/source/CMakeLists.txt
# Test hook for the reproducibility check: shift the build's wall clock.
if [[ -n ${CONVT_BUILD_CLOCK_OFFSET_DAYS:-} ]]; then
  gcc -shared -fPIC -O2 -o /work/clock-offset.so packaging/linux/clock-offset.c -ldl
  export CONVT_CLOCK_OFFSET_SECONDS=$((CONVT_BUILD_CLOCK_OFFSET_DAYS * 86400)) LD_PRELOAD=/work/clock-offset.so
  echo "Simulated build clock: $(date -u)"
fi
export PKG_CONFIG_PATH=/work/native/lib/pkgconfig
export LD_LIBRARY_PATH=/work/native/lib
common=(-DCMAKE_BUILD_TYPE=Release -DCMAKE_INSTALL_PREFIX=/work/native -DCMAKE_INSTALL_LIBDIR=lib -DCMAKE_POLICY_VERSION_MINIMUM=3.5)
cmake -S /work/source/x265/source -B /work/source/x265/build "${common[@]}" -DENABLE_SHARED=ON -DENABLE_CLI=OFF -DENABLE_LIBNUMA=OFF
cmake --build /work/source/x265/build -j "$jobs"
cmake --install /work/source/x265/build
cmake -S /work/source/libde265 -B /work/source/libde265/build "${common[@]}" -DBUILD_SHARED_LIBS=ON -DENABLE_DEC265=OFF -DENABLE_SDL=OFF
cmake --build /work/source/libde265/build -j "$jobs"
cmake --install /work/source/libde265/build
cmake -S /work/source/aom -B /work/source/aom/build "${common[@]}" -DBUILD_SHARED_LIBS=ON -DENABLE_TESTS=OFF -DENABLE_EXAMPLES=OFF -DENABLE_TOOLS=OFF -DENABLE_DOCS=OFF -DCONFIG_LIBYUV=0 -DCONFIG_WEBM_IO=0
cmake --build /work/source/aom/build -j "$jobs"
cmake --install /work/source/aom/build
patch -d /work/source/libheif -p1 < packaging/linux/libheif-explicit-init.patch
cmake -S /work/source/libheif -B /work/source/libheif/build "${common[@]}" -DBUILD_SHARED_LIBS=ON -DBUILD_TESTING=OFF -DWITH_EXAMPLES=OFF -DENABLE_PLUGIN_LOADING=ON -DWITH_LIBDE265=ON -DWITH_LIBDE265_PLUGIN=ON -DWITH_X265=ON -DWITH_X265_PLUGIN=ON -DWITH_AOM_DECODER=ON -DWITH_AOM_DECODER_PLUGIN=ON -DWITH_AOM_ENCODER=ON -DWITH_AOM_ENCODER_PLUGIN=ON -DWITH_DAV1D=OFF -DWITH_RAV1E=OFF -DWITH_SvtEnc=OFF -DWITH_LIBSHARPYUV=OFF
cmake --build /work/source/libheif/build -j "$jobs"
cmake --install /work/source/libheif/build
# Main-executable DT_RPATH is transitive: native libraries and their plugins
# retain the private closure without exporting a loader path to Office children.
# Keep $ORIGIN literal for the ELF loader, independent of the install location.
RUSTFLAGS="${RUSTFLAGS:-} -C link-arg=-Wl,--disable-new-dtags -C link-arg=-Wl,-rpath,\$ORIGIN/lib" \
  cargo build --offline --locked --release -p convt-cli -p convt-app -j "$jobs"
stage=/work/convt
install -m755 /work/target/release/convt "$stage/convt.bin"
install -m755 /work/target/release/convt-app "$stage/convt-app.bin"
install -m755 packaging/linux/launch.sh "$stage/convt"
install -m755 packaging/linux/launch.sh "$stage/convt-app"
cp -a /work/native/lib/*.so* "$stage/lib/"
cp -a /work/native/lib/libheif/*.so "$stage/lib/libheif/plugins/"
tar xf /inputs/pdfium.tgz -C /work/native
cp /work/native/lib/libpdfium.so "$stage/lib/"
bash /repo/packaging/linux/build-ffmpeg.sh
# A newer C++ runtime must also support host desktop drivers loaded by GPUI.
# These hash-locked conda-forge builds target glibc 2.17; the final ELF audit
# checks them again inside this baseline container.
python3 packaging/linux/compiler-runtime.py "$stage"
# Collect the complete native closure from this old build environment.
env -u LD_PRELOAD python3 packaging/linux/native-closure.py "$stage"
for path in "$stage/lib/"*.so* "$stage/lib/libheif/plugins/"*.so; do strip --strip-unneeded "$path"; done
cp -a /work/native/licenses "$stage/licenses/pdfium"
cp /work/native/LICENSE "$stage/licenses/pdfium-build-MIT.txt"
python3 packaging/release/install-pdfium-notices.py packaging/release/pdfium-source.lock.json /inputs "$stage/licenses/pdfium-runtime"
cp packaging/linux/{inputs.lock.json,build-rpms.lock.json,build-image.json,INVENTORY.md,libheif-explicit-init.patch} "$stage/licenses/"
cp LICENSE "$stage/licenses/convt-AGPL.txt"
# Tie the dependency notice map to this exact payload source snapshot.
sha256sum Cargo.lock | cut -d" " -f1 > "$stage/licenses/cargo-lock.sha256"
cp packaging/linux/{convt.desktop,convt.svg} "$stage/share/"
mkdir -p "$stage/share/integrations/nautilus"
cp integrations/linux/install.py "$stage/share/integrations/"
cp integrations/linux/nautilus/convt_nautilus.py "$stage/share/integrations/nautilus/"
# Preserve exact patched sources and recipes for the codecs we built.
mkdir -p "$stage/licenses/codecs"
for package in x265 libde265 aom libheif; do
 cp -a "/work/source/$package/debian/copyright" "$stage/licenses/codecs/$package.txt"
done
rpm -qa | sort > "$stage/licenses/build-rpms.txt"
# Build test tools with the same baseline; never ship these in the product.
mkdir -p /work/validation-tools
cargo test --offline --locked --release -p convt-engines --test matrix --no-run --message-format=json -j "$jobs" > /work/matrix-build.json
python3 - <<'MATRIX'
import json,shutil
for line in open('/work/matrix-build.json'):
    i=json.loads(line)
    if i.get('reason')=='compiler-artifact' and i['target']['name']=='matrix' and i.get('executable'):
        shutil.copy2(i['executable'],'/work/validation-tools/matrix')
shutil.copytree('/opt/python/cp312-cp312','/work/validation-tools/python',symlinks=False)
MATRIX
env -u LD_PRELOAD python3 packaging/linux/native-closure.py /work/validation-tools --python
unset LD_PRELOAD
# Audit the final payload: every ELF, including the static ffmpeg and ffprobe
# added after the closure was collected. The self-test first proves the audit
# rejects a newer-glibc need and dependencies outside the payload.
python3 packaging/linux/elf-audit.py --self-test
python3 packaging/linux/elf-audit.py "$stage" --report "$stage/licenses/native-ldd.txt"
python3 packaging/linux/elf-audit.py /work/validation-tools --lib validator-lib --lib python/lib --report /work/validation-tools/native-ldd.txt

