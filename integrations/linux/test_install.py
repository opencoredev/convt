"""Tests for install.py and the Nautilus extension.

    python3 -m unittest discover -s integrations/linux

The menu commands run for real: a fake convt-app in a folder whose name has
spaces and quotes records the arguments it receives. Desktop entries launch
through GIO (`gio launch`), Thunar commands through `sh -c` as Thunar does,
and Nemo commands are split with GLib's parser as Nemo does. Tests that need
GLib skip without it.
"""

import contextlib
import io
import json
import os
import re
import shlex
import shutil
import subprocess
import sys
import tempfile
import time
import types
import unittest
from pathlib import Path
from unittest import mock
from urllib.parse import unquote, urlparse

HERE = Path(__file__).parent
sys.path.insert(0, str(HERE))
import install  # noqa: E402

try:
    import gi

    gi.require_version("GLib", "2.0")
    from gi.repository import GLib
except (ImportError, ValueError):
    GLib = None

GROUPS = [
    ([{"mime": "image/png", "extensions": ["png"]}], ("jpeg", "webp")),
    ([{"mime": "image/jpeg", "extensions": ["jpg", "jpeg"]}], ("png",)),
]
NAMES = ["plain.png", "two words.png", "it's.png", 'say "hi".png', "$HOME `x` 100%.png", "-starts-with-dash.png"]

FAKE_APP = """#!/bin/sh
exec python3 -c 'import json, sys; json.dump(sys.argv[1:], open(sys.argv[1], "w"))' "$CONVT_TEST_OUT" "$@"
"""


class Fixture(unittest.TestCase):
    def setUp(self):
        self.tmp = Path(tempfile.mkdtemp(prefix="convt-linux-"))
        self.addCleanup(shutil.rmtree, self.tmp)
        bin_dir = self.tmp / "my \"bin\" it's $dir"
        bin_dir.mkdir()
        self.app = bin_dir / "convt-app"
        self.app.write_text(FAKE_APP)
        self.app.chmod(0o755)
        self.out = self.tmp / "argv.json"
        files = self.tmp / "files"
        files.mkdir()
        self.files = [files / n for n in NAMES]
        for f in self.files:
            f.write_bytes(b"x")
        patcher = mock.patch.object(install, "APP", str(self.app))
        patcher.start()
        self.addCleanup(patcher.stop)

    def received(self, timeout=10):
        """The arguments the fake app got, after the program name and the
        output path."""
        deadline = time.monotonic() + timeout
        while not self.out.exists():
            self.assertLess(time.monotonic(), deadline, "the fake app never ran")
            time.sleep(0.02)
        time.sleep(0.05)
        return json.loads(self.out.read_text())[1:]

    def expected(self, target=None):
        return ["open", *(["--to", target] if target is not None else []), "--", *map(str, self.files)]


class DesktopExec(unittest.TestCase):
    def test_plain_words_stay_unquoted(self):
        self.assertEqual(install.desktop_exec("/usr/bin/convt-app", "open"), "/usr/bin/convt-app open")

    def test_reserved_characters_are_quoted_and_escaped(self):
        # One string-level backslash escape on top of the spec's quoting.
        self.assertEqual(install.desktop_exec('/a b/"x"$'), '"/a b/\\\\"x\\\\"\\\\$"')
        self.assertEqual(install.desktop_exec("100%"), "100%%")


