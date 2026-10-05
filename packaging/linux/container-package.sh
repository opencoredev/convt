#!/usr/bin/env bash
set -euo pipefail
version=$CONVT_PACKAGE_VERSION
release=$CONVT_PACKAGE_RELEASE
if [[ $1 == deb ]]; then
  cp -a /work/root /work/deb-root
  mkdir /work/deb-root/DEBIAN
  size=$(du -sk /work/deb-root | cut -f1)
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
Recommends: libreoffice, python3-nautilus
Description: local file converter with desktop integration
 Convert images, audio, video and PDF files locally. Documents use
 an optional document pack or an installed LibreOffice.
CONTROL
  dpkg-deb --version > /work/artifacts/deb-tool-version.txt
  dpkg-deb --root-owner-group -Zxz -z9 --threads-max=1 --build /work/deb-root "/work/artifacts/convt_${version}-${release}_amd64.deb"
else
  mkdir -p /work/rpmbuild/{BUILD,BUILDROOT,RPMS,SOURCES,SPECS,SRPMS}
  cat > /work/rpmbuild/SPECS/convt.spec <<SPEC
Name: convt
Version: $version
Release: $release
Summary: Local file converter with desktop integration
License: AGPL-3.0-only
URL: https://convt.app
BuildArch: x86_64
AutoReqProv: no
Requires: glibc >= 2.28
Requires: vulkan-loader, libglvnd-egl, wayland, fontconfig, xkeyboard-config
Requires: libX11, libXcursor, libXrandr, libXi, libXrender
Recommends: libreoffice, nautilus-python

%description
Convert images, audio, video and PDF files locally. Documents use
an optional document pack or an installed LibreOffice.

%prep
%build
%install
mkdir -p %{buildroot}
cp -a /work/root/. %{buildroot}/

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
%license /usr/share/doc/convt/copyright
SPEC
  rpm --version > /work/artifacts/rpm-tool-version.txt
  rpmbuild -bb --define '_topdir /work/rpmbuild' --define '_buildhost convt-builder' \
    --define 'use_source_date_epoch_as_buildtime 1' --define 'clamp_mtime_to_source_date_epoch 1' \
    --define '_binary_payload w9.xzdio' --define '_build_id_links none' \
    --define '__os_install_post %{nil}' /work/rpmbuild/SPECS/convt.spec
  cp /work/rpmbuild/RPMS/x86_64/*.rpm /work/artifacts/
fi
