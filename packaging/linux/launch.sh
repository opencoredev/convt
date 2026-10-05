#!/bin/sh
set -eu
# Fixed layout, independent of the caller's working directory.
bundle_launcher=$(readlink -f -- "$0")
bundle_dir=$(dirname -- "$bundle_launcher")
export LD_LIBRARY_PATH="$bundle_dir/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
# The library's implicit loader receives only the bundle directory. Convt's
# explicit plugin override is checked separately and must be absolute.
export LIBHEIF_PLUGIN_PATH="$bundle_dir/lib/libheif/plugins"
exec "$bundle_dir/$(basename -- "$bundle_launcher").bin" "$@"
