#!/usr/bin/env python3
"""Stage integration without inspecting or writing any user's home."""
import importlib.util
import json
import pathlib
import shutil
import subprocess
import sys

repo = pathlib.Path(__file__).resolve().parents[2]
root = pathlib.Path(sys.argv[1])
cli = sys.argv[2]
spec = importlib.util.spec_from_file_location("menus", repo / "integrations/linux/install.py")
menus = importlib.util.module_from_spec(spec)
spec.loader.exec_module(menus)
menus.APP = "/usr/bin/convt-app"
formats = json.loads(subprocess.check_output([cli, "formats", "--json"]))
# Quick convert chooses targets at runtime, including newly installed document
# engines. Static menus must not freeze the build machine's capabilities.
groups = [(formats, ())]

def write(relative, text, mode=0o644):
    path = root / relative
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text)
    path.chmod(mode)

for name, text in menus.dolphin_menus(groups).items():
    write("usr/share/kio/servicemenus/" + name, text, 0o755)
for name, text in menus.nemo_actions(groups).items():
    write("usr/share/nemo/actions/" + name, text)
write("usr/share/applications/convt-app.desktop", menus.app_entry())
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
