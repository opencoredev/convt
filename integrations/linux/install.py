#!/usr/bin/env python3
"""Installs "Convert with convt" menus for Linux file managers.

The menus open the convt app (`convt-app open --to <format> <files>`), which
shows progress, lets the user cancel and reports errors. Dolphin, Nemo and
Thunar use static menu files, so this script generates them from
`convt formats --json` and `convt targets`, then writes them to the user's
config. Rerun it after updating convt. Nautilus uses the dynamic extension in
nautilus/ instead.

It also installs convt-app.desktop, which handles convt:// links and lists
convt among all applications, so "Open With Other Application" can reach it.
It claims no file types and sets no defaults, so a double-click never opens
convt; the menus above are the way in.

    python3 integrations/linux/install.py [--dolphin] [--nemo] [--thunar] [--nautilus]

With no flags it installs for every file manager it finds. Both `convt` and
`convt-app` must be on PATH for installation.

Remove generated user integrations, including recognized legacy files:
    python3 /usr/share/convt/integrations/install.py --uninstall

Cleanup needs no binaries or file managers. It preserves unrelated files and
system integrations; restart file managers after cleanup.
"""

import hashlib
import json
import os
import re
import shlex
import shutil
import subprocess
import sys
from collections import defaultdict
from pathlib import Path
from xml.etree import ElementTree
from contextlib import contextmanager
from xml.sax.saxutils import escape as xml_escape

HOME = Path.home()
DATA = Path(os.environ.get("XDG_DATA_HOME", HOME / ".local/share"))
CONVT = shutil.which("convt")
APP = shutil.which("convt-app")
SYSTEM_DATA = Path("/usr/share")

MARKER = "# convt-generated: linux-integration-v1\n"
ACTION_MARKER = "<!-- convt-generated: linux-integration-v1 -->"
LEGACY_NAUTILUS_SHA256 = "dc9731676f911afbf77e959f85fe6af026cf9321d6300accb64699766dfdea7e"

# Characters that force quoting in a desktop entry Exec argument.
DESKTOP_RESERVED = set(" \t\n\"'\\><~|&;$*?#()`")


def run(*args):
    return subprocess.run([CONVT, *args], capture_output=True, text=True, check=True).stdout


def desktop_exec(*args):
    """An Exec value for a desktop entry, quoted as the Desktop Entry spec
    requires. Field codes such as %F must be passed separately with
    `desktop_exec(...) + " %F"`."""
    words = []
    for arg in args:
        arg = arg.replace("%", "%%")
        if DESKTOP_RESERVED & set(arg) or not arg:
            arg = '"' + re.sub(r'(["`$\\])', r"\\\1", arg) + '"'
        words.append(arg)
    # The string escape rule applies on top of the quoting rule.
    return " ".join(words).replace("\\", "\\\\")


def menu_command(target=None):
    """The app invocation for one target, before the file list."""
    return [APP, "open", *(["--to", target] if target is not None else []), "--"]


def groups():
    """Groups source formats that share the same target list, so each group
    becomes one menu with one MIME list."""
    formats = json.loads(run("formats", "--json"))
    by_targets = defaultdict(list)
    for f in formats:
        targets = tuple(run("targets", "--menu", f"x.{f['extensions'][0]}").split())
        by_targets[targets].append(f)
    return [(sources, targets) for targets, sources in by_targets.items()]


def extensions_by_target(groups):
    out = defaultdict(set)
    for sources, targets in groups:
        for t in targets:
            out[t].update(e for s in sources for e in s["extensions"])
    return out


