#!/usr/bin/env python3
"""Name, checksum and public URL of each platform's document pack.

Every release build downloads document support from its own GitHub release:
`convt-<version>-<platform>-documents.tar.gz`, with a `.sha256` beside it. The
release job builds the archive, publishes it here, and compiles the URL, digest
and sizes into convt (`CONVT_DOCUMENT_PACK_*`). The installers never carry it.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import sys
from pathlib import Path

PLATFORMS = ("linux-x86_64", "macos-arm64", "windows-x86_64")


def asset_name(version: str, platform: str) -> str:
    if not version or any(not part.isdigit() for part in version.split(".")):
        raise ValueError(f"expected stable semver, got {version!r}")
    if platform not in PLATFORMS:
        raise ValueError(f"unknown platform {platform!r}")
    return f"convt-{version}-{platform}-documents.tar.gz"


def checksum_name(version: str, platform: str) -> str:
    return asset_name(version, platform) + ".sha256"


def release_url(version: str, platform: str, repository: str | None = None) -> str:
    repo = repository or os.environ.get("GITHUB_REPOSITORY") or "opencoredev/convt"
    if "/" not in repo or repo.startswith("/") or repo.endswith("/"):
        raise ValueError(f"expected owner/name repository, got {repo!r}")
    return f"https://github.com/{repo}/releases/download/v{version}/{asset_name(version, platform)}"


def public_url(version: str, platform: str, repository: str | None = None) -> str:
    if override := os.environ.get("CONVT_DOCUMENT_PACK_URL"):
        return override
    return release_url(version, platform, repository)


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def publish(archive: Path, version: str, platform: str, destination: Path) -> dict[str, object]:
    if not archive.is_file() or archive.stat().st_size == 0:
        raise SystemExit(f"Document pack archive is missing: {archive}")
    destination.mkdir(parents=True, exist_ok=True)
    target = destination / asset_name(version, platform)
    if archive.resolve() != target.resolve():
        if target.exists():
            raise SystemExit(f"Output exists: {target}")
        archive.replace(target)
    digest = sha256_file(target)
    checksum = destination / checksum_name(version, platform)
    checksum.write_text(f"{digest}  {target.name}\n", encoding="utf-8")
    return {
        "url": release_url(version, platform),
        "name": target.name,
        "sha256": digest,
        "size": target.stat().st_size,
        "checksum": checksum.name,
    }


def github_env(receipt: dict[str, object], version: str, installed: int) -> str:
    """The `CONVT_DOCUMENT_PACK_*` lines a release job appends to $GITHUB_ENV."""
    return "".join(
        f"{key}={value}\n"
        for key, value in (
            ("CONVT_DOCUMENT_PACK_URL", receipt["url"]),
            ("CONVT_DOCUMENT_PACK_SHA256", receipt["sha256"]),
            ("CONVT_DOCUMENT_PACK_SIZE", receipt["size"]),
            ("CONVT_DOCUMENT_PACK_INSTALLED_SIZE", installed),
            ("CONVT_DOCUMENT_PACK_VERSION", version),
        )
    )


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    url = sub.add_parser("url", help="Print the GitHub release URL for this version")
    url.add_argument("version")
    url.add_argument("--platform", required=True, choices=PLATFORMS)
    url.add_argument("--repository")
    pub = sub.add_parser("publish", help="Rename the archive and write its checksum")
    pub.add_argument("archive", type=Path)
    pub.add_argument("version")
    pub.add_argument("destination", type=Path)
    pub.add_argument("--platform", required=True, choices=PLATFORMS)
    pub.add_argument(
        "--github-env",
        type=Path,
        help="Also append the build's CONVT_DOCUMENT_PACK_* variables to this file",
    )
    pub.add_argument("--pack-version", help="What the pack holds, such as 'LibreOffice 25.8.7 macOS arm64'")
    pub.add_argument("--installed-size", type=int, help="Bytes the pack takes once installed")
    args = parser.parse_args()
    if args.command == "url":
        print(public_url(args.version, args.platform, args.repository))
        return 0
    receipt = publish(args.archive, args.version, args.platform, args.destination)
    if args.github_env:
        if not args.pack_version or not args.installed_size:
            parser.error("--github-env needs --pack-version and --installed-size")
        with args.github_env.open("a", encoding="utf-8") as env:
            env.write(github_env(receipt, args.pack_version, args.installed_size))
    print(json.dumps(receipt, indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())
