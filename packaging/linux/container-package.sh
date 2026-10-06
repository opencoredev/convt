#!/usr/bin/env bash
set -euo pipefail
umask 022
trap 'chown -R "${CONVT_BUILD_UID:-0}:${CONVT_BUILD_GID:-0}" /work' EXIT
version=$CONVT_PACKAGE_VERSION
release=$CONVT_PACKAGE_RELEASE
if [[ $1 == deb ]]; then
  cp -a /work/root /work/deb-root
  find /work/deb-root/opt/convt/lib -type f -name '*.so*' -exec chmod 644 {} +
  mkdir /work/deb-root/DEBIAN
  cp /repo/packaging/linux/menu-migration-message.sh /work/deb-root/DEBIAN/postinst
  chmod 755 /work/deb-root/DEBIAN/postinst
  touch -d "@$SOURCE_DATE_EPOCH" /work/deb-root/DEBIAN/postinst
  # Filesystem directory allocation differs even for identical copied trees.
  # Estimate installed KiB from logical file/link bytes, independent of du.
  size=$(find /work/deb-root \( -type f -o -type l \) -printf '%s\n' | awk '{bytes += $1} END {print int((bytes + 1023) / 1024)}')
  cat > /work/deb-root/DEBIAN/control <<CONTROL
Package: convt
Version: $version-$release
Architecture: amd64
Maintainer: Convt <support@convt.app>
Section: graphics
Priority: optional
Homepage: https://convt.app
Installed-Size: $size
Depends: libc6 (>= 2.28), libvulkan1, libegl1, libwayland-client0, libwayland-egl1, libfontconfig1, fontconfig, xkb-data, libx11-6, libxcursor1, libxrandr2, libxi6, libxrender1
Recommends: python3-nautilus
Suggests: libreoffice
Description: local file converter with desktop integration
 Convert images, audio, video and PDF files locally. Documents use
 an optional document pack or an installed LibreOffice.
CONTROL
  find /work/deb-root -exec touch -h -d "@$SOURCE_DATE_EPOCH" {} +
  dpkg-deb --version > /work/artifacts/deb-tool-version.txt
  dpkg-deb --root-owner-group -Zxz -z9 --threads-max=1 --build /work/deb-root "/work/artifacts/convt_${version}-${release}_amd64.deb"
else
  mkdir -p /work/rpmbuild/{BUILD,BUILDROOT,RPMS,SOURCES,SPECS,SRPMS}
  cat > /work/rpmbuild/SPECS/convt.spec <<SPEC
Name: convt
Version: $version
Release: $release
Summary: Local file converter with desktop integration
License: $(cat /work/root/usr/share/doc/convt/rpm-license.txt)
URL: https://convt.app
BuildArch: x86_64
AutoReqProv: no
Requires: glibc >= 2.28
Requires: /bin/sh, coreutils
Requires: libvulkan.so.1()(64bit), libEGL.so.1()(64bit)
Requires: libwayland-client.so.0()(64bit), libwayland-egl.so.1()(64bit)
Requires: libfontconfig.so.1()(64bit), fontconfig, xkeyboard-config
Requires: libX11.so.6()(64bit), libXcursor.so.1()(64bit), libXrandr.so.2()(64bit)
Requires: libXi.so.6()(64bit), libXrender.so.1()(64bit)
Recommends: nautilus-python
Suggests: libreoffice

%description
Convert images, audio, video and PDF files locally. Documents use
an optional document pack or an installed LibreOffice.

%prep
%build
%install
mkdir -p %{buildroot}
cp -a /work/root/. %{buildroot}/

%post
/usr/share/convt/menu-migration-message.sh

%files
/opt/convt
/usr/bin/convt
/usr/bin/convt-app
/usr/share/applications/convt-app.desktop
/usr/share/icons/hicolor/scalable/apps/convt.svg
/usr/share/metainfo/app.convt.convt.metainfo.xml
/usr/share/kio/servicemenus/convt-*.desktop
/usr/share/nemo/actions/convt-*.nemo_action
/usr/share/nautilus-python/extensions/convt_nautilus.py
/usr/share/convt
/usr/share/man/man1/convt*.1.gz
%doc /usr/share/doc/convt/changelog.Debian.gz
%license /usr/share/doc/convt/copyright
%license /usr/share/doc/convt/component-summary.txt
%license /usr/share/doc/convt/rpm-license.txt
%license /usr/share/doc/convt/components.spdx.json
%license /usr/share/doc/convt/ffmpeg-static-components.json
%license /usr/share/doc/convt/rust-license-components.json

%changelog
* Sun Oct 04 2026 Convt <support@convt.app> - $version-$release
- Package the audited Linux payload and system integration.
SPEC
  rpm --version > /work/artifacts/rpm-tool-version.txt
  rpmbuild -bb --define '_topdir /work/rpmbuild' --define '_buildhost convt-builder' \
    --define 'use_source_date_epoch_as_buildtime 1' --define 'clamp_mtime_to_source_date_epoch 1' \
    --define '_binary_payload w9.xzdio' --define '_build_id_links none' \
    --define '__os_install_post %{nil}' /work/rpmbuild/SPECS/convt.spec
  cp /work/rpmbuild/RPMS/x86_64/*.rpm /work/artifacts/
fi