def dolphin_menus(groups):
    """Service menu files by name. KIO sorts a submenu's actions by their
    IDs, not their labels, so "options" sorts after every "convert_" ID."""
    menus = {}
    for i, (sources, targets) in enumerate(groups):
        mimes = ";".join(sorted({s["mime"] for s in sources})) + ";"
        actions = ";".join([*(f"convert_{t}" for t in targets), "options"]) + ";"
        lines = [
            "[Desktop Entry]",
            "Type=Service",
            f"MimeType={mimes}",
            f"Actions={actions}",
            "X-KDE-Submenu=Convert with convt",
            "X-KDE-Priority=TopLevel",
            "",
        ]
        for t in targets:
            exec_ = desktop_exec(*menu_command(t)) + " %F"
            lines += [f"[Desktop Action convert_{t}]", f"Name={t.upper()}", "Icon=convt", f"Exec={exec_}", ""]
        lines += ["[Desktop Action options]", "Name=More options…", "Icon=convt",
                  f"Exec={desktop_exec(*menu_command())} %F", ""]
        menus[f"convt-{i}.desktop"] = MARKER + "\n".join(lines)
    return menus


def install_dolphin(groups):
    out = DATA / "kio/servicemenus"
    out.mkdir(parents=True, exist_ok=True)
    for old in out.glob("convt-*.desktop"):
        remove_owned(old, "dolphin")
    menus = dolphin_menus(groups)
    for name, text in menus.items():
        path = out / name
        if not write_owned(path, text, "dolphin"):
            continue
        path.chmod(0o755)  # Plasma 6 only runs executable service menus
    print(f"dolphin: {len(menus)} menus in {out}")


def nemo_actions(groups):
    """Nemo action files by name. Nemo shell-quotes each path it puts in %F
    and splits Exec like a shell, after reading it as a key file string.
    Nemo 6.0 has no action submenus and lists actions in file name order,
    so "More options…" gets a name that sorts after every target."""
    actions = {}
    for t, exts in sorted(extensions_by_target(groups).items()):
        exec_ = (shlex.join(menu_command(t)) + " %F").replace("\\", "\\\\")
        actions[f"convt-{t}.nemo_action"] = MARKER + "\n".join([
            "[Nemo Action]",
            f"Name=Convert to {t.upper()}",
            "Comment=Convert with convt",
            f"Exec={exec_}",
            "Icon-Name=convt",
            "Selection=notnone",
            f"Extensions={';'.join(sorted(exts))};",
            "",
        ])
    exts = {e for sources, _ in groups for source in sources for e in source["extensions"]}
    if exts:
        exec_ = (shlex.join(menu_command()) + " %F").replace("\\", "\\\\")
        actions["convt-zz-more-options.nemo_action"] = MARKER + "\n".join([
            "[Nemo Action]", "Name=More options…", "Comment=Open Quick convert",
            f"Exec={exec_}", "Icon-Name=convt", "Selection=notnone",
            f"Extensions={';'.join(sorted(exts))};", "",
        ])
    return actions


def install_nemo(groups):
    out = DATA / "nemo/actions"
    out.mkdir(parents=True, exist_ok=True)
    for old in out.glob("convt-*.nemo_action"):
        remove_owned(old, "nemo")
    actions = nemo_actions(groups)
    for name, text in actions.items():
        write_owned(out / name, text, "nemo")
    print(f"nemo: {len(actions)} actions in {out}")


def thunar_actions(groups):
    """The <action> elements for uca.xml. Thunar shell-quotes each path it
    puts in %F, turns %% into %, and splits the command like a shell."""
    out = []
    for t, exts in sorted(extensions_by_target(groups).items()):
        command = shlex.join(menu_command(t)).replace("%", "%%") + " %F"
        patterns = ";".join("*." + e for e in sorted(exts))
        out.append(
            f"<action>{ACTION_MARKER}<icon>convt</icon><name>Convert to {xml_escape(t.upper())}</name>"
            f"<submenu>Convert with convt</submenu>\n"
            f"<unique-id>convt-{xml_escape(t)}</unique-id><command>{xml_escape(command)}</command>"
            f"<description>Convert with convt</description>\n"
            f"<patterns>{xml_escape(patterns)}</patterns>"
            f"<other-files/><image-files/><audio-files/><video-files/><text-files/></action>\n"
        )
    exts = {e for sources, _ in groups for source in sources for e in source["extensions"]}
    if exts:
        command = shlex.join(menu_command()).replace("%", "%%") + " %F"
        patterns = ";".join("*." + e for e in sorted(exts))
        out.append(
            f"<action>{ACTION_MARKER}<icon>convt</icon><name>More options…</name>"
            "<submenu>Convert with convt</submenu>\n"
            f"<unique-id>convt-more-options</unique-id><command>{xml_escape(command)}</command>"
            "<description>Open Quick convert</description>\n"
            f"<patterns>{xml_escape(patterns)}</patterns>"
            "<other-files/><image-files/><audio-files/><video-files/><text-files/></action>\n"
        )
    return out