@unittest.skipUnless(GLib and shutil.which("gio"), "needs GLib and gio")
class DesktopEntries(Fixture):
    def launch(self, exec_line, args):
        entry = self.tmp / "t.desktop"
        entry.write_text(f"[Desktop Entry]\nType=Application\nName=t\n{exec_line}\n")
        env = {**os.environ, "CONVT_TEST_OUT": str(self.out)}
        subprocess.run(["gio", "launch", str(entry), *args], check=True, env=env, timeout=20)

    def test_dolphin_actions_pass_every_file_name_intact(self):
        menus = install.dolphin_menus(GROUPS)
        text = menus["convt-0.desktop"]
        self.assertIn("MimeType=image/png;", text)
        self.assertIn("Actions=convert_jpeg;convert_webp;options;", text)
        exec_line = re.search(r"\[Desktop Action convert_webp\]\n.*?\n.*?\n(Exec=.*)\n", text).group(1)
        self.launch(exec_line, map(str, self.files))
        self.assertEqual(self.received(), self.expected("webp"))

    def test_dolphin_more_options_preserves_selection_without_a_target(self):
        menus = install.dolphin_menus(GROUPS)
        for text in menus.values():
            self.assertIn(";options;", text)
            self.assertIn("Name=More options…", text)
        section = menus["convt-0.desktop"].split("[Desktop Action options]", 1)[1]
        exec_line = next(line for line in section.splitlines() if line.startswith("Exec="))
        self.launch(exec_line, map(str, self.files))
        self.assertEqual(self.received(), self.expected())

    def test_dolphin_lists_more_options_last(self):
        # KIO sorts a submenu's actions by their IDs, comparing UTF-16 code units.
        for text in install.dolphin_menus(GROUPS).values():
            ids = re.search(r"^Actions=(.*);$", text, re.M).group(1).split(";")
            self.assertEqual(sorted(ids)[-1], "options")

    def test_app_entry_takes_files_and_links(self):
        text = install.app_entry()
        exec_line = next(line for line in text.splitlines() if line.startswith("Exec="))
        self.launch(exec_line, map(str, self.files))
        # "Open With Other Application" passes local files as paths (GIO) or
        # file:// URIs (KDE). The app takes both.
        paths = [unquote(urlparse(a).path) if a.startswith("file://") else a for a in self.received()]
        self.assertEqual(paths, list(map(str, self.files)))

    def test_app_entry_claims_no_file_types(self):
        # Claiming a file type can make convt its double-click default on
        # GNOME and Cinnamon; see app_entry.
        lines = install.app_entry().splitlines()
        self.assertEqual([l for l in lines if l.startswith("MimeType=")], ["MimeType=x-scheme-handler/convt;"])
        # Still listed among all applications.
        self.assertNotIn("NoDisplay=true", lines)

    def test_install_sets_no_defaults(self):
        with mock.patch.object(install, "DATA", self.tmp / "data"), \
                mock.patch.object(install.subprocess, "run") as run, \
                contextlib.redirect_stdout(io.StringIO()):
            install.install_app()
        commands = [c.args[0] for c in run.call_args_list]
        self.assertFalse([c for c in commands if c[0] == "xdg-mime"], commands)
        self.assertTrue((self.tmp / "data/applications/convt-app.desktop").exists())
        self.assertFalse(list(self.tmp.rglob("mimeapps.list")))

    @unittest.skipUnless(shutil.which("gio") and shutil.which("update-desktop-database"), "needs gio")
    def test_gio_never_picks_convt_but_finds_it_for_links(self):
        data = self.tmp / "data"
        env = {
            "PATH": os.environ.get("PATH", "/usr/bin:/bin"),
            "HOME": str(self.tmp / "home"),
            "XDG_CONFIG_HOME": str(self.tmp / "config"),
            "XDG_DATA_HOME": str(data),
            "XDG_CACHE_HOME": str(self.tmp / "cache"),
            # No system applications: convt is the only candidate there is.
            "XDG_DATA_DIRS": str(self.tmp / "none"),
        }
        with mock.patch.object(install, "DATA", data), contextlib.redirect_stdout(io.StringIO()):
            install.install_app()

        def default(mime):
            out = subprocess.run(["gio", "mime", mime], env=env, capture_output=True, text=True).stdout
            return out.splitlines()[0] if out else ""

        self.assertIn("convt-app.desktop", default("x-scheme-handler/convt"))
        for mime in ["image/png", "text/html", "image/qoi", "application/pdf"]:
            self.assertNotIn("convt", default(mime), mime)

    @unittest.skipUnless(shutil.which("desktop-file-validate"), "needs desktop-file-validate")
    def test_app_entry_is_valid(self):
        entry = self.tmp / "convt-app.desktop"
        entry.write_text(install.app_entry())
        result = subprocess.run(["desktop-file-validate", str(entry)], capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)


