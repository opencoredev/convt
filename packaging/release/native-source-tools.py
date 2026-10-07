#!/usr/bin/env python3
"""Fetch, verify and assemble the pinned native/runtime source closures."""

import argparse
import gzip
import hashlib
import io
import json
import pathlib
import shutil
import subprocess
import tarfile
import tempfile
import urllib.request
import uuid
import zipfile


ROOT = pathlib.Path(__file__).resolve().parents[2]


def digest(data):
    return hashlib.sha256(data).hexdigest()


def archive_member(data, member):
    stream = io.BytesIO(data)
    if data.startswith(b"PK"):
        with zipfile.ZipFile(stream) as archive:
            return archive.read(member)
    if data.startswith(b"\x28\xb5\x2f\xfd"):
        data = subprocess.run(["zstd", "-dc"], input=data, capture_output=True,
                              check=True, timeout=120).stdout
        stream = io.BytesIO(data)
    mode = "r|*"
    if data.startswith(b"\x1f\x8b"):
        # APKs contain concatenated gzip/tar streams (signature, control, data).
        stream = gzip.GzipFile(fileobj=stream)
        mode = "r|"
    with tarfile.open(fileobj=stream, mode=mode, ignore_zeros=True) as archive:
        for entry in archive:
            if entry.name == member:
                if not entry.isfile():
                    raise ValueError(f"Not a regular archive member: {member}")
                return archive.extractfile(entry).read()
    raise ValueError(f"Missing archive member: {member}")


def rpm_extract(cache, sources, image):
    extracted = cache / "native-source/rpm-extracted"
    packages = [s for s in sources if s.get("format") == "rpm"
                and not (extracted / pathlib.Path(s["cache"]).name).is_dir()]
    if not packages:
        return
    # Only this task's named/labeled container and cache are touched. No host rpm.
    name = "convt-native-source-" + uuid.uuid4().hex[:12]
    script = """set -eu
dnf -q install -y cpio >&2
shift
for file do
  dir="/cache/native-source/rpm-extracted/$(basename "$file")"
  mkdir -p "$dir"
  cd "$dir"
  rpm2cpio "$file" | cpio -idmu --quiet
done
"""
    args = ["docker", "run", "--rm", "--name", name, "--label",
            "convt.task=native-source-closure", "-v", f"{cache}:/cache", image,
            "bash", "-c", script, "native-source", "unused"]
    args += ["/cache/" + s["cache"] for s in packages]
    try:
        subprocess.run(args, check=True, timeout=180)
    finally:
        subprocess.run(["docker", "rm", "-f", name], capture_output=True, timeout=20)


def member_data(ref, sources, cache):
    source = sources.get(ref.get("source"))
    if "local" in ref:
        path = ROOT / ref["local"]
        if not path.resolve().is_relative_to(ROOT):
            raise ValueError("Local recipe escapes source tree")
        data = path.read_bytes()
    elif source.get("format") == "rpm":
        base = cache / "native-source/rpm-extracted" / pathlib.Path(source["cache"]).name
        path = base / ref["member"]
        if not path.resolve().is_relative_to(base.resolve()):
            raise ValueError("RPM member escapes extraction directory")
        data = path.read_bytes()
    else:
        data = (cache / source["cache"]).read_bytes()
        for member in ref.get("containers", []):
            data = archive_member(data, member)
        data = archive_member(data, ref["member"])
    if digest(data) != ref["sha256"]:
        raise ValueError(f"Member hash mismatch: {ref}")
    return data


def check_rpm_headers(lock, sources, cache, image):
    entries = lock.get("rpm_headers", [])
    if not entries:
        return
    name = "convt-native-headers-" + uuid.uuid4().hex[:12]
    args = ["docker", "run", "--rm", "--name", name, "--network", "none", "--label", "convt.task=native-source-closure",
            "-v", f"{cache}:/cache:ro", image, "rpm", "-qp", "--qf",
            "%{NAME}|%{VERSION}|%{RELEASE}|%{ARCH}|%{SOURCERPM}\\n"]
    args += ["/cache/" + sources[e["source"]]["cache"] for e in entries]
    try:
        actual = subprocess.run(args, check=True, capture_output=True, text=True,
                                timeout=60).stdout.splitlines()
    finally:
        subprocess.run(["docker", "rm", "-f", name], capture_output=True, timeout=20)
    expected = [e["header"] for e in entries]
    if actual != expected:
        raise ValueError(f"RPM header mismatch: {actual!r} != {expected!r}")


