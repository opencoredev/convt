#!/usr/bin/env bash
# Network is limited to dependency installation; all conversion runs are offline.
set -euo pipefail
repo=$(cd "$(dirname "$0")/../.." && pwd)
cd "$repo"
out=$(realpath "${CONVT_BUNDLE_OUT:-$repo/packaging/out}")
report=${CONVT_PACKAGE_REPORT:-$out/verification/packages}
mkdir -p "$report"
report=$(realpath "$report")
work=$(mktemp -d /tmp/convt-pkg-test-XXXXXX)
images=()
snapshots=()
containers=()
cleanup() {
  for container in "${containers[@]}"; do docker rm -f "$container" >/dev/null 2>&1 || true; done
  for image in "${images[@]}"; do docker image rm "$image" >/dev/null 2>&1 || true; done
  for ((index=${#snapshots[@]}-1; index>=0; index--)); do docker image rm "${snapshots[$index]}" >/dev/null 2>&1 || true; done
  rm -rf "$work"
}
trap cleanup EXIT
cp -a "$out/validation-tools/." "$work/"
cp scripts/matrix-fixtures.py "$work/"
cat > "$work/python3" <<'PYTHON'
#!/bin/sh
export PYTHONHOME=/validation/python
export LD_LIBRARY_PATH=/validation/validator-lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}
exec /validation/python/bin/python3 "$@"
PYTHON
cat > "$work/convt-clean-env" <<'CLI'
#!/bin/sh
exec env -u CONVT_LIBHEIF_DIR -u CONVT_LIBHEIF_PLUGIN_DIR -u CONVT_PDFIUM_DIR -u CONVT_FFMPEG -u CONVT_FFPROBE -u LIBHEIF_PLUGIN_PATH -u LD_LIBRARY_PATH -u CONVT_SOFFICE /usr/bin/convt "$@"
CLI
cat > "$work/soffice-system" <<'OFFICE'
#!/bin/sh
exec env -u LD_LIBRARY_PATH -u PYTHONHOME -u PYTHONPATH /usr/bin/soffice "$@"
OFFICE
chmod +x "$work/python3" "$work/convt-clean-env" "$work/soffice-system"
read -ra distros <<< "${CONVT_PACKAGE_DISTROS:-ubuntu-22.04 ubuntu-24.04 debian-12 fedora-latest almalinux-8}"
for distro in "${distros[@]}"; do
  base=$(python3 -c 'import json,sys;print(json.load(open("packaging/linux/package-images.lock.json"))["tests"][sys.argv[1]])' "$distro")
  family=rpm
  [[ $distro == ubuntu* || $distro == debian* ]] && family=deb
  image="convt-pkg-test-$distro-$$"
  container="$image"
  images+=("$image")
  containers+=("$container")
  mkdir -p "$report/$distro" "$work/$distro"
  printf '%s\n' "$base" > "$report/$distro/base-image.txt"
  docker create --name "$container" -v "$out:/packages:ro" "$base" sleep infinity >/dev/null
  docker start "$container" >/dev/null
  if [[ $family == deb ]]; then
    docker exec "$container" bash -euo pipefail -c 'apt-get update && apt-get install -y --no-install-recommends /packages/verification/packages/older/convt_*_amd64.deb desktop-file-utils appstream lintian && dpkg-query -W convt && apt-get install -y /packages/convt_*_amd64.deb && dpkg-query -W convt && test ! -e /usr/bin/soffice && ! dpkg-query -W libreoffice-common 2>/dev/null' > "$report/$distro/install.log" 2>&1
    lint_status=0
    docker exec "$container" bash -euo pipefail -c 'lintian --allow-root /packages/convt_*_amd64.deb' > "$report/$distro/lint.log" 2>&1 || lint_status=$?
    printf '%s\n' "$lint_status" > "$report/$distro/lint-status.txt"
  else
    docker exec "$container" bash -euo pipefail -c 'dnf install -y --setopt=install_weak_deps=False /packages/verification/packages/older/convt-*.rpm desktop-file-utils $(if command -v dnf5 >/dev/null; then echo appstream; fi) && rpm -q convt && dnf install -y /packages/convt-*.rpm && rpm -q convt && test ! -e /usr/bin/soffice && ! rpm -q libreoffice-core' > "$report/$distro/install.log" 2>&1
    # AlmaLinux does not ship rpmlint in its base repositories. RPM lint runs
    # in Fedora, against the same portable package, without adding EPEL.
    if [[ $distro == fedora* ]]; then
      docker exec "$container" dnf install -y --setopt=install_weak_deps=False rpmlint >> "$report/$distro/install.log" 2>&1
      lint_status=0
      docker exec "$container" bash -euo pipefail -c 'rpmlint /packages/convt-*.rpm' > "$report/$distro/lint.log" 2>&1 || lint_status=$?
      printf '%s\n' "$lint_status" > "$report/$distro/lint-status.txt"
    fi
  fi
  if [[ -e $report/$distro/lint.log ]]; then
    python3 packaging/linux/lint-check.py "$family" "$report/$distro/lint.log" "$(cat "$report/$distro/lint-status.txt")" > "$report/$distro/lint-gate.log"
  fi
  docker exec "$container" bash -euo pipefail -c 'for f in convt convt-app convt.bin convt-app.bin ffmpeg ffprobe; do test "$(sha256sum "/packages/convt/$f" | cut -d" " -f1)" = "$(sha256sum "/opt/convt/$f" | cut -d" " -f1)"; done; if command -v dpkg-deb >/dev/null; then dpkg-deb --ctrl-tarfile /packages/convt_*_amd64.deb | tar tf -; else rpm -qp --scripts /packages/convt-*.rpm; rpm -qp --requires /packages/convt-*.rpm; fi' > "$report/$distro/payload-metadata.txt"
  docker exec --user 65534 "$container" /usr/bin/convt engines > "$report/$distro/nonroot-engines.txt"
  docker exec "$container" bash -euo pipefail -c 'desktop-file-validate /usr/share/applications/convt-app.desktop; if command -v appstreamcli >/dev/null; then appstreamcli validate --no-net /usr/share/metainfo/app.convt.convt.metainfo.xml; fi' > "$report/$distro/metadata.log" 2>&1
  if [[ $distro == almalinux* ]]; then
    docker cp "$container:/usr/share/metainfo/app.convt.convt.metainfo.xml" "$report/$distro/installed.metainfo.xml"
    appstreamcli validate --no-net "$report/$distro/installed.metainfo.xml" >> "$report/$distro/metadata.log" 2>&1
  fi
  docker exec "$container" bash -euo pipefail -c 'LD_LIBRARY_PATH=/opt/convt/lib ldd /opt/convt/convt-app.bin; test -f /usr/share/nautilus-python/extensions/convt_nautilus.py; test -x /usr/share/kio/servicemenus/convt-0.desktop; test -f /usr/share/nemo/actions/convt-zz-more-options.nemo_action; test -f /usr/share/convt/integrations/install.py' > "$report/$distro/gui-ldd-menus.log" 2>&1
  ! /usr/bin/grep -q 'not found' "$report/$distro/gui-ldd-menus.log"
  snapshots+=("$(docker commit --message "convt-pkg offline installer verification" "$container" "$image")")
  docker rm -f "$container" >/dev/null
  jobs=${CONVT_PACKAGE_MATRIX_JOBS:-1}
  run_matrix() {
    docker run --rm --name "$container" --network none -v "$work:/validation:ro" \
      -v "$repo/packaging/linux/package-run.sh:/run-matrix.sh:ro" -v "$report/$distro:/reports" \
      -e "CONVT_MATRIX_JOBS=$jobs" "$image" bash /run-matrix.sh "$1" > "$report/$distro/$1.log" 2>&1
    tail -5 "$report/$distro/$1.log"
  }
  if [[ ${CONVT_PACKAGE_LIFECYCLE_ONLY:-0} != 1 ]]; then
  run_matrix without-office
  docker run -d --name "$container" "$image" sleep infinity >/dev/null
  if [[ $family == deb ]]; then
    docker exec "$container" apt-get install -y --no-install-recommends libreoffice > "$report/$distro/office-install.log" 2>&1
  else
    docker exec "$container" dnf install -y --setopt=install_weak_deps=False libreoffice > "$report/$distro/office-install.log" 2>&1
  fi
  snapshots+=("$(docker commit --message "convt-pkg offline installer verification" "$container" "$image")")
  docker rm -f "$container" >/dev/null
  run_matrix with-office
  # Removal and lifecycle operations need no network: package files and package
  # manager metadata are already local. Nothing mounts a host home directory.
  docker run -d --name "$container" --network none -v "$out:/packages:ro" -v "$report/$distro:/reports" "$image" sleep infinity >/dev/null
  if [[ $family == deb ]]; then
    docker exec "$container" bash -euo pipefail -c 'apt-get remove -y "libreoffice*"; test ! -e /usr/bin/soffice' > "$report/$distro/office-remove.log" 2>&1
  else
    docker exec "$container" bash -euo pipefail -c 'dnf remove -y "libreoffice*"; test ! -e /usr/bin/soffice' > "$report/$distro/office-remove.log" 2>&1
  fi
  snapshots+=("$(docker commit --message "convt-pkg offline installer verification" "$container" "$image")")
  docker rm -f "$container" >/dev/null
  run_matrix removed-office
  fi
  docker run -d --name "$container" --network none -v "$out:/packages:ro" "$image" sleep infinity >/dev/null
  if [[ $family == deb ]]; then
    docker exec "$container" bash -euo pipefail -c 'apt-get install -y --no-install-recommends --reinstall /packages/convt_*_amd64.deb; dpkg-query -L convt > /tmp/convt-files; mkdir -p /root/.local/share/convt; echo keep > /root/.local/share/convt/sentinel; apt-get remove -y convt; while read -r f; do if [ -f "$f" ] || [ -L "$f" ]; then echo "Left behind after remove: $f"; exit 1; fi; done < /tmp/convt-files; test ! -d /opt/convt; test "$(cat /root/.local/share/convt/sentinel)" = keep; apt-get install -y --no-install-recommends /packages/convt_*_amd64.deb; apt-get purge -y convt; test "$(cat /root/.local/share/convt/sentinel)" = keep; while read -r f; do if [ -f "$f" ] || [ -L "$f" ]; then echo "Left behind: $f"; exit 1; fi; done < /tmp/convt-files; test ! -d /opt/convt' > "$report/$distro/lifecycle.log" 2>&1
  else
    docker exec "$container" bash -euo pipefail -c 'dnf --disablerepo="*" reinstall -y --setopt=install_weak_deps=False /packages/convt-*.rpm; rpm -ql convt > /tmp/convt-files; mkdir -p /root/.local/share/convt; echo keep > /root/.local/share/convt/sentinel; dnf remove -y convt; test "$(cat /root/.local/share/convt/sentinel)" = keep; while read -r f; do if [ -f "$f" ] || [ -L "$f" ]; then echo "Left behind: $f"; exit 1; fi; done < /tmp/convt-files; test ! -d /opt/convt' > "$report/$distro/lifecycle.log" 2>&1
  fi
  docker rm -f "$container" >/dev/null
  printf 'PASS %s\n' "$distro"
done
