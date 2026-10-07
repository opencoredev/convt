#!/usr/bin/env python3
"""Name, checksum and public URL for the Windows document pack.

Mac and Linux already download document support on demand after the user
clicks Install (`packs::install_documents`). They have no hosted archive yet,
so a released build only uses a system LibreOffice. Windows used to embed the
pack in the MSI. The MSI stays runtime-only; this helper publishes the same
pinned archive next to it so the existing Download UI can fetch and verify it.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import sys
from pathlib import Path


def asset_name(version: str) -> str:
    if not version or any(part for part in version.split(".") if not part.isdigit()):
        raise ValueError(f"expected stable semver, got {version!r}")
    return f"convt-{version}-windows-x86_64-documents.tar.gz"


def checksum_name(version: str) -> str:
    return asset_name(version) + ".sha256"


def public_url(version: str, repository: str | None = None) -> str:
    if override := os.environ.get("CONVT_DOCUMENT_PACK_URL"):
        return override
    repo = repository or os.environ.get("GITHUB_REPOSITORY") or "opencoredev/convt"
    if "/" not in repo or repo.startswith("/") or repo.endswith("/"):
        raise ValueError(f"expected owner/name repository, got {repo!r}")
    name = asset_name(version)
    return f"https://github.com/{repo}/releases/download/v{version}/{name}"


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def publish(archive: Path, version: str, destination: Path) -> dict[str, object]:
    if not archive.is_file() or archive.stat().st_size == 0:
        raise SystemExit(f"Document pack archive is missing: {archive}")
    destination.mkdir(parents=True, exist_ok=True)
    target = destination / asset_name(version)
    if archive.resolve() != target.resolve():
        if target.exists():
            raise SystemExit(f"Output exists: {target}")
        archive.replace(target)
    digest = sha256_file(target)
    checksum = destination / checksum_name(version)
    checksum.write_text(f"{digest}  {target.name}\n", encoding="utf-8")
    return {
        "url": public_url(version),
        "name": target.name,
        "sha256": digest,
        "size": target.stat().st_size,
        "checksum": checksum.name,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    url = sub.add_parser("url", help="Print the GitHub release URL for this version")
    url.add_argument("version")
    url.add_argument("--repository")
    pub = sub.add_parser("publish", help="Rename the archive and write its checksum")
    pub.add_argument("archive", type=Path)
    pub.add_argument("version")
    pub.add_argument("destination", type=Path)
    args = parser.parse_args()
    if args.command == "url":
        print(public_url(args.version, args.repository))
        return 0
    print(json.dumps(publish(args.archive, args.version, args.destination), indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())
