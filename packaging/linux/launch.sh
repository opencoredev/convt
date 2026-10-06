#!/bin/sh
set -eu
# Private origin-relative ELF RPATH supplies the native closure. Never pass
# an inherited loader search path to helpers or system Office children.
unset LD_LIBRARY_PATH
# Fixed layout, independent of the caller's working directory.
bundle_launcher=$(/usr/bin/readlink -f -- "$0")
bundle_dir=$(/usr/bin/dirname -- "$bundle_launcher")
# Convt loads plugins through its absolute bundle paths and checked override.
# Do not inject a bundle plugin path into system Office children.
exec "$bundle_dir/$(/usr/bin/basename -- "$bundle_launcher").bin" "$@"
