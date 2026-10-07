#!/usr/bin/env python3
"""Read and bump the Homebrew cask. Casks/convt.rb is the source of truth."""
from __future__ import annotations

import json
import os
from pathlib import Path
import re
import shutil
import ssl
import subprocess
import tempfile
import time
import urllib.error
import urllib.request

REPO = Path(__file__).resolve().parents[2]
DEFAULT_CASK = REPO / "Casks" / "convt.rb"
TAP_REPO = "opencoredev/homebrew-tap"
VERSION_RE = re.compile(r'^(\s*version\s+")([^"]+)(")\s*$', re.M)
SHA_RE = re.compile(r'^(\s*sha256(?:\s+arm:)?\s+")([0-9a-f]{64})(")\s*$', re.M)
VERSION_VALUE = re.compile(r"^\d+\.\d+\.\d+$")
SHA_VALUE = re.compile(r"^[0-9a-f]{64}$")
REQUIRED = (
    'cask "convt" do',
    "livecheck do",
    "strategy :github_latest",
    "depends_on arch: :arm64",
    "depends_on macos:",
    'app "convt.app"',
    "zap trash:",
    "convt-macos-arm64.dmg",
)

TAP_README = """# opencoredev Homebrew tap

```bash
brew install --cask opencoredev/tap/convt
```

Apple silicon only. This tap is updated after each
[convt](https://github.com/opencoredev/convt) GitHub release.
"""


def validate_cask(text: str) -> None:
    missing = [item for item in REQUIRED if item not in text]
    if missing:
        raise ValueError("cask is missing: " + ", ".join(missing))
    if VERSION_RE.search(text) is None:
        raise ValueError("cask needs a version stanza")
    if SHA_RE.search(text) is None:
        raise ValueError("cask needs a sha256 stanza")


def bump_cask(text: str, *, version: str, sha256: str) -> str:
    if not VERSION_VALUE.fullmatch(version):
        raise ValueError(f"invalid version: {version}")
    if not SHA_VALUE.fullmatch(sha256):
        raise ValueError(f"invalid sha256: {sha256}")
    validate_cask(text)
    bumped, count = VERSION_RE.subn(rf"\g<1>{version}\g<3>", text, count=1)
    if count != 1:
        raise ValueError("expected exactly one version stanza")
    bumped, count = SHA_RE.subn(rf"\g<1>{sha256}\g<3>", bumped, count=1)
    if count != 1:
        raise ValueError("expected exactly one sha256 stanza")
    validate_cask(bumped)
    return bumped


def dmg_from_manifest(manifest: dict) -> tuple[str, str]:
    builds = manifest.get("builds") or []
    if not builds:
        raise ValueError("manifest has no builds")
    build = builds[-1]
    version = build.get("version")
    if not isinstance(version, str) or not VERSION_VALUE.fullmatch(version):
        raise ValueError("manifest build is missing a version")
    for artifact in build.get("artifacts") or []:
        if artifact.get("platform") == "macos-arm64" and artifact.get("kind") == "dmg":
            digest = artifact.get("sha256")
            if not isinstance(digest, str) or not SHA_VALUE.fullmatch(digest):
                raise ValueError("macos-arm64 dmg has a bad sha256")
            return version, digest
    raise ValueError("manifest has no macos-arm64 dmg")


def fetch_json(url: str, *, attempts: int = 8, delay: float = 5.0) -> dict:
    if not url.startswith("https://"):
        raise ValueError("manifest URL must be https")
    last: Exception | None = None
    context = ssl.create_default_context()
    for attempt in range(attempts):
        try:
            request = urllib.request.Request(url, headers={"Accept": "application/json"})
            with urllib.request.urlopen(request, context=context, timeout=30) as response:
                return json.loads(response.read().decode())
        except (urllib.error.URLError, TimeoutError, json.JSONDecodeError, OSError) as error:
            last = error
            if attempt + 1 < attempts:
                time.sleep(delay)
    raise ValueError(f"could not fetch {url}: {last}") from last


def write_cask(path: Path, text: str) -> bool:
    previous = path.read_text() if path.exists() else ""
    if previous == text:
        return False
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text)
    return True


def git_env(token: str | None = None) -> dict[str, str]:
    env = os.environ.copy()
    env["GIT_TERMINAL_PROMPT"] = "0"
    if token:
        env["GIT_CONFIG_COUNT"] = "1"
        env["GIT_CONFIG_KEY_0"] = "http.https://github.com/.extraheader"
        env["GIT_CONFIG_VALUE_0"] = f"AUTHORIZATION: bearer {token}"
    return env


def run_git(args: list[str], *, cwd: Path, env: dict[str, str] | None = None) -> str:
    merged = git_env()
    if env:
        merged.update(env)
    return subprocess.check_output(["git", *args], cwd=cwd, env=merged, text=True).strip()


def commit_if_changed(repo: Path, path: Path, message: str) -> bool:
    run_git(["add", "--", str(path.relative_to(repo))], cwd=repo)
    staged = run_git(["diff", "--cached", "--name-only", "--", str(path.relative_to(repo))], cwd=repo)
    if not staged:
        return False
    run_git(
        [
            "-c",
            "user.name=github-actions[bot]",
            "-c",
            "user.email=41898282+github-actions[bot]@users.noreply.github.com",
            "commit",
            "-m",
            message,
        ],
        cwd=repo,
    )
    return True


def push_head(repo: Path, remote: str = "origin") -> None:
    branch = run_git(["rev-parse", "--abbrev-ref", "HEAD"], cwd=repo)
    if branch in {"HEAD", ""}:
        raise ValueError("refusing to push a detached HEAD")
    subprocess.check_call(["git", "push", remote, f"HEAD:refs/heads/{branch}"], cwd=repo)


def publish_tap(cask: Path, *, tap: str, token: str, version: str) -> None:
    if "/" not in tap:
        raise ValueError(f"tap must be owner/repo: {tap}")
    work = Path(tempfile.mkdtemp(prefix="convt-homebrew-tap-"))
    env = git_env(token)
    try:
        try:
            subprocess.check_call(
                ["git", "clone", "--depth", "1", f"https://github.com/{tap}.git", str(work / "tap")],
                env=env,
            )
        except subprocess.CalledProcessError:
            raise ValueError(
                f"could not clone {tap}; create the public repo and set HOMEBREW_TAP_TOKEN"
            ) from None
        repo = work / "tap"
        dest = repo / "Casks" / "convt.rb"
        dest.parent.mkdir(parents=True, exist_ok=True)
        dest.write_text(cask.read_text())
        readme = repo / "README.md"
        if not readme.exists():
            readme.write_text(TAP_README)
        run_git(["add", "Casks/convt.rb", "README.md"], cwd=repo, env=env)
        if not run_git(["diff", "--cached", "--name-only"], cwd=repo, env=env):
            return
        run_git(
            [
                "-c",
                "user.name=github-actions[bot]",
                "-c",
                "user.email=41898282+github-actions[bot]@users.noreply.github.com",
                "commit",
                "-m",
                f"convt {version}",
            ],
            cwd=repo,
            env=env,
        )
        subprocess.check_call(["git", "push", "origin", "HEAD"], cwd=repo, env=env)
    finally:
        shutil.rmtree(work, ignore_errors=True)