def install_thunar(groups):
    uca = thunar_path()
    actions = thunar_actions(groups)
    if uca.is_symlink() or not user_path_safe(uca, uca.parents[1]):
        print(f"preserving redirected Thunar configuration: {uca}")
        return
    if not uca.exists():
        uca.parent.mkdir(parents=True, exist_ok=True)
        uca.write_text('<?xml version="1.0" encoding="UTF-8"?>\n<actions>\n</actions>\n')
    with uca.open(newline="") as source:
        text = source.read()
    # Drop actions from a previous run, then append the new ones.
    text = remove_thunar_actions(text)
    text = text.replace("</actions>", "".join(actions) + "</actions>")
    uca.write_text(text)
    print(f"thunar: {len(actions)} actions in {uca} (restart Thunar)")


def install_nautilus():
    out = DATA / "nautilus-python/extensions"
    out.mkdir(parents=True, exist_ok=True)
    source = Path(__file__).parent / "nautilus/convt_nautilus.py"
    write_owned(out / source.name, source.read_text(), "nautilus")
    print(f"nautilus: extension in {out} (needs nautilus-python; run `nautilus -q`)")


def app_entry():
    """convt-app.desktop: the convt:// handler, and an app entry that "Open
    With Other Application" lists. %U passes file:// URIs and convt:// links
    alike.

    It lists no file MIME types on purpose. GIO (GNOME, Cinnamon, Xfce)
    ignores InitialPreference and, when the system names no default for a
    type, picks the first entry that claims it, searching the user's own
    applications folder first. Claiming image/png or text/html from
    ~/.local/share/applications would make convt the double-click opener
    wherever the distribution has no default list for that type. KDE honors
    InitialPreference=0, but still picks convt for a type no other app
    claims. Without file types it can only become a default the user sets
    explicitly."""
    return MARKER + "\n".join([
        "[Desktop Entry]",
        "Type=Application",
        "Name=convt",
        "GenericName=File Converter",
        "Comment=Convert files on this computer",
        "Icon=convt",
        f"Exec={desktop_exec(APP)} %U",
        "Terminal=false",
        "Categories=Utility;",
        "MimeType=x-scheme-handler/convt;",
        "",
    ])


def install_app():
    out = DATA / "applications"
    out.mkdir(parents=True, exist_ok=True)
    path = out / "convt-app.desktop"
    write_owned(path, app_entry(), "app")
    # Refreshes mimeinfo.cache, which is how GIO, KDE and xdg-open find the
    # only convt:// handler. Nothing is written to mimeapps.list.
    if shutil.which("update-desktop-database"):
        subprocess.run(["update-desktop-database", str(out)], check=False)
    print(f"app: {path} (convt:// links, and Open With Other Application)")


def thunar_path():
    return Path(os.environ.get("XDG_CONFIG_HOME", HOME / ".config")) / "Thunar/uca.xml"


@contextmanager
def legacy_app(app):
    """Regenerate a legacy template with its original executable path."""
    global APP
    previous, APP = APP, app
    try:
        yield
    finally:
        APP = previous


