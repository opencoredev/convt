#!/bin/sh
set -eu
# Parent-side startup only. Never source, dump or pass service variables to jobs.
if [ "$(id -u)" != 0 ]; then
    printf '%s\n' 'worker parent must run as root for sandbox setup' >&2
    exit 1
fi
# The worker checks database configuration after its sandbox gate.
# Fixed template path prevents an environment value from redirecting mknod.
# Do not rely on special files surviving OCI COPY/export/import.
make_device() {
    name=$1
    minor=$2
    path=/srv/convt-template/dev/$name
    if [ -e "$path" ] || [ -L "$path" ]; then
        if [ -L "$path" ] || [ ! -c "$path" ]; then
            printf '%s\n' 'sandbox device is not a trusted character device' >&2
            exit 1
        fi
    else
        if ! mknod "$path" c 1 "$minor"; then
            printf '%s\n' 'sandbox requires CAP_MKNOD and device policy access; startup refused' >&2
            exit 1
        fi
    fi
    case "$name" in
        null) expected=1:3 ;;
        urandom) expected=1:9 ;;
        random) expected=1:8 ;;
        *) exit 1 ;;
    esac
    [ "$(stat -c '%t:%T' "$path")" = "$expected" ] || exit 1
    case "$name" in null) chmod 0666 "$path" ;; *) chmod 0444 "$path" ;; esac
}
chmod u+w /srv/convt-template/dev
make_device null 3
make_device urandom 9
make_device random 8
chmod a-w /srv/convt-template/dev
# Confirm the platform device policy permits use, not merely node creation.
printf '' > /srv/convt-template/dev/null
if [ "$(head -c 1 /srv/convt-template/dev/urandom | wc -c)" -ne 1 ]; then
    printf '%s\n' 'sandbox urandom is inaccessible; startup refused' >&2
    exit 1
fi
exec /opt/convt/convt-worker "$@"
