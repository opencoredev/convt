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

    def test_app_entry_lists_file_types_and_the_scheme(self):
        lines = install.app_entry(GROUPS).splitlines()
        mime = next(l for l in lines if l.startswith("MimeType="))
        self.assertIn("x-scheme-handler/convt", mime)
        self.assertIn("image/png", mime)
        self.assertIn("image/jpeg", mime)
        self.assertNotIn("NoDisplay=true", lines)

    def test_install_sets_no_defaults(self):
        with mock.patch.object(install, "DATA", self.tmp / "data"), \
                mock.patch.object(install.subprocess, "run") as run, \
                contextlib.redirect_stdout(io.StringIO()):
            install.install_app(GROUPS)
        commands = [c.args[0] for c in run.call_args_list]
        self.assertFalse([c for c in commands if c[0] == "xdg-mime"], commands)
        self.assertTrue((self.tmp / "data/applications/convt-app.desktop").exists())
        self.assertFalse(list(self.tmp.rglob("mimeapps.list")))
        desktop = (self.tmp / "data/applications/convt-app.desktop").read_text()
        self.assertIn("MimeType=", desktop)
        self.assertIn("image/png", desktop)

    @unittest.skipUnless(shutil.which("gio") and shutil.which("update-desktop-database"), "needs gio")
    def test_gio_registers_open_with_without_overwriting_a_default(self):
        data = self.tmp / "data"
        system = self.tmp / "system/applications"
        system.mkdir(parents=True)
        (system / "photos.desktop").write_text(
            "[Desktop Entry]\nType=Application\nName=Photos\nExec=true %F\nMimeType=image/png;\n"
        )
        config = self.tmp / "config"
        config.mkdir()
        (config / "mimeapps.list").write_text("[Default Applications]\nimage/png=photos.desktop\n")
        env = {
            "PATH": os.environ.get("PATH", "/usr/bin:/bin"),
            "HOME": str(self.tmp / "home"),
            "XDG_CONFIG_HOME": str(config),
            "XDG_DATA_HOME": str(data),
            "XDG_CACHE_HOME": str(self.tmp / "cache"),
            "XDG_DATA_DIRS": str(self.tmp / "system"),
        }
        with mock.patch.object(install, "DATA", data), contextlib.redirect_stdout(io.StringIO()):
            install.install_app(GROUPS)
        if shutil.which("update-desktop-database"):
            subprocess.run(["update-desktop-database", str(system)], check=False)

        def default(mime):
            out = subprocess.run(["gio", "mime", mime], env=env, capture_output=True, text=True).stdout
            return out.splitlines()[0] if out else ""

        self.assertIn("convt-app.desktop", default("x-scheme-handler/convt"))
        png = subprocess.run(["gio", "mime", "image/png"], env=env, capture_output=True, text=True).stdout
        self.assertIn("convt-app.desktop", png)
        self.assertNotIn("convt-app.desktop", default("image/png"))
        self.assertEqual(
            (config / "mimeapps.list").read_text(),
            "[Default Applications]\nimage/png=photos.desktop\n",
        )

    @unittest.skipUnless(shutil.which("desktop-file-validate"), "needs desktop-file-validate")
    def test_app_entry_is_valid(self):
        entry = self.tmp / "convt-app.desktop"
        entry.write_text(install.app_entry())
        result = subprocess.run(["desktop-file-validate", str(entry)], capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_app_entry_does_not_need_a_terminal(self):
        self.assertIn("\nTerminal=false\n", install.app_entry())

    @unittest.skipUnless(shutil.which("desktop-file-validate"), "needs desktop-file-validate")
    def test_packaged_desktop_is_valid_without_a_terminal(self):
        desktop = HERE.parent.parent / "packaging/linux/convt.desktop"
        text = desktop.read_text()
        self.assertIn("\nTerminal=false\n", text)
        result = subprocess.run(["desktop-file-validate", str(desktop)], capture_output=True, text=True)
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

    def setUp(self):
        patch = mock.patch.object(self.ext, "supported_extensions", return_value={"png", "jpg", "wav"})
        patch.start()
        self.recognition_patch = patch
        self.addCleanup(patch.stop)
        self.ext.cached_targets.cache_clear()
        self.ext.cached_extensions.cache_clear()

    @staticmethod
    def file(path, directory=False):
        location = types.SimpleNamespace(get_path=lambda: path)
        return types.SimpleNamespace(get_location=lambda: location, is_directory=lambda: directory)

    def targets(self, ext):
        return {"png": ("jpeg", "webp", "gif"), "jpg": ("png", "webp", "gif")}.get(ext, ())

    def test_as_argv_accepts_a_list_or_a_string(self):
        self.assertEqual(self.ext.as_argv("convt"), ["convt"])
        self.assertEqual(self.ext.as_argv(["/app.AppImage", "--cli"]), ["/app.AppImage", "--cli"])

    def test_cli_list_argv_runs_appimage_cli(self):
        self.ext.cached_targets.cache_clear()
        with mock.patch.object(self.ext, "CONVT", ["/app.AppImage", "--cli"]), \
                mock.patch.object(self.ext.subprocess, "run",
                                  return_value=types.SimpleNamespace(stdout="jpeg")) as run:
            self.assertEqual(self.ext.targets_for("png"), ("jpeg",))
        self.assertEqual(run.call_args.args[0][:3], ["/app.AppImage", "--cli", "targets"])

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

    def test_no_menu_for_unrecognized_inputs(self):
        with mock.patch.object(self.ext, "targets_for", self.targets):
            menu = self.ext.ConvtMenu()
            self.assertEqual(menu.get_file_items([self.file("/tmp/README")]), [])
            self.assertEqual(menu.get_file_items([self.file("/tmp/x.png", directory=True)]), [])
            self.assertEqual(menu.get_file_items([self.file("/tmp/a.png"), self.file("/tmp/b.txt")]), [])

    def test_extensions(self):
        self.assertEqual(self.ext.extension_of("/a.b/c"), "")
        self.assertEqual(self.ext.extension_of("/a/photo.HEIC"), "heic")
        self.assertEqual(self.ext.extension_of("/a/.hidden"), "")

    def test_recognized_format_without_targets_still_opens_more_options(self):
        with mock.patch.object(self.ext, 'targets_for', return_value=()):
            root, = self.ext.ConvtMenu().get_file_items([self.file('/tmp/x.wav')])
        self.assertEqual([item.kw['label'] for item in root.submenu.items], ['More options…'])

    def test_targets_retry_after_ttl_including_failed_queries(self):
        for first in [types.SimpleNamespace(stdout=''), subprocess.TimeoutExpired('convt', 2)]:
            self.ext.cached_targets.cache_clear()
            with mock.patch.object(self.ext.time, 'monotonic', return_value=0) as now, \
                    mock.patch.object(self.ext.subprocess, 'run', side_effect=[first, types.SimpleNamespace(stdout='jpeg webp')]) as run:
                self.assertEqual(self.ext.targets_for('png'), ())
                now.return_value = self.ext.CACHE_TTL - 1
                self.assertEqual(self.ext.targets_for('png'), ())
                self.assertEqual(run.call_count, 1)
                now.return_value = self.ext.CACHE_TTL
                self.assertEqual(self.ext.targets_for('png'), ('jpeg', 'webp'))
                self.assertEqual(run.call_count, 2)

    def test_recognition_cache_recovers_after_missing_cli(self):
        # Undo the menu-test recognition stub to exercise the public query.
        self.recognition_patch.stop()
        with mock.patch.object(self.ext.time, 'monotonic', return_value=0) as now, \
                mock.patch.object(self.ext.subprocess, 'run', side_effect=[FileNotFoundError(), types.SimpleNamespace(stdout='[{"extensions":["heic","heif"]}]')]) as run:
            self.assertEqual(self.ext.supported_extensions(), frozenset())
            self.assertEqual(self.ext.supported_extensions(), frozenset())
            self.assertEqual(run.call_count, 1)
            now.return_value = self.ext.CACHE_TTL
            self.assertEqual(self.ext.supported_extensions(), {'heic', 'heif'})


class NautilusScripts(Fixture):
    def test_scripts_pass_every_file_name_intact(self):
        scripts = install.nautilus_scripts(GROUPS)
        self.assertIn("Convert with convt/WEBP", scripts)
        self.assertIn("Convert with convt/More options…", scripts)
        script = self.tmp / "webp"
        script.write_text(scripts["Convert with convt/WEBP"])
        script.chmod(0o755)
        subprocess.run([str(script), *map(str, self.files)], check=True,
                       env={**os.environ, "CONVT_TEST_OUT": str(self.out)}, timeout=20)
        self.assertEqual(self.received(), self.expected("webp"))

    def test_more_options_script_opens_quick_convert(self):
        script = self.tmp / "more"
        script.write_text(install.nautilus_scripts(GROUPS)["Convert with convt/More options…"])
        script.chmod(0o755)
        subprocess.run([str(script), *map(str, self.files)], check=True,
                       env={**os.environ, "CONVT_TEST_OUT": str(self.out)}, timeout=20)
        self.assertEqual(self.received(), self.expected())

    def test_install_writes_executable_scripts(self):
        data = self.tmp / "data"
        with mock.patch.object(install, "DATA", data), contextlib.redirect_stdout(io.StringIO()):
            install.install_nautilus_scripts(GROUPS)
        folder = data / "nautilus/scripts" / install.NAUTILUS_SCRIPT_FOLDER
        webp = folder / "WEBP"
        self.assertTrue(webp.is_file())
        self.assertEqual(webp.stat().st_mode & 0o111, 0o111)
        self.assertTrue((webp.read_text()).startswith("#!/bin/sh\n" + install.MARKER))

    def test_install_scripts_does_not_follow_redirected_user_directories(self):
        outside = self.tmp / "outside"
        outside.mkdir()
        data = self.tmp / "data"
        (data / "nautilus").mkdir(parents=True)
        (data / "nautilus/scripts").symlink_to(outside, target_is_directory=True)
        with mock.patch.object(install, "DATA", data), contextlib.redirect_stdout(io.StringIO()):
            install.install_nautilus_scripts(GROUPS)
        self.assertTrue((data / "nautilus/scripts").is_symlink())
        self.assertEqual(list(outside.iterdir()), [])


class SystemIntegration(unittest.TestCase):
    def test_explicit_user_install_retires_duplicates_without_touching_other_files(self):
        with tempfile.TemporaryDirectory(prefix="convt-linux-system-") as tmp:
            root = Path(tmp)
            system, user = root / "system", root / "user"
            for directory, name in [("nemo/actions", "convt-zz-more-options.nemo_action"),
                                    ("kio/servicemenus", "convt-0.desktop"),
                                    ("nautilus-python/extensions", "convt_nautilus.py"),
                                    (f"nautilus/scripts/{install.NAUTILUS_SCRIPT_FOLDER}", "WEBP"),
                                    ("applications", "convt-app.desktop")]:
                for base in [system, user]:
                    (base / directory).mkdir(parents=True, exist_ok=True)
                    (base / directory / name).write_text(install.MARKER + "convt")
                (user / directory / "unrelated").write_text("keep")
            with mock.patch.object(install, "SYSTEM_DATA", system), mock.patch.object(install, "DATA", user):
                for name in ["nemo", "dolphin", "nautilus", "nautilus-scripts", "app"]:
                    self.assertTrue(install.use_system_install(name))
                self.assertFalse(install.use_system_install("thunar"))
            self.assertEqual(len(list(user.rglob("unrelated"))), 5)
            self.assertEqual(list(user.rglob("convt*")), [])
            self.assertFalse((user / f"nautilus/scripts/{install.NAUTILUS_SCRIPT_FOLDER}/WEBP").exists())
            self.assertEqual(len(list(p for p in system.rglob("*") if p.is_file())), 5)

    def test_tarball_install_still_uses_user_paths(self):
        with tempfile.TemporaryDirectory() as tmp, mock.patch.object(install, "SYSTEM_DATA", Path(tmp)):
            self.assertFalse(install.use_system_install("nemo"))

    def test_user_flag_writes_even_when_system_menus_exist(self):
        with tempfile.TemporaryDirectory(prefix="convt-linux-user-") as tmp:
            root = Path(tmp)
            system, data = root / "system", root / "user"
            (system / "kio/servicemenus").mkdir(parents=True)
            (system / "kio/servicemenus/convt-0.desktop").write_text(install.MARKER + "system")
            with mock.patch.object(install, "SYSTEM_DATA", system), \
                    mock.patch.object(install, "DATA", data), \
                    mock.patch.object(install, "CONVT", "/bin/true"), \
                    mock.patch.object(install, "APP", "/usr/bin/convt-app"), \
                    mock.patch.object(install, "groups", return_value=GROUPS), \
                    mock.patch.object(sys, "argv", ["install.py", "--user", "--dolphin"]), \
                    contextlib.redirect_stdout(io.StringIO()):
                install.main()
            written = list((data / "kio/servicemenus").glob("convt-*.desktop"))
            self.assertTrue(written)
            self.assertIn("X-KDE-Submenu=Convert with convt", written[0].read_text())

    def test_appimage_env_writes_the_stable_appimage_path(self):
        with tempfile.TemporaryDirectory(prefix="convt-appimage-") as tmp:
            image = Path(tmp) / "convt-linux-x86_64.AppImage"
            image.write_text("payload")
            mount = Path(tmp) / ".mount_convt123"
            mount.mkdir()
            with mock.patch.object(install, "APP", str(mount / "convt-app")), \
                    mock.patch.dict(os.environ, {"APPIMAGE": str(image)}):
                command = install.menu_command("webp")
                desktop = install.app_entry()
            self.assertEqual(command[0], str(image.resolve()))
            self.assertIn(str(image.resolve()), desktop)
            self.assertNotIn(".mount_convt123", command[0])
            self.assertNotIn(".mount_convt123", desktop)

    def test_legacy_recognition_uses_the_saved_path_inside_an_appimage(self):
        with tempfile.TemporaryDirectory(prefix="convt-legacy-appimage-") as tmp:
            image = Path(tmp) / "convt-linux-x86_64.AppImage"
            image.write_text("payload")
            old = "/opt/old/convt-app"
            with mock.patch.object(install, "APP", str(image.resolve())), \
                    mock.patch.dict(os.environ, {"APPIMAGE": str(image)}):
                self.assertEqual(install.app_path(), str(image.resolve()))
                with install.legacy_app(old):
                    self.assertEqual(install.app_path(), old)
                    self.assertEqual(install.menu_command("webp")[0], old)
                self.assertEqual(install.app_path(), str(image.resolve()))

    def test_nautilus_extension_bakes_resolved_binaries(self):
        source = (HERE / "nautilus/convt_nautilus.py").read_text()
        with mock.patch.object(install, "CONVT", "/opt/convt/convt"), \
                mock.patch.object(install, "APP", "/opt/convt/convt-app"), \
                mock.patch.dict(os.environ, {"APPIMAGE": ""}):
            baked = install.bake_nautilus_extension(source)
        self.assertIn("CONVT = ['/opt/convt/convt']", baked)
        self.assertIn("APP = '/opt/convt/convt-app'", baked)
        assignment = baked[baked.index("CONVT = "):baked.index("CACHE_TTL")]
        self.assertNotIn("shutil.which", assignment)

    def test_appimage_bakes_cli_flag_and_stable_file(self):
        with tempfile.TemporaryDirectory(prefix="convt-nautilus-bake-") as tmp:
            image = Path(tmp) / "convt-linux-x86_64.AppImage"
            image.write_text("payload")
            mount = Path(tmp) / ".mount_convt123"
            source = (HERE / "nautilus/convt_nautilus.py").read_text()
            with mock.patch.object(install, "CONVT", str(mount / "convt")), \
                    mock.patch.object(install, "APP", str(mount / "convt-app")), \
                    mock.patch.dict(os.environ, {"APPIMAGE": str(image)}):
                baked = install.bake_nautilus_extension(source)
                command = install.cli_command()
            resolved = str(image.resolve())
            self.assertEqual(command, [resolved, "--cli"])
            self.assertIn(f"CONVT = {command!r}", baked)
            self.assertIn(f"APP = {resolved!r}", baked)
            self.assertNotIn(".mount_convt123", baked)

    def test_install_nautilus_writes_the_baked_extension(self):
        with tempfile.TemporaryDirectory(prefix="convt-nautilus-install-") as tmp:
            data = Path(tmp) / "user"
            with mock.patch.object(install, "DATA", data), \
                    mock.patch.object(install, "CONVT", "/opt/convt/convt"), \
                    mock.patch.object(install, "APP", "/opt/convt/convt-app"), \
                    mock.patch.dict(os.environ, {"APPIMAGE": ""}), \
                    contextlib.redirect_stdout(io.StringIO()):
                install.install_nautilus()
            dest = data / "nautilus-python/extensions/convt_nautilus.py"
            text = dest.read_text()
            self.assertTrue(text.startswith(install.MARKER) or "CONVT = ['/opt/convt/convt']" in text)
            self.assertIn("CONVT = ['/opt/convt/convt']", text)
            self.assertIn("APP = '/opt/convt/convt-app'", text)


class Ownership(Fixture):
    def setUp(self):
        super().setUp()
        self.data = self.tmp / 'data'
        self.config = self.tmp / 'config'
        for patch in [mock.patch.object(install, 'DATA', self.data),
                      mock.patch.dict(os.environ, {'XDG_CONFIG_HOME': str(self.config)})]:
            patch.start()
            self.addCleanup(patch.stop)

    def seed(self, legacy=False):
        artifacts = [('dolphin', install.dolphin_menus(GROUPS)),
                     ('nemo', install.nemo_actions(GROUPS)),
                     ('app', {'convt-app.desktop': install.app_entry(GROUPS)}),
                     ('nautilus', {'convt_nautilus.py': (HERE / 'nautilus/convt_nautilus.py').read_text()})]
        if not legacy:
            artifacts.append(
                ('nautilus-scripts', {Path(name).name: text for name, text in install.nautilus_scripts(GROUPS).items()})
            )
        paths = []
        for kind, files in artifacts:
            directory = self.data / install.USER_ARTIFACTS[kind][0]
            directory.mkdir(parents=True, exist_ok=True)
            for name, text in files.items():
                if legacy and kind == 'nautilus':
                    text = (HERE / 'fixtures/legacy-nautilus.txt').read_text()
                path = directory / name
                path.write_text(text.removeprefix(install.MARKER) if legacy else text)
                paths.append(path)
        uca = install.thunar_path()
        uca.parent.mkdir(parents=True, exist_ok=True)
        actions = ''.join(install.thunar_actions(GROUPS))
        if legacy:
            actions = actions.replace(install.ACTION_MARKER, '')
        self.unrelated_action = '<action><icon>convt</icon><name>Mine</name><unique-id>convt-custom</unique-id><command>echo keep</command></action>'
        uca.write_text('<actions>' + self.unrelated_action + actions + '</actions>')
        return paths

    def test_uninstall_marked_files_is_idempotent_and_preserves_others(self):
        paths = self.seed()
        unrelated = self.data / 'nemo/actions/convt-personal.nemo_action'
        unrelated.write_text('[Nemo Action]\nName=Mine\nIcon-Name=convt\nExec=echo keep\n')
        original = unrelated.read_bytes()
        link = self.data / 'kio/servicemenus/convt-link.desktop'
        link.symlink_to(unrelated)
        with contextlib.redirect_stdout(io.StringIO()):
            install.uninstall()
            install.uninstall()
        self.assertFalse(any(path.exists() for path in paths))
        self.assertEqual(unrelated.read_bytes(), original)
        self.assertTrue(link.is_symlink())
        self.assertEqual(install.thunar_path().read_text().strip(), '<actions>' + self.unrelated_action + '\n' * 4 + '</actions>')

    def test_uninstall_recognizes_complete_legacy_templates_with_quoted_paths(self):
        paths = self.seed(legacy=True)
        for path in paths:
            kind = next(kind for kind, (directory, _) in install.USER_ARTIFACTS.items()
                        if path.parent == self.data / directory)
            self.assertTrue(install.owned_file(path, kind), str(path))
        with contextlib.redirect_stdout(io.StringIO()):
            install.uninstall()
        self.assertFalse(any(path.exists() for path in paths))
        self.assertIn(self.unrelated_action, install.thunar_path().read_text())
        self.assertNotIn('<unique-id>convt-webp</unique-id>', install.thunar_path().read_text())

    def test_similar_legacy_templates_and_binary_files_are_preserved(self):
        paths = self.seed(legacy=True)
        for path in paths:
            path.write_text(path.read_text().replace('Icon=convt', 'Icon=personal').replace('Icon-Name=convt', 'Icon-Name=personal') + '# user modification\n')
        binary = self.data / 'nemo/actions/convt-binary.nemo_action'
        binary.write_bytes(b'\xff\x00')
        with contextlib.redirect_stdout(io.StringIO()):
            install.uninstall()
        self.assertTrue(all(path.exists() for path in paths))
        self.assertEqual(binary.read_bytes(), b'\xff\x00')

    def test_install_does_not_overwrite_unrelated_colliding_file(self):
        directory = self.data / 'nemo/actions'
        directory.mkdir(parents=True)
        path = directory / 'convt-webp.nemo_action'
        path.write_text('my custom menu')
        with contextlib.redirect_stdout(io.StringIO()):
            install.install_nemo(GROUPS)
        self.assertEqual(path.read_text(), 'my custom menu')

    def test_uninstall_requires_no_binaries_and_does_not_read_system_files(self):
        self.seed()
        with mock.patch.object(sys, 'argv', ['install.py', '--uninstall']), \
                mock.patch.object(install, 'CONVT', None), mock.patch.object(install, 'APP', None), \
                mock.patch.object(install, 'groups', side_effect=AssertionError('must not query binaries')), \
                contextlib.redirect_stdout(io.StringIO()):
            install.main()
        self.assertFalse((self.data / 'applications/convt-app.desktop').exists())

    def test_uninstall_does_not_follow_redirected_user_directories(self):
        outside = self.tmp / 'outside'
        outside.mkdir()
        path = outside / 'convt-0.desktop'
        path.write_text(install.MARKER + 'owned elsewhere')
        (self.data / 'kio').mkdir(parents=True)
        (self.data / 'kio/servicemenus').symlink_to(outside, target_is_directory=True)
        with contextlib.redirect_stdout(io.StringIO()):
            install.uninstall()
        self.assertTrue(path.exists())

    def test_invalid_thunar_xml_is_left_untouched(self):
        path = install.thunar_path()
        path.parent.mkdir(parents=True)
        text = '<actions><action>' + install.ACTION_MARKER
        path.write_text(text)
        with contextlib.redirect_stdout(io.StringIO()):
            install.uninstall()
        self.assertEqual(path.read_text(), text)


class EmptyTargets(Fixture):
    def test_user_static_menus_keep_supported_formats_without_targets(self):
        formats = [{'mime': 'image/heic', 'extensions': ['heic', 'heif']}]
        with mock.patch.object(install, 'run', side_effect=[json.dumps(formats), '']):
            groups = install.groups()
        self.assertEqual(groups, [(formats, ())])
        text = install.dolphin_menus(groups)['convt-0.desktop']
        self.assertIn('Actions=options;', text)
        self.assertIn('MimeType=image/heic;', text)
        self.assertIn('Extensions=heic;heif;', install.nemo_actions(groups)['convt-zz-more-options.nemo_action'])
        self.assertIn('<patterns>*.heic;*.heif</patterns>', install.thunar_actions(groups)[0])

    def test_packaged_static_menus_keep_all_recognized_inputs(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            cli = root / 'convt'
            formats = [{'mime': 'image/heic', 'extensions': ['heic', 'heif']}]
            cli.write_text(
                '#!/usr/bin/python3\nimport sys\n'
                'print(' + repr(json.dumps(formats)) + ' if "formats" in sys.argv else "")\n'
            )
            cli.chmod(0o755)
            subprocess.run([sys.executable, str(HERE.parents[1] / 'packaging/linux/system-menus.py'), str(root / 'stage'), str(cli)], check=True)
            desktop = (root / 'stage/usr/share/kio/servicemenus/convt-0.desktop').read_text()
            self.assertIn('Actions=options;', desktop)
            self.assertIn('MimeType=image/heic;', desktop)
            self.assertIn('Extensions=heic;heif;', (root / 'stage/usr/share/nemo/actions/convt-zz-more-options.nemo_action').read_text())
            self.assertFalse((root / 'stage/usr/share/nautilus/scripts').exists())
            self.assertIn('image/heic', (root / 'stage/usr/share/applications/convt-app.desktop').read_text())

    def test_packaged_static_menus_include_full_targets(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            cli = root / 'convt'
            formats = [{'mime': 'image/png', 'extensions': ['png']}]
            cli.write_text(
                '#!/usr/bin/python3\nimport sys\n'
                + 'print(' + repr(json.dumps(formats)) + ' if "formats" in sys.argv else "jpeg webp")\n'
            )
            cli.chmod(0o755)
            subprocess.run([sys.executable, str(HERE.parents[1] / 'packaging/linux/system-menus.py'), str(root / 'stage'), str(cli)], check=True)
            desktop = (root / 'stage/usr/share/kio/servicemenus/convt-0.desktop').read_text()
            self.assertIn('Actions=convert_jpeg;convert_webp;options;', desktop)
            self.assertTrue((root / 'stage/usr/share/nemo/actions/convt-jpeg.nemo_action').is_file())
            self.assertFalse((root / 'stage/usr/share/nautilus/scripts').exists())




if __name__ == '__main__':
    unittest.main()
