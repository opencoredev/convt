# convt-generated: linux-integration-v1
"""Nautilus (GNOME Files) extension: adds "Convert with convt" to the context menu.

Install: copy to ~/.local/share/nautilus-python/extensions/ and restart Nautilus
(`nautilus -q`). Needs the `nautilus-python` package, and `convt` and
`convt-app` on PATH. The menu lists targets from `convt targets`; picking one
opens the app, which shows progress and errors.
"""

import json
import os
import shutil
import subprocess
import time
from functools import lru_cache

from gi.repository import GObject, Nautilus

CONVT = shutil.which("convt") or "convt"
APP = shutil.which("convt-app") or "convt-app"


CACHE_TTL = 30


@lru_cache(maxsize=256)
def cached_targets(extension, generation):
    if not extension:
        return ()
    try:
        out = subprocess.run(
            [CONVT, "targets", "--menu", f"file.{extension}"],
            capture_output=True, text=True, timeout=2, check=True,
        ).stdout
    except (OSError, subprocess.SubprocessError):
        return ()
    return tuple(out.split())


def targets_for(extension):
    return cached_targets(extension, int(time.monotonic() / CACHE_TTL))


@lru_cache(maxsize=2)
def cached_extensions(generation):
    try:
        result = subprocess.run([CONVT, "formats", "--json"], capture_output=True,
                                text=True, timeout=2, check=True)
        return frozenset(e for f in json.loads(result.stdout) for e in f["extensions"])
    except (OSError, subprocess.SubprocessError, ValueError, KeyError, TypeError):
        return frozenset()


def supported_extensions():
    return cached_extensions(int(time.monotonic() / CACHE_TTL))


def path_of(item):
    location = item.get_location()
    return location.get_path() if location else None


def extension_of(path):
    return os.path.splitext(path)[1][1:].lower()


def command(paths, target=None):
    return [APP, "open", *(["--to", target] if target is not None else []), "--", *paths]


class ConvtMenu(GObject.GObject, Nautilus.MenuProvider):
    def get_file_items(self, files):
        paths = [path_of(f) for f in files if not f.is_directory()]
        if not paths or None in paths:
            return []
        exts = [extension_of(p) for p in paths]
        # Offer only targets every selected file supports, in the first file's order.
        supported = supported_extensions()
        if not all(e in supported for e in exts):
            return []
        lists = [targets_for(e) for e in exts]
        common = set.intersection(*(set(targets) for targets in lists))
        targets = [t for t in lists[0] if t in common]

        root = Nautilus.MenuItem(name="Convt::root", label="Convert with convt")
        submenu = Nautilus.Menu()
        root.set_submenu(submenu)
        for target in targets:
            item = Nautilus.MenuItem(name=f"Convt::{target}", label=target.upper())
            item.connect("activate", self.convert, paths, target)
            submenu.append_item(item)
        more = Nautilus.MenuItem(name="Convt::more_options", label="More options…")
        more.connect("activate", self.convert, paths, None)
        submenu.append_item(more)
        return [root]

    def convert(self, _item, paths, target):
        try:
            subprocess.Popen(command(paths, target), start_new_session=True)
        except OSError as e:
            print(f"convt: could not start {APP}: {e}")