def stripped_digest(data, cache, image):
    with tempfile.TemporaryDirectory(prefix="binary-association-", dir=cache) as directory:
        path = pathlib.Path(directory) / "binary"
        path.write_bytes(data)
        name = "convt-native-strip-" + uuid.uuid4().hex[:12]
        try:
            subprocess.run(["docker", "run", "--rm", "--name", name, "--label",
                            "convt.task=native-source-closure", "--network", "none",
                            "-v", f"{directory}:/inputs", image, "strip",
                            "--strip-unneeded", "/inputs/binary"], check=True, timeout=60)
        finally:
            subprocess.run(["docker", "rm", "-f", name], capture_output=True, timeout=20)
        return digest(path.read_bytes())


def main(default_lock):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=["fetch", "verify", "collect"])
    parser.add_argument("--cache", type=pathlib.Path, default=ROOT / "packaging/.cache")
    parser.add_argument("--lock", type=pathlib.Path, default=default_lock)
    parser.add_argument("--payload", type=pathlib.Path)
    parser.add_argument("--output", type=pathlib.Path)
    parser.add_argument("--source-only", action="store_true",
                        help="Retain sources, recipes and notices; declare binary build prerequisites in the lock only")
    parser.add_argument("--runtime-file", type=pathlib.Path,
                        help="Verify the runtime used by the AppImage wrapper against the rebuilt-runtime pin")
    parser.add_argument("--rpm-image", default="almalinux:8")
    parser.add_argument("--require-complete", action="store_true")
    args = parser.parse_args()
    if args.command == "collect" and not args.output:
        parser.error("collect requires --output")
    cache = args.cache.resolve()
    lock = json.loads(args.lock.read_text())
    if args.runtime_file:
        expected = lock.get("build", {}).get("runtime_sha256")
        if not expected or digest(args.runtime_file.read_bytes()) != expected:
            raise ValueError(f"Wrapper runtime hash mismatch: {args.runtime_file}")
    sources = {s["name"]: s for s in lock["sources"]}
    if len(sources) != len(lock["sources"]):
        raise ValueError("Duplicate source name")
    for source in sources.values():
        path = cache / source["cache"]
        if not path.resolve().is_relative_to(cache):
            raise ValueError("Source escapes cache")
        if args.command == "fetch" and not path.exists():
            path.parent.mkdir(parents=True, exist_ok=True)
            temporary = path.with_suffix(path.suffix + ".download")
            with urllib.request.urlopen(source["url"], timeout=60) as response, temporary.open("wb") as out:
                shutil.copyfileobj(response, out)
            if digest(temporary.read_bytes()) != source["sha256"]:
                temporary.unlink()
                raise ValueError(f"Downloaded source hash mismatch: {source['name']}")
            temporary.replace(path)
        data = path.read_bytes()
        if digest(data) != source["sha256"]:
            raise ValueError(f"Source hash mismatch: {source['name']}")
        if source.get("recipe_sha512") and hashlib.sha512(data).hexdigest() != source["recipe_sha512"]:
            raise ValueError(f"Recipe checksum mismatch: {source['name']}")
        if source.get("apk"):
            metadata = archive_member(data, ".PKGINFO").decode()
            fields = dict(line.split(" = ", 1) for line in metadata.splitlines() if " = " in line)
            if any(fields.get(k) != v for k, v in source["apk"].items()):
                raise ValueError(f"APK header mismatch: {source['name']}")
    if args.command in ("fetch", "collect"):
        rpm_extract(cache, lock["sources"], args.rpm_image)
    check_rpm_headers(lock, sources, cache, args.rpm_image)
    for check in lock.get("binary_checks", []):
        path = cache / (check["cache"] if "cache" in check else sources[check["source"]]["cache"])
        # fetch populates locked inputs; the rebuilt runtime does not exist yet.
        if args.command == "fetch" and not path.exists():
            continue
        if check.get("sha256") and digest(path.read_bytes()) != check["sha256"]:
            raise ValueError("Built runtime hash mismatch")
        if check.get("static_elf"):
            dynamic = subprocess.run(["readelf", "-d", str(path)], check=True,
                                     capture_output=True, text=True, timeout=30).stdout
            program = subprocess.run(["readelf", "-l", str(path)], check=True,
                                     capture_output=True, text=True, timeout=30).stdout
            if "(NEEDED)" in dynamic or "INTERP" in program:
                raise ValueError("AppImage runtime is not statically linked")
        if check.get("version_output"):
            with tempfile.TemporaryDirectory(prefix="runtime-version-", dir=cache) as directory:
                executable = pathlib.Path(directory) / "runtime"
                shutil.copyfile(path, executable)
                executable.chmod(0o700)
                result = subprocess.run([str(executable), "--appimage-version"], check=True,
                                        capture_output=True, text=True, timeout=30)
                output = (result.stdout + result.stderr).strip()
                if output != check["version_output"]:
                    raise ValueError(f"Runtime version mismatch: {output}")
    count = 0
    associations = []
    for component in lock["components"]:
        if not component["recipes"] or not component["notices"]:
            raise ValueError(f"Missing recipes or notices: {component['name']}")
        for used in component["sources"]:
            if used not in sources:
                raise ValueError(f"Unknown source: {used}")
        for kind in ("recipes", "notices"):
            for index, ref in enumerate(component[kind]):
                data = member_data(ref, sources, cache)
                if args.output and args.command == "collect":
                    # Preserve complete recipe/patch paths; notice names remain unique.
                    retained = ref["retain_as"] if "retain_as" in ref else f"{index:03d}-{pathlib.PurePosixPath(ref['member']).name}"
                    relative = pathlib.PurePosixPath(retained)
                    if relative.is_absolute() or ".." in relative.parts:
                        raise ValueError("Unsafe retained member path")
                    target = args.output / component["name"] / kind / relative
                    target.parent.mkdir(parents=True, exist_ok=True)
                    target.write_bytes(data)
                count += 1
        for binary in component.get("binaries", []):
            allowed = set()
            if "original" in binary:
                data = member_data(binary["original"], sources, cache)
                transformed = stripped_digest(data, cache, lock["strip_image"])
                if transformed != binary["stripped_sha256"]:
                    raise ValueError(f"Builder strip association mismatch: {binary['payload_path']}")
                allowed = {digest(data), transformed}
            if args.payload:
                if "payload_path" in binary:
                    path = args.payload / binary["payload_path"]
                    actual = digest(path.read_bytes())
                    if actual not in allowed:
                        raise ValueError(f"Payload hash mismatch: {path}")
                    associations.append({"path": binary["payload_path"], "sha256": actual,
                                         "original_sha256": binary["original"]["sha256"],
                                         "normalization": "unmodified" if actual == binary["original"]["sha256"] else "builder strip --strip-unneeded"})
    if args.command == "collect" and args.output:
        for source in sources.values():
            target = args.output / "sources" / source["name"]
            binary_only = (source.get("role") in {"build-apk", "binary", "tool", "build-tool", "binary-only", "tool-only"}
                           or source["name"].endswith((".conda", ".apk", ".AppImage"))
                           or source.get("format") == "rpm" and not source["name"].endswith(".src.rpm"))
            if args.source_only and binary_only:
                # Remove only previously collected, explicitly locked prerequisites.
                if target.is_file():
                    target.unlink()
                continue
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(cache / source["cache"], target)
        shutil.copyfile(args.lock, args.output / args.lock.name)
    for entry in lock.get("build_evidence", []):
        path = cache / entry["cache"]
        # Provenance is written by the network-none rebuild after fetch.
        if args.command == "fetch" and not path.exists():
            continue
        if digest(path.read_bytes()) != entry["sha256"]:
            raise ValueError(f"Build evidence hash mismatch: {path}")
        if args.command == "collect" and args.output:
            target = args.output / "build-evidence" / path.name
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(path, target)
    print(json.dumps({"sources": len(sources), "components": len(lock["components"]),
                      "verified_members": count, "closure_complete": lock["closure_complete"],
                      "binary_associations": associations,
                      "blockers": lock.get("blockers", [])}, indent=2))
    if args.require_complete and not lock["closure_complete"]:
        raise SystemExit("Closure incomplete; see blockers above")


if __name__ == "__main__":
    main(pathlib.Path(__file__).with_name("native-sources.lock.json"))