class Thunar(Fixture):
    def test_commands_survive_thunar_and_sh(self):
        actions = install.thunar_actions(GROUPS)
        self.assertEqual(len(actions), 4)
        webp = next(a for a in actions if "<unique-id>convt-webp</unique-id>" in a)
        self.assertIn("<patterns>*.png</patterns>", webp)
        from xml.dom.minidom import parseString

        command = parseString(webp).getElementsByTagName("command")[0].firstChild.data
        # What Thunar does: quote each path into %F, turn %% into %, run sh -c.
        expanded = command.replace("%F", " ".join(shlex.quote(str(f)) for f in self.files)).replace("%%", "%")
        env = {**os.environ, "CONVT_TEST_OUT": str(self.out)}
        subprocess.run(["/bin/sh", "-c", expanded], check=True, env=env, timeout=20)
        self.assertEqual(self.received(), self.expected("webp"))

    def test_more_options_survives_thunar_and_sh(self):
        from xml.dom.minidom import parseString

        action = next(a for a in install.thunar_actions(GROUPS) if "<unique-id>convt-more-options</unique-id>" in a)
        self.assertIn("<name>More options…</name>", action)
        self.assertIn("<patterns>*.jpeg;*.jpg;*.png</patterns>", action)
        command = parseString(action).getElementsByTagName("command")[0].firstChild.data
        expanded = command.replace("%F", " ".join(shlex.quote(str(f)) for f in self.files)).replace("%%", "%")
        subprocess.run(["/bin/sh", "-c", expanded], check=True,
                       env={**os.environ, "CONVT_TEST_OUT": str(self.out)}, timeout=20)
        self.assertEqual(self.received(), self.expected())

    def test_reinstall_replaces_old_actions_and_keeps_others(self):
        with mock.patch.object(install, "HOME", self.tmp):
            uca = self.tmp / ".config/Thunar/uca.xml"
            uca.parent.mkdir(parents=True)
            mine = "<action><icon>term</icon><name>Mine</name><command>xterm</command></action>\n"
            uca.write_text(f'<?xml version="1.0" encoding="UTF-8"?>\n<actions>\n{mine}</actions>\n')
            with contextlib.redirect_stdout(io.StringIO()):
                install.install_thunar(GROUPS)
                install.install_thunar(GROUPS)
            text = uca.read_text()
        self.assertIn(mine, text)
        self.assertEqual(text.count("<unique-id>convt-webp</unique-id>"), 1)
        self.assertEqual(text.count("<unique-id>convt-more-options</unique-id>"), 1)


@unittest.skipUnless(GLib, "needs GLib")
class Nemo(Fixture):
    def test_commands_survive_nemo(self):
        actions = install.nemo_actions(GROUPS)
        text = actions["convt-webp.nemo_action"]
        self.assertIn("Extensions=png;", text)
        keys = GLib.KeyFile()
        keys.load_from_data(text, len(text.encode()), GLib.KeyFileFlags.NONE)
        exec_ = keys.get_string("Nemo Action", "Exec")
        # What Nemo does: backslash-escape shell characters in each path,
        # then split the line with g_shell_parse_argv.
        escaped = " ".join(re.sub(r"([^\w\-./,:@+=])", r"\\\1", str(f)) for f in self.files)
        ok, argv = GLib.shell_parse_argv(exec_.replace("%F", escaped))
        self.assertTrue(ok)
        self.assertEqual(argv, [str(self.app), *self.expected("webp")])

    def test_nemo_lists_more_options_last(self):
        # Nemo orders actions by file name, comparing bytes.
        names = sorted(install.nemo_actions(GROUPS), key=str.encode)
        self.assertEqual(names[-1], "convt-zz-more-options.nemo_action")

    def test_more_options_survives_nemo(self):
        text = install.nemo_actions(GROUPS)["convt-zz-more-options.nemo_action"]
        self.assertIn("Name=More options…", text)
        self.assertIn("Extensions=jpeg;jpg;png;", text)
        keys = GLib.KeyFile()
        keys.load_from_data(text, len(text.encode()), GLib.KeyFileFlags.NONE)
        escaped = " ".join(re.sub(r"([^\w\-./,:@+=])", r"\\\1", str(f)) for f in self.files)
        ok, argv = GLib.shell_parse_argv(keys.get_string("Nemo Action", "Exec").replace("%F", escaped))
        self.assertTrue(ok)
        self.assertEqual(argv, [str(self.app), *self.expected()])
        subprocess.run(argv, check=True, env={**os.environ, "CONVT_TEST_OUT": str(self.out)}, timeout=20)
        self.assertEqual(self.received(), self.expected())


