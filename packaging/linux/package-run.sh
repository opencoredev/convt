#!/bin/bash
# Runs only inside the offline installed-package verification container.
set -euo pipefail
stage=$1
export PATH=/validation:/opt/convt:/usr/bin:/bin
export CONVT_LICENSE_STORE=file CONVT_CONFIG_DIR=/tmp/convt-config CONVT_DATA_DIR=/tmp/convt-data
# Private libraries are inputs to the independent validator. The installed CLI
# is invoked through a clean environment so its own launcher/discovery is tested.
export LD_LIBRARY_PATH=/opt/convt/lib
export CONVT_LIBHEIF_DIR=/opt/convt/lib CONVT_LIBHEIF_PLUGIN_DIR=/opt/convt/lib/libheif/plugins
export LIBHEIF_PLUGIN_PATH=/opt/convt/lib/libheif/plugins CONVT_PDFIUM_DIR=/opt/convt/lib
export CONVT_FFMPEG=/opt/convt/ffmpeg CONVT_FFPROBE=/opt/convt/ffprobe
export CONVT_BIN=/validation/convt-clean-env CONVT_MATRIX_HELPER=/validation/matrix-fixtures.py
export CONVT_MATRIX_REPORT=/reports/$stage.json
if [[ $stage == with-office ]]; then export CONVT_SOFFICE=/validation/soffice-system; fi
cat /etc/os-release
/validation/convt-clean-env engines | tee /reports/$stage-engines.txt
/validation/convt-clean-env pack status | tee /reports/$stage-pack.txt
if [[ $stage == with-office ]]; then
  test -n "$(/validation/convt-clean-env targets sample.docx)"
else
  test -z "$(/validation/convt-clean-env targets sample.docx)"
fi
/validation/matrix --ignored --nocapture full_matrix
python3 - "$CONVT_MATRIX_REPORT" "$stage" <<'PY'
import json,sys
r=json.load(open(sys.argv[1])); expected=579 if sys.argv[2]=='with-office' else 349
assert len(r)==expected, (len(r),expected)
assert all(c['error'] is None for c in r), 'conversion failures'
print(f'{len(r)} passed, 0 failed')
PY
