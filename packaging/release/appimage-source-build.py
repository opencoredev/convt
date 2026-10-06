#!/usr/bin/env python3
"""Build the runtime with locked APKs and sources, without container networking."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import uuid


ROOT = Path(__file__).resolve().parents[2]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cache", type=Path, default=ROOT / "packaging/.cache")
    parser.add_argument("--lock", type=Path, default=Path(__file__).with_name("appimage-source-closure.lock.json"))
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    cache = args.cache.resolve()
    output = (args.output or cache / "appimage-source/rebuilt").resolve()
    lock = json.loads(args.lock.read_text())
    for source in lock["sources"]:
        path = cache / source["cache"]
        if not path.resolve().is_relative_to(cache):
            raise ValueError("Source escapes cache")
        if hashlib.sha256(path.read_bytes()).hexdigest() != source["sha256"]:
            raise ValueError(f"Source input hash mismatch: {source['name']}")
    expected_apks = {s["name"] for s in lock["sources"] if s.get("role") == "build-apk"}
    apks = cache / "appimage-source/apks"
    if {p.name for p in apks.glob("*.apk")} != expected_apks:
        raise ValueError("APK directory does not match the locked package set")
    recipe = Path(__file__).with_suffix(".sh")
    if hashlib.sha256(recipe.read_bytes()).hexdigest() != lock["build"]["script_sha256"]:
        raise ValueError("Build recipe hash mismatch")
    output.mkdir(parents=True, exist_ok=True)
    name = "convt-appimage-source-" + uuid.uuid4().hex[:12]
    command = ["docker", "run", "--rm", "--name", name, "--label",
               "convt.task=appimage-source-rebuild", "--network", "none",
               "--platform", "linux/amd64", "-e", f"BUILD_UID={os.getuid()}",
               "-e", f"BUILD_GID={os.getgid()}", "-e", "SOURCE_DATE_EPOCH=1790619238",
               "-v", f"{apks}:/apks:ro", "-v", f"{cache / 'appimage-source'}:/sources:ro",
               "-v", f"{output}:/output", "-v", f"{recipe}:/recipe.sh:ro",
               lock["build"]["image"], "sh", "/recipe.sh"]
    try:
        subprocess.run(command, check=True, timeout=900)
    finally:
        subprocess.run(["docker", "rm", "-f", name], capture_output=True, timeout=20)
    runtime = output / "runtime-x86_64"
    actual = hashlib.sha256(runtime.read_bytes()).hexdigest()
    expected = lock["build"].get("runtime_sha256")
    if expected and actual != expected:
        raise ValueError(f"Rebuilt runtime differs: {actual} != {expected}")
    print(json.dumps({"runtime": str(runtime), "sha256": actual}, indent=2))


if __name__ == "__main__":
    main()
