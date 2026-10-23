#!/usr/bin/env python3
"""The Windows document pack's name, checksum and public URL.

A Windows-only front for `scripts/release/pack_assets.py`, which names every
platform's pack the same way. build.ps1 calls this file; the MSI stays
runtime-only and the app downloads the published archive after Install.
"""
from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "scripts" / "release"))
import pack_assets  # noqa: E402

PLATFORM = "windows-x86_64"
sha256_file = pack_assets.sha256_file


def asset_name(version: str) -> str:
    return pack_assets.asset_name(version, PLATFORM)


def checksum_name(version: str) -> str:
    return pack_assets.checksum_name(version, PLATFORM)


def release_url(version: str, repository: str | None = None) -> str:
    return pack_assets.release_url(version, PLATFORM, repository)


def public_url(version: str, repository: str | None = None) -> str:
    return pack_assets.public_url(version, PLATFORM, repository)


def publish(archive: Path, version: str, destination: Path) -> dict[str, object]:
    return pack_assets.publish(archive, version, PLATFORM, destination)


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
