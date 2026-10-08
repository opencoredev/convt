#!/bin/sh
set -e
# Inform the installing user. Never inspect or modify any user's home.
cat <<'MESSAGE'
convt: If you previously installed per-user file-manager menus, run this as your normal user:
  python3 /usr/share/convt/integrations/install.py --uninstall
Then restart your file manager. GNOME Files without python3-nautilus, and Thunar,
need a per-user install: open Settings and choose Set up right-click menu, or run
  python3 /usr/share/convt/integrations/install.py --user --nautilus --thunar
Documents are opt-in: use convt pack, or explicitly install your distro's LibreOffice.
MESSAGE
