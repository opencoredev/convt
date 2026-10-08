#!/usr/bin/env python3
"""Stage integration without inspecting or writing any user's home."""
import importlib.util
import pathlib
import shutil
import sys

repo = pathlib.Path(__file__).resolve().parents[2]
root = pathlib.Path(sys.argv[1])
cli = sys.argv[2]
spec = importlib.util.spec_from_file_location("menus", repo / "integrations/linux/install.py")
menus = importlib.util.module_from_spec(spec)
spec.loader.exec_module(menus)
menus.CONVT = cli
menus.APP = "/usr/bin/convt-app"
# Full target lists from the bundled CLI. Document formats stay at
# "More options…" when this build has no LibreOffice; the Nautilus extension
# and scripts still open Quick convert for those files.
groups = menus.groups()

def write(relative, text, mode=0o644):
    path = root / relative
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text)
    path.chmod(mode)

for name, text in menus.dolphin_menus(groups).items():
    write("usr/share/kio/servicemenus/" + name, text, 0o755)
for name, text in menus.nemo_actions(groups).items():
    write("usr/share/nemo/actions/" + name, text)
# Nautilus scripts only appear when they live in the user's data directory.
# The package ships the Python extension; Settings or install.py --nautilus
# writes the script fallback that works without python3-nautilus.
write("usr/share/applications/convt-app.desktop", menus.app_entry(groups))
for source, dest in [
    ("integrations/linux/nautilus/convt_nautilus.py", "usr/share/nautilus-python/extensions/convt_nautilus.py"),
    ("packaging/linux/convt.svg", "usr/share/icons/hicolor/scalable/apps/convt.svg"),
    ("packaging/linux/convt.metainfo.xml", "usr/share/metainfo/app.convt.convt.metainfo.xml"),
    ("integrations/linux/install.py", "usr/share/convt/integrations/install.py"),
    ("integrations/linux/nautilus/convt_nautilus.py", "usr/share/convt/integrations/nautilus/convt_nautilus.py"),
]:
    path = root / dest
    path.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(repo / source, path)
