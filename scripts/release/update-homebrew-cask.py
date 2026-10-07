#!/usr/bin/env python3
"""Bump Casks/convt.rb from a release manifest and optionally push the tap.

A failing push never has to stop a release: the Release workflow runs this
job with continue-on-error. Local use:

  python3 scripts/release/update-homebrew-cask.py --cask Casks/convt.rb --check
  python3 scripts/release/update-homebrew-cask.py --cask Casks/convt.rb \\
    --version 0.2.0 --sha256 <64-hex>
"""
from __future__ import annotations

import argparse
from importlib.util import module_from_spec, spec_from_file_location
import json
import os
from pathlib import Path
import sys
import traceback

spec = spec_from_file_location("homebrew_cask", Path(__file__).with_name("homebrew_cask.py"))
homebrew = module_from_spec(spec)
spec.loader.exec_module(homebrew)


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cask", type=Path, default=homebrew.DEFAULT_CASK)
    parser.add_argument("--version")
    parser.add_argument("--sha256")
    parser.add_argument("--manifest", type=Path, help="Local release-manifest.json")
    parser.add_argument("--from-manifest-url", help="HTTPS URL of release-manifest.json")
    parser.add_argument("--check", action="store_true", help="Validate the cask and exit")
    parser.add_argument("--commit", action="store_true")
    parser.add_argument("--push", action="store_true")
    parser.add_argument("--tap", default=homebrew.TAP_REPO)
    parser.add_argument("--skip-tap", action="store_true")
    return parser.parse_args(argv)


def version_and_sha(args: argparse.Namespace) -> tuple[str, str]:
    if args.manifest is not None:
        return homebrew.dmg_from_manifest(json.loads(args.manifest.read_text()))
    if args.from_manifest_url:
        return homebrew.dmg_from_manifest(homebrew.fetch_json(args.from_manifest_url))
    if args.version and args.sha256:
        return args.version, args.sha256
    raise ValueError("pass --version and --sha256, --manifest, or --from-manifest-url")


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv)
    cask = args.cask.resolve()
    try:
        text = cask.read_text()
        homebrew.validate_cask(text)
        if args.check:
            print(f"ok {cask}")
            return 0
        version, sha256 = version_and_sha(args)
        bumped = homebrew.bump_cask(text, version=version, sha256=sha256)
        changed = homebrew.write_cask(cask, bumped)
        print(f"{'updated' if changed else 'unchanged'} {cask} {version} {sha256}")
        errors: list[str] = []
        repo = homebrew.REPO if cask.is_relative_to(homebrew.REPO) else cask.parent.parent
        # Commit, push and tap are independent. A failed push must still be
        # retryable when the cask file already matches, and a failed commit
        # must not skip the tap.
        if args.commit:
            try:
                if homebrew.commit_if_changed(repo, cask, f"chore(homebrew): bump convt to {version}"):
                    print(f"committed {cask.relative_to(repo)}")
                else:
                    print("nothing to commit")
            except Exception as error:  # noqa: BLE001 — still try push and tap
                errors.append(f"commit this repository: {error}")
        if args.push:
            try:
                homebrew.push_head(repo)
                print("pushed this repository")
            except Exception as error:  # noqa: BLE001 — still try the tap
                errors.append(f"push this repository: {error}")
        token = os.environ.get("HOMEBREW_TAP_TOKEN", "").strip()
        if not args.skip_tap and token:
            try:
                homebrew.publish_tap(cask, tap=args.tap, token=token, version=version)
                print(f"published {args.tap}")
            except Exception as error:  # noqa: BLE001 — tap is optional
                errors.append(f"publish tap: {error}")
        elif not args.skip_tap:
            print("HOMEBREW_TAP_TOKEN unset; skipped opencoredev/homebrew-tap")
        if errors:
            print("\n".join(errors), file=sys.stderr)
            return 1
        return 0
    except Exception:
        traceback.print_exc()
        return 1


if __name__ == "__main__":
    sys.exit(main())