def legacy_command(value, desktop=False, app_entry=False):
    if desktop:
        prefix, separator, tail = value.partition(" open ")
        if app_entry:
            prefix, separator, tail = value.rpartition(" %U")
        if not separator:
            raise ValueError("not a desktop command")
        prefix = prefix.replace("\\\\", "\\")
        if prefix.startswith('"') and prefix.endswith('"'):
            prefix = re.sub(r'\\(["`$\\])', r"\1", prefix[1:-1])
        argv = [prefix, *(shlex.split("open " + tail) if not app_entry else ["%U"])]
    else:
        argv = shlex.split(value.replace("\\\\", "\\"))
    if not argv or Path(argv[0]).name != "convt-app":
        raise ValueError("not a convt app command")
    if app_entry:
        if argv[1:] != ["%U"]:
            raise ValueError("not an app entry")
    elif argv[1:] != ["open", "--", "%F"]:
        if len(argv) != 6 or argv[1:3] != ["open", "--to"] or argv[4:] != ["--", "%F"]:
            raise ValueError("not a menu command")
        if not re.fullmatch(r"[a-z0-9]+", argv[3]):
            raise ValueError("not a format")
    return argv[0].replace("%%", "%") if desktop else argv[0]


def legacy_generated(text, kind, name):
    """Recognize complete old generator output, never a filename or icon alone."""
    if kind == "nautilus":
        return hashlib.sha256(text.encode()).hexdigest() == LEGACY_NAUTILUS_SHA256
    try:
        command = re.search(r"^Exec=(.*)$", text, re.M).group(1)
        app = legacy_command(command, desktop=kind in {"app", "dolphin"}, app_entry=kind == "app")
        with legacy_app(app):
            if kind == "app":
                expected = app_entry()
            elif kind == "dolphin":
                mimes = re.search(r"^MimeType=(.*);$", text, re.M).group(1).split(";")
                actions = re.search(r"^Actions=(.*);$", text, re.M).group(1).split(";")
                if actions[-1] != "options" or any(not re.fullmatch(r"convert_[a-z0-9]+", a) for a in actions[:-1]):
                    return False
                sources = [{"mime": mime} for mime in mimes]
                expected = dolphin_menus([(sources, tuple(a[8:] for a in actions[:-1]))])["convt-0.desktop"]
            elif kind == "nemo":
                exts = re.search(r"^Extensions=(.*);$", text, re.M).group(1).split(";")
                target = re.fullmatch(r"convt-([a-z0-9]+|zz-more-options)\.nemo_action", name).group(1)
                targets = () if target == "zz-more-options" else (target,)
                expected = nemo_actions([([{"extensions": exts}], targets)])[name]
            else:
                return False
        return text == expected.removeprefix(MARKER)
    except (AttributeError, KeyError, ValueError, IndexError):
        return False


def user_path_safe(path, root):
    """Do not traverse integration directories redirected outside the user tree."""
    return all(not parent.is_symlink() for parent in path.parents if parent != root and root in parent.parents)


def owned_file(path, kind):
    # Never follow a user symlink or delete a directory, even with an owned name.
    if not user_path_safe(path, DATA) or path.is_symlink() or not path.is_file():
        return False
    try:
        text = path.read_text()
    except (OSError, UnicodeError):
        return False
    return text.startswith(MARKER) or legacy_generated(text, kind, path.name)


def remove_owned(path, kind):
    if owned_file(path, kind):
        path.unlink()


def write_owned(path, text, kind):
    if not user_path_safe(path, DATA) or ((path.exists() or path.is_symlink()) and not owned_file(path, kind)):
        print(f"preserving unrelated file: {path}")
        return False
    path.write_text(text)
    return True


