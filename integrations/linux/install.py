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
`convt-app` must be on PATH.
"""

import json
import re
import shlex
import shutil
import subprocess
import sys
from collections import defaultdict
from pathlib import Path
from xml.sax.saxutils import escape as xml_escape

HOME = Path.home()
DATA = HOME / ".local/share"
CONVT = shutil.which("convt")
APP = shutil.which("convt-app")

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
        targets = tuple(run("targets", f"x.{f['extensions'][0]}").split())
        if targets:
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
        menus[f"convt-{i}.desktop"] = "\n".join(lines)
    return menus


def install_dolphin(groups):
    out = DATA / "kio/servicemenus"
    out.mkdir(parents=True, exist_ok=True)
    for old in out.glob("convt-*.desktop"):
        old.unlink()
    menus = dolphin_menus(groups)
    for name, text in menus.items():
        path = out / name
        path.write_text(text)
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
        actions[f"convt-{t}.nemo_action"] = "\n".join([
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
        actions["convt-zz-more-options.nemo_action"] = "\n".join([
            "[Nemo Action]", "Name=More options…", "Comment=Open Quick convert",
            f"Exec={exec_}", "Icon-Name=convt", "Selection=notnone",
            f"Extensions={';'.join(sorted(exts))};", "",
        ])
    return actions


def install_nemo(groups):
    out = DATA / "nemo/actions"
    out.mkdir(parents=True, exist_ok=True)
    for old in out.glob("convt-*.nemo_action"):
        old.unlink()
    actions = nemo_actions(groups)
    for name, text in actions.items():
        (out / name).write_text(text)
    print(f"nemo: {len(actions)} actions in {out}")


def thunar_actions(groups):
    """The <action> elements for uca.xml. Thunar shell-quotes each path it
    puts in %F, turns %% into %, and splits the command like a shell."""
    out = []
    for t, exts in sorted(extensions_by_target(groups).items()):
        command = shlex.join(menu_command(t)).replace("%", "%%") + " %F"
        patterns = ";".join("*." + e for e in sorted(exts))
        out.append(
            f"<action><icon>convt</icon><name>Convert to {xml_escape(t.upper())}</name>"
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
            "<action><icon>convt</icon><name>More options…</name>"
            "<submenu>Convert with convt</submenu>\n"
            f"<unique-id>convt-more-options</unique-id><command>{xml_escape(command)}</command>"
            "<description>Open Quick convert</description>\n"
            f"<patterns>{xml_escape(patterns)}</patterns>"
            "<other-files/><image-files/><audio-files/><video-files/><text-files/></action>\n"
        )
    return out


def install_thunar(groups):
    uca = HOME / ".config/Thunar/uca.xml"
    actions = thunar_actions(groups)
    if not uca.exists():
        uca.parent.mkdir(parents=True, exist_ok=True)
        uca.write_text('<?xml version="1.0" encoding="UTF-8"?>\n<actions>\n</actions>\n')
    text = uca.read_text()
    # Drop actions from a previous run, then append the new ones.
    text = re.sub(r"<action><icon>convt</icon>.*?</action>\n", "", text, flags=re.S)
    text = text.replace("</actions>", "".join(actions) + "</actions>")
    uca.write_text(text)
    print(f"thunar: {len(actions)} actions in {uca} (restart Thunar)")


def install_nautilus():
    out = DATA / "nautilus-python/extensions"
    out.mkdir(parents=True, exist_ok=True)
    shutil.copy(Path(__file__).parent / "nautilus/convt_nautilus.py", out)
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
    return "\n".join([
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
    path.write_text(app_entry())
    # Refreshes mimeinfo.cache, which is how GIO, KDE and xdg-open find the
    # only convt:// handler. Nothing is written to mimeapps.list.
    if shutil.which("update-desktop-database"):
        subprocess.run(["update-desktop-database", str(out)], check=False)
    print(f"app: {path} (convt:// links, and Open With Other Application)")


def main():
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
        if name == "nautilus":
            install_nautilus()
        else:
            globals()[f"install_{name}"](g)
    install_app()


if __name__ == "__main__":
    main()