class Nautilus(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        class Item:
            def __init__(self, **kw):
                self.kw, self.submenu, self.handlers = kw, None, []

            def set_submenu(self, menu):
                self.submenu = menu

            def connect(self, signal, handler, *args):
                self.handlers.append((signal, handler, args))

        class Menu:
            def __init__(self):
                self.items = []

            def append_item(self, item):
                self.items.append(item)

        repository = types.SimpleNamespace(
            GObject=types.SimpleNamespace(GObject=type("GObject", (), {})),
            Nautilus=types.SimpleNamespace(MenuProvider=type("MenuProvider", (), {}), MenuItem=Item, Menu=Menu),
        )
        stub = types.ModuleType("gi")
        stub.repository = repository
        with mock.patch.dict(sys.modules, {"gi": stub, "gi.repository": repository}):
            sys.path.insert(0, str(HERE / "nautilus"))
            try:
                import convt_nautilus
            finally:
                sys.path.remove(str(HERE / "nautilus"))
        cls.ext = convt_nautilus

    @staticmethod
    def file(path, directory=False):
        location = types.SimpleNamespace(get_path=lambda: path)
        return types.SimpleNamespace(get_location=lambda: location, is_directory=lambda: directory)

    def targets(self, ext):
        return {"png": ("jpeg", "webp", "gif"), "jpg": ("png", "webp", "gif")}.get(ext, ())

    def test_menu_offers_common_targets_and_opens_the_app(self):
        paths = ["/tmp/a b.png", "/tmp/it's \"q\".JPG"]
        with mock.patch.object(self.ext, "targets_for", self.targets):
            (root,) = self.ext.ConvtMenu().get_file_items([self.file(p) for p in paths])
        items = root.submenu.items
        self.assertEqual([i.kw["label"] for i in items], ["WEBP", "GIF", "More options…"])
        _, handler, args = items[0].handlers[0]
        with mock.patch.object(self.ext.subprocess, "Popen") as popen:
            handler(items[0], *args)
        self.assertEqual(popen.call_args.args[0], [self.ext.APP, "open", "--to", "webp", "--", *paths])

    def test_more_options_opens_quick_convert_without_a_target(self):
        paths = [f"/tmp/{name}" for name in NAMES]
        with mock.patch.object(self.ext, "targets_for", self.targets):
            (root,) = self.ext.ConvtMenu().get_file_items([self.file(p) for p in paths])
        more = root.submenu.items[-1]
        self.assertEqual(more.kw["label"], "More options…")
        _, handler, args = more.handlers[0]
        with mock.patch.object(self.ext.subprocess, "Popen") as popen:
            handler(more, *args)
        self.assertEqual(popen.call_args.args[0], [self.ext.APP, "open", "--", *paths])

    def test_supported_selection_without_common_targets_keeps_more_options(self):
        paths = ["/tmp/photo.png", "/tmp/sound.wav"]
        with mock.patch.object(self.ext, "targets_for", lambda ext: ("jpeg",) if ext == "png" else ("mp3",)):
            (root,) = self.ext.ConvtMenu().get_file_items([self.file(p) for p in paths])
        self.assertEqual([item.kw["label"] for item in root.submenu.items], ["More options…"])

    def test_no_menu_without_targets(self):
        with mock.patch.object(self.ext, "targets_for", self.targets):
            menu = self.ext.ConvtMenu()
            self.assertEqual(menu.get_file_items([self.file("/tmp/README")]), [])
            self.assertEqual(menu.get_file_items([self.file("/tmp/x.png", directory=True)]), [])
            self.assertEqual(menu.get_file_items([self.file("/tmp/a.png"), self.file("/tmp/b.txt")]), [])

    def test_extensions(self):
        self.assertEqual(self.ext.extension_of("/a.b/c"), "")
        self.assertEqual(self.ext.extension_of("/a/photo.HEIC"), "heic")
        self.assertEqual(self.ext.extension_of("/a/.hidden"), "")


if __name__ == "__main__":
    unittest.main()