def owned_thunar_action(text):
    try:
        action = ElementTree.fromstring(text)
        if action.tag != "action":
            return False
        if text.startswith(f"<action>{ACTION_MARKER}"):
            return True
        identity = action.findtext("unique-id", "")
        if not re.fullmatch(r"convt-([a-z0-9]+|more-options)", identity):
            return False
        app = legacy_command(action.findtext("command", "").replace("%%", "%"))
        patterns = action.findtext("patterns", "").split(";")
        if any(not p.startswith("*.") for p in patterns):
            return False
        target = identity[6:]
        with legacy_app(app):
            generated = thunar_actions([([{"extensions": [p[2:] for p in patterns]}],
                                        () if target == "more-options" else (target,))])
        return any(text == item.replace(ACTION_MARKER, "").rstrip("\n") for item in generated)
    except (ElementTree.ParseError, ValueError):
        return False


def remove_thunar_actions(text):
    # Validate the document first. Preserve unrelated action bytes and whitespace.
    ElementTree.fromstring(text)
    return re.sub(r"<!--.*?-->|<action\b[^>]*>.*?</action>",
                  lambda match: "" if owned_thunar_action(match.group()) else match.group(),
                  text, flags=re.S)


USER_ARTIFACTS = {
    "dolphin": ("kio/servicemenus", "convt-*.desktop"),
    "nemo": ("nemo/actions", "convt-*.nemo_action"),
    "nautilus": ("nautilus-python/extensions", "convt_nautilus.py"),
    "app": ("applications", "convt-app.desktop"),
}


def uninstall():
    for kind, (directory, pattern) in USER_ARTIFACTS.items():
        for path in (DATA / directory).glob(pattern):
            remove_owned(path, kind)
    uca = thunar_path()
    if user_path_safe(uca, uca.parents[1]) and uca.is_file() and not uca.is_symlink():
        try:
            with uca.open(newline="") as source:
                text = source.read()
            cleaned = remove_thunar_actions(text)
        except (OSError, UnicodeError, ElementTree.ParseError):
            print(f"preserving unreadable or invalid Thunar configuration: {uca}")
        else:
            if cleaned != text:
                uca.write_text(cleaned)
    applications = DATA / "applications"
    if (not applications.is_symlink() and user_path_safe(applications, DATA)
            and applications.is_dir() and shutil.which("update-desktop-database")):
        subprocess.run(["update-desktop-database", str(applications)], check=False)
    print("Removed generated user integrations; restart your file managers.")


def use_system_install(name):
    """An explicit per-user install retires only this user's old convt files.

    Package scripts never call this and never inspect user homes. Thunar
    has no system custom actions, so it continues using its user installer.
    """
    paths = USER_ARTIFACTS
    if name not in paths:
        return False
    directory, pattern = paths[name]
    if not any((SYSTEM_DATA / directory).glob(pattern)):
        return False
    for old in (DATA / directory).glob(pattern):
        remove_owned(old, name)
    print(f"{name}: using system integration in {SYSTEM_DATA / directory}")
    return True


def main():
    if sys.argv[1:] == ["--uninstall"]:
        uninstall()
        return
    missing = [name for name, path in [("convt", CONVT), ("convt-app", APP)] if not path]
    if missing:
        sys.exit(
            f"{' and '.join(missing)} not on PATH. Install them first, e.g. "
            "`cargo install --path crates/convt-cli` and `cargo install --path crates/convt-app`."
        )
    flags = {a.lstrip("-") for a in sys.argv[1:]}
    known = {"dolphin", "nemo", "thunar", "nautilus"}
    if flags - known:
        sys.exit(f"unknown option --{sorted(flags - known)[0]}\n\n{__doc__.strip()}")
    wanted = flags or {name for name in known if shutil.which(name)}
    if not wanted:
        sys.exit("No supported file manager found. Pass --dolphin, --nemo, --thunar or --nautilus.")
    g = groups()
    for name in sorted(wanted):
        if use_system_install(name):
            continue
        if name == "nautilus":
            install_nautilus()
        else:
            globals()[f"install_{name}"](g)
    if not use_system_install("app"):
        install_app()


if __name__ == "__main__":
    main()
