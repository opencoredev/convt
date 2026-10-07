#!/usr/bin/env python3
"""Copy only the Windows runtime payload into the MSI harvest directory.

The v0.2.0 MSI was 537 MiB because WiX harvested everything under
``packaging/out/windows/payload``, including the optional LibreOffice
``documents.tar.gz`` (446 MiB, already gzipped) and 2,316 codec source files.
This stager refuses those, plus PDBs, import libraries and headers, and
copies only the files the installed app needs next to ``convt.exe``.
"""
from __future__ import annotations

import argparse
import shutil
import sys
from pathlib import Path

REQUIRED = (
    "convt.exe",
    "convt-app.exe",
    "ffmpeg.exe",
    "ffprobe.exe",
    "pdfium.dll",
    "LICENSE.txt",
    "build-receipt.json",
)
DLL_FAMILIES = {
    "libheif": ("heif.dll", "libheif.dll"),
    "libde265": ("libde265.dll", "de265.dll", "libde265-0.dll"),
    "x265": ("libx265.dll", "x265.dll"),
    "aom": ("aom.dll", "libaom.dll"),
    "msvc-crt": ("msvcp140.dll", "vcruntime140.dll", "vcruntime140_1.dll"),
}
FORBIDDEN_NAMES = frozenset({"documents.tar.gz"})
FORBIDDEN_SUFFIXES = frozenset(
    {".pdb", ".lib", ".a", ".exp", ".ilk", ".obj", ".iobj", ".ipdb", ".h", ".hpp", ".c", ".cc", ".cpp"}
)
FORBIDDEN_DIRS = frozenset({"native-source", "build-convt"})
ROOT_SUFFIXES = frozenset({".exe", ".dll"})
ROOT_NAMES = frozenset({"license.txt", "build-receipt.json"})
LICENSE_SUFFIXES = frozenset({".txt", ".md", ".json", ".ps1", ".py", ".rs", ".patch"})
# build-native.ps1 copies LICENSE/COPYING/NOTICE/PATENTS/COPYRIGHT, including
# COPYING.LESSER and a LICENSE-1 disambiguation when two files share a name.
LICENSE_NAMES = frozenset({"license", "copying", "notice", "patents", "copyright"})


def rel(path: Path, root: Path) -> Path:
    return path.relative_to(root)


def is_document_pack_name(name: str) -> bool:
    lower = name.lower()
    return lower in FORBIDDEN_NAMES or lower.endswith("-documents.tar.gz")


def forbidden_reason(path: Path, root: Path) -> str | None:
    relative = rel(path, root)
    name = path.name
    if is_document_pack_name(name):
        return f"{relative}: optional document pack does not belong in the MSI"
    if path.suffix.lower() in FORBIDDEN_SUFFIXES:
        return f"{relative}: {path.suffix} is a build artifact, not a runtime file"
    if any(part.lower() in FORBIDDEN_DIRS for part in relative.parts):
        return f"{relative}: corresponding source is not a runtime file"
    return None


def keep(path: Path, root: Path) -> bool:
    relative = rel(path, root)
    if forbidden_reason(path, root):
        return False
    if len(relative.parts) == 1:
        return path.suffix.lower() in ROOT_SUFFIXES or path.name.lower() in ROOT_NAMES
    if relative.parts[0] != "licenses":
        return False
    return is_license_notice(path.name)


def is_license_notice(name: str) -> bool:
    lower = name.lower()
    if lower in LICENSE_NAMES or Path(name).suffix.lower() in LICENSE_SUFFIXES:
        return True
    stem = Path(lower).stem
    if "-" in stem and stem.rsplit("-", 1)[-1].isdigit():
        stem = stem.rsplit("-", 1)[0]
    return stem in LICENSE_NAMES or stem.split(".")[0] in LICENSE_NAMES


def payload_files(root: Path) -> list[Path]:
    return sorted(path for path in root.rglob("*") if path.is_file())


def family_present(names: set[str], options: tuple[str, ...]) -> bool:
    lower = {name.lower() for name in names}
    return any(option.lower() in lower for option in options)


def validate(root: Path) -> list[str]:
    files = payload_files(root)
    errors = [reason for path in files if (reason := forbidden_reason(path, root))]
    names = {path.name for path in files if path.parent == root}
    for required in REQUIRED:
        if required not in names:
            errors.append(f"missing required runtime file: {required}")
    root_dlls = {path.name for path in files if path.parent == root and path.suffix.lower() == ".dll"}
    for family, options in DLL_FAMILIES.items():
        if not family_present(root_dlls, options):
            errors.append(f"missing {family} runtime DLL (expected one of {', '.join(options)})")
    extras = []
    for path in files:
        if forbidden_reason(path, root) or keep(path, root):
            continue
        extras.append(f"{rel(path, root)}: not a runtime file")
    errors.extend(extras)
    return errors


def write_table(files: list[tuple[Path, int]], dest: Path | None) -> str:
    rows = ["size_bytes\tpath"]
    rows.extend(f"{size}\t{path.as_posix()}" for path, size in files)
    text = "\n".join(rows) + "\n"
    if dest is not None:
        dest.write_text(text, encoding="utf-8")
    return text


def stage(source: Path, destination: Path) -> list[tuple[Path, int]]:
    if destination.exists():
        raise SystemExit(f"Output exists: {destination}")
    errors = validate(source)
    if errors:
        raise SystemExit("Windows MSI payload is not runtime-only:\n- " + "\n- ".join(errors))
    selected = [path for path in payload_files(source) if keep(path, source)]
    staged: list[tuple[Path, int]] = []
    for path in selected:
        target = destination / rel(path, source)
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(path, target)
        staged.append((rel(path, source), path.stat().st_size))
    staged.sort(key=lambda item: (-item[1], item[0].as_posix()))
    return staged


def format_bytes(size: int) -> str:
    if size >= 1024 * 1024:
        return f"{size / (1024 * 1024):.1f} MiB"
    if size >= 1024:
        return f"{size / 1024:.1f} KiB"
    return f"{size} B"


def print_report(staged: list[tuple[Path, int]]) -> None:
    total = sum(size for _, size in staged)
    print(f"{len(staged)} runtime files, {format_bytes(total)} uncompressed")
    print(f"{'size':>12}  path")
    for path, size in staged:
        print(f"{size:12d}  {path.as_posix()}")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source", type=Path, help="Verified payload directory")
    parser.add_argument("destination", type=Path, help="Fresh MSI harvest directory")
    parser.add_argument(
        "--table",
        type=Path,
        help="Write a size-sorted contents table (default: destination parent / msi-contents.txt)",
    )
    args = parser.parse_args()
    source = args.source.resolve()
    destination = args.destination.resolve()
    if not source.is_dir():
        raise SystemExit(f"Payload is missing: {source}")
    staged = stage(source, destination)
    table = args.table or destination.parent / "msi-contents.txt"
    write_table(staged, table)
    print_report(staged)
    return 0


if __name__ == "__main__":
    sys.exit(main())
