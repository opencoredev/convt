#!/bin/sh
# Executed only inside the locked, network-disabled Alpine build container.
set -eu
apk add --no-network --repositories-file /dev/null /apks/*.apk
apk info -vv | sort > /output/installed-apks.txt
mkdir -p /build
cd /build
tar xf /sources/fuse-3.15.0.tar.xz
tar xf /sources/squashfuse-0.5.2.tar.gz
tar xf /sources/appimage-runtime-source.tar.gz
runtime=type2-runtime-8f39b89e2ac31e1640b3d3f7e9a5108e6ce805fa
cd fuse-3.15.0
patch -p1 < "/build/$runtime/patches/libfuse/mount.c.diff"
meson setup --prefix=/usr --default-library=static build
ninja -C build -v install
cd /build/squashfuse-0.5.2
export CFLAGS="-ffunction-sections -fdata-sections -Os"
./autogen.sh
./configure LDFLAGS="-static"
cp config.log /output/squashfuse-config.log
make -j4
make install
mkdir -p /usr/local/include/squashfuse
install -m644 ./*.h /usr/local/include/squashfuse
cd "/build/$runtime"
printf '%s\n' 'https://github.com/AppImage/type2-runtime/commit/8f39b89' > src/runtime/version
# Trace the actual implicit startup/runtime objects and static archives.
make -C src/runtime CC='clang -Wl,--trace' runtime > /output/link-inputs.txt 2>&1
strip --version > /output/strip-version.txt
clang --version > /output/clang-version.txt
cp src/runtime/runtime /output/runtime-unstripped
objcopy --only-keep-debug src/runtime/runtime /output/runtime-x86_64.debug
strip --strip-debug --strip-unneeded src/runtime/runtime
objcopy --add-gnu-debuglink=/output/runtime-x86_64.debug src/runtime/runtime
printf 'AI\002' | dd of=src/runtime/runtime bs=1 count=3 seek=8 conv=notrunc
cp src/runtime/runtime /output/runtime-x86_64
/output/runtime-x86_64 --appimage-version > /output/runtime-version.txt
sha256sum /output/runtime-x86_64 > /output/runtime.sha256
chown -R "$BUILD_UID:$BUILD_GID" /output
