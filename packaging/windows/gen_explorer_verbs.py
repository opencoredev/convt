#!/usr/bin/env python3
"""Write the WiX fragment that registers classic Explorer cascade verbs.

The table is derived from crates/convt-core/src/formats.rs and the same
preferred-target policy as menu_preferred(). The MSI uses HKMU so a per-user
install writes HKCU and a per-machine install would write HKLM.
"""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[1]
FORMATS_RS = REPO / "crates/convt-core/src/formats.rs"
OUT = HERE / "explorer-verbs.wxs"

VERB_ID = "ConvertWithConvt"
VERB_LABEL = "Convert with Convt"
PREFERRED = {
    "gif": ("mp4", "webp", "png"),
    "image": ("jpeg", "png", "webp"),
    "vector": ("png", "jpeg", "pdf"),
    "video": ("mp4", "mov", "gif", "mp3"),
    "audio": ("mp3", "m4a", "wav"),
    "pdf": ("png", "jpeg", "docx"),
    "document": ("pdf", "docx", "txt"),
    "spreadsheet": ("pdf", "xlsx", "csv"),
    "presentation": ("pdf", "pptx"),
}


def parse_formats(text: str) -> list[tuple[str, str, list[str]]]:
    body = text.split("formats! {", 1)[1].split("}", 1)[0]
    rows = []
    for match in re.finditer(
        r'"([^"]+)",\s*"[^"]+",\s*(\w+),\s*\[([^\]]+)\],\s*"[^"]+";',
        body,
    ):
        fmt_id, category, ext_blob = match.group(1), match.group(2).lower(), match.group(3)
        extensions = re.findall(r'"([^"]+)"', ext_blob)
        if not extensions:
            raise ValueError(f"no extensions for {fmt_id}")
        rows.append((fmt_id, category, extensions))
    if not rows:
        raise ValueError("formats.rs produced no formats")
    return rows


def preferred(fmt_id: str, category: str) -> tuple[str, ...]:
    if fmt_id == "gif":
        return PREFERRED["gif"]
    try:
        return PREFERRED[category]
    except KeyError as exc:
        raise KeyError(f"no preferred targets for {fmt_id} ({category})") from exc


def associations(
    formats: list[tuple[str, str, list[str]]],
) -> list[tuple[str, str, list[str]]]:
    names = {fmt_id for fmt_id, _, _ in formats}
    out = []
    for fmt_id, category, extensions in formats:
        targets = [target for target in preferred(fmt_id, category) if target != fmt_id]
        missing = [target for target in targets if target not in names]
        if missing:
            raise ValueError(f"{fmt_id} prefers unknown targets {missing}")
        for extension in extensions:
            out.append((extension, fmt_id, targets))
    return out


def verb_key(extension: str) -> str:
    return rf"Software\Classes\SystemFileAssociations\.{extension}\shell\{VERB_ID}"


def xml_escape(value: str) -> str:
    return value.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;").replace('"', "&quot;")


def command(target: str | None) -> str:
    args = "open --to {0} -- &quot;%1&quot;".format(target) if target else "open -- &quot;%1&quot;"
    return f"&quot;[INSTALLFOLDER]convt-app.exe&quot; {args}"


def component(extension: str, targets: list[str]) -> str:
    key = verb_key(extension)
    ident = re.sub(r"[^A-Za-z0-9_]", "_", extension)
    lines = [
        f'      <Component Id="ExplorerVerb_{ident}" Guid="*">',
        f'        <RegistryKey Root="HKMU" Key="{xml_escape(key)}" ForceDeleteOnUninstall="yes">',
        f'          <RegistryValue Type="string" Name="MUIVerb" Value="{xml_escape(VERB_LABEL)}" KeyPath="yes" />',
        '          <RegistryValue Type="string" Name="Icon" Value="[INSTALLFOLDER]convt-app.exe,0" />',
        '          <RegistryValue Type="string" Name="MultiSelectModel" Value="Player" />',
        "        </RegistryKey>",
    ]
    for index, target in enumerate(targets, start=1):
        sub = f"{index * 10:02d}_{target}"
        subkey = rf"{key}\shell\{sub}"
        lines += [
            f'        <RegistryKey Root="HKMU" Key="{xml_escape(subkey)}" ForceDeleteOnUninstall="yes">',
            f'          <RegistryValue Type="string" Name="MUIVerb" Value="{xml_escape(target.upper())}" />',
            '          <RegistryValue Type="string" Name="MultiSelectModel" Value="Document" />',
            "        </RegistryKey>",
            f'        <RegistryKey Root="HKMU" Key="{xml_escape(subkey)}\\command" ForceDeleteOnUninstall="yes">',
            f'          <RegistryValue Type="string" Value="{command(target)}" />',
            "        </RegistryKey>",
        ]
    more = rf"{key}\shell\90_more"
    lines += [
        f'        <RegistryKey Root="HKMU" Key="{xml_escape(more)}" ForceDeleteOnUninstall="yes">',
        '          <RegistryValue Type="string" Name="MUIVerb" Value="More options…" />',
        '          <RegistryValue Type="string" Name="MultiSelectModel" Value="Document" />',
        "        </RegistryKey>",
        f'        <RegistryKey Root="HKMU" Key="{xml_escape(more)}\\command" ForceDeleteOnUninstall="yes">',
        f'          <RegistryValue Type="string" Value="{command(None)}" />',
        "        </RegistryKey>",
        "      </Component>",
    ]
    return "\n".join(lines)


def render(rows: list[tuple[str, str, list[str]]]) -> str:
    body = "\n".join(component(extension, targets) for extension, _, targets in rows)
    return f"""<?xml version="1.0" encoding="utf-8"?>
<Wix xmlns="http://wixtoolset.org/schemas/v4/wxs">
  <Fragment>
    <StandardDirectory Id="SendToFolder">
      <Component Id="SendToShortcut" Guid="*">
        <Shortcut Id="ConvtSendTo" Name="Convt" Target="[INSTALLFOLDER]convt-app.exe" Arguments="open --" WorkingDirectory="INSTALLFOLDER" />
        <RegistryValue Root="HKMU" Key="Software\\Convt" Name="SendTo" Type="string" Value="1" KeyPath="yes" />
      </Component>
    </StandardDirectory>
  </Fragment>
  <Fragment>
    <ComponentGroup Id="ExplorerVerbs">
      <ComponentRef Id="SendToShortcut" />
{body}
    </ComponentGroup>
  </Fragment>
</Wix>
"""


def generate() -> str:
    return render(associations(parse_formats(FORMATS_RS.read_text())))


def normalize(text: str) -> str:
    return text.replace("\r\n", "\n")


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true", help="fail if explorer-verbs.wxs is stale")
    args = parser.parse_args(argv)
    text = generate()
    if args.check:
        current = OUT.read_text() if OUT.exists() else ""
        if normalize(current) != normalize(text):
            print(f"{OUT} is stale; run packaging/windows/gen_explorer_verbs.py", file=sys.stderr)
            return 1
        print(f"{OUT}: up to date")
        return 0
    OUT.write_text(text, newline="\n")
    print(f"wrote {OUT}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
