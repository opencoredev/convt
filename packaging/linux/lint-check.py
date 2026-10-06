#!/usr/bin/env python3
"""Fail on findings outside the documented private-bundle exceptions."""
import pathlib
import re
import sys

family, filename, status = sys.argv[1:]
status = int(status)
text = pathlib.Path(filename).read_text()
common = {
    "script-not-executable": "Upstream notice recipes are retained as data files.",
    "executable-not-elf-or-script": "Dolphin requires executable service-menu desktop files.",
    "initial-upload-closes-no-bugs": "This local verification package is not a Debian archive upload.",
    "national-encoding": "Retain the original PDFium FreeType notice bytes.",
    "dir-or-file-in-opt": "The requested relocatable payload lives under /opt/convt.",
    "statically-linked-binary": "Pinned static FFmpeg and ffprobe are part of the audited payload.",
    "embedded-library": "Bundled PDFium and codec libraries provide the offline conversion engines.",
    "binary-or-shlib-defines-rpath": "Main executables use fixed $ORIGIN/lib RPATH so child tools inherit no private loader path; replaceable GCC runtimes use an origin-relative RUNPATH.",
    "invalid-soname": "Pinned PDFium is a private dynamically loaded unversioned library.",
    "files-duplicate": "Upstream licence and build provenance files repeat upstream content.",
    "files-duplicated-waste": "Retain complete upstream notices and provenance without rewriting the payload.",
    "devel-file-in-non-devel-package": "Upstream compiler notices include its original recipe and test source.",
    "position-independent-executable-suggested": "Pinned third-party static FFmpeg binaries are copied unchanged.",
    "zero-length": "Empty upstream compiler git receipts are retained with provenance.",
    "incorrect-fsf-address": "Retain the hash-locked upstream wayland-protocols-plasma 0.3.12 notice unchanged, including its historical address.",
    "one-line-command-in-%post": "The shared informative-only helper prints migration guidance; it never accesses user homes.",
    "non-executable-script": "Notice recipes and the explicitly Python-invoked user installer are data files.",
}
# Every line from the underlying linter stays in the raw evidence. This gate
# accepts only known tags, and checks their paths where the policy is specific.
patterns = {
    "deb": r"^[EWI]: convt(?: [^:]+)?: ([a-z0-9-]+)(.*)$",
    "rpm": r"^convt\.x86_64: [EW]: ([a-z0-9%-]+)(.*)$",
}
findings = re.findall(patterns[family], text, re.M)
if re.search(r"Traceback|Segmentation fault|Internal error|Fatal error", text, re.I):
    sys.exit("Linter failed internally")
if family == "deb":
    if status not in {0, 1, 2} or (status != 0 and not findings):
        sys.exit(f"Lintian failed with status {status}")
    for line in text.splitlines():
        if line.strip() and not re.match(r"^[EWI]: convt(?: [^:]+)?: |^N:", line):
            sys.exit("Unexpected lintian output: " + line)
else:
    footer = re.search(r"(\d+) packages and (\d+) specfiles checked; (\d+) errors, (\d+) warnings", text)
    if not footer or int(footer[1]) != 1 or int(footer[2]) != 0:
        sys.exit("RPM lint did not complete")
    counts = [len(re.findall(r"^convt\.x86_64: " + level + r":", text, re.M)) for level in ["E", "W"]]
    if counts != [int(footer[3]), int(footer[4])]:
        sys.exit("Incomplete RPM lint findings")
    if status not in {0, 64, 66}:
        sys.exit(f"RPM lint failed with status {status}")
    after_finding = False
    for line in text.splitlines():
        if line.startswith("convt.x86_64:"):
            after_finding = True
        elif after_finding and line.strip() and not re.match(r"^\s*\d+ packages and", line):
            sys.exit("Unexpected RPM lint output: " + line)
for tag, detail in findings:
    if tag not in common:
        sys.exit(f"Unapproved {family} finding: {tag}{detail}")
    if tag == "incorrect-fsf-address" and detail.strip() != "/opt/convt/licenses/rust/wayland-protocols-plasma-0.3.12.txt":
        sys.exit("Unexpected historical notice address: " + detail)
    if tag == "one-line-command-in-%post" and detail.strip() != "/usr/share/convt/menu-migration-message.sh":
        sys.exit("Unexpected post-install command: " + detail)
    if tag == "dir-or-file-in-opt" and "opt/convt" not in detail:
        sys.exit("Unexpected /opt path: " + detail)
    if tag in {"non-executable-script", "script-not-executable", "zero-length"}:
        if "opt/convt/licenses/" not in detail and "usr/share/convt/integrations/" not in detail:
            sys.exit("Unexpected provenance finding: " + detail)
    normalized = detail.replace("[", " ").replace("]", " ").split()
    paths = {"/" + x.lstrip("/") for x in normalized if x.lstrip("/").startswith(("opt/", "usr/"))}
    if tag == "statically-linked-binary" and not paths <= {"/opt/convt/ffmpeg", "/opt/convt/ffprobe"}:
        sys.exit("Unexpected static binary: " + detail)
    if tag == "embedded-library" and not paths <= {"/opt/convt/ffmpeg", "/opt/convt/ffprobe", "/opt/convt/lib/libpdfium.so"}:
        sys.exit("Unexpected embedded library: " + detail)
    if tag == "invalid-soname" and paths != {"/opt/convt/lib/libpdfium.so"}:
        sys.exit("Unexpected private SONAME: " + detail)
    if tag == "devel-file-in-non-devel-package" and not any(x in detail for x in ["/opt/convt/licenses/", "/opt/convt/lib/"]):
        sys.exit("Unexpected development file: " + detail)
    if tag == "executable-not-elf-or-script" and "usr/share/kio/servicemenus/convt-0.desktop" not in detail:
        sys.exit("Unexpected executable data: " + detail)
    if tag == "national-encoding" and "opt/convt/licenses/pdfium/freetype.txt" not in detail:
        sys.exit("Unexpected encoded notice: " + detail)
    if tag == "binary-or-shlib-defines-rpath":
        origins = re.findall(r'\$ORIGIN[^)\]\s"\']*', detail)
        main = paths in [{"/opt/convt/convt.bin"}, {"/opt/convt/convt-app.bin"}] and origins == ["$ORIGIN/lib"]
        compiler = paths in [{"/opt/convt/lib/libgcc_s.so.1"}, {"/opt/convt/lib/libstdc++.so.6"}] and origins == ["$ORIGIN/."]
        if not (main or compiler):
            sys.exit("Unexpected RUNPATH: " + detail)
print(f"Accepted {len(findings)} documented {family} findings; no unapproved findings")
