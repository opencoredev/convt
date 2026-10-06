#!/bin/sh
set -e
# Inform the installing user. Never inspect or modify any user's home.
cat <<'MESSAGE'
convt: If you previously installed per-user file-manager menus, run this as your normal user:
  python3 /usr/share/convt/integrations/install.py --uninstall
Then restart your file manager. Thunar users can install its per-user actions with:
  python3 /usr/share/convt/integrations/install.py --thunar
Documents are opt-in: use convt pack, or explicitly install your distro's LibreOffice.
MESSAGE
