"""Runs every offered input -> target pair through the convt CLI and checks
each output with tools that share no code with convt: Pillow and FFmpeg for
images, ffprobe and a full FFmpeg decode for media, pdfinfo and pdftotext for
PDFs, LibreOffice (a separate run) for office files, and Python parsers for
text formats.

    python3 -I scripts/conversion-audit/audit.py CONVT FIXTURES WORK REPORT.json [JOBS]

Writes one JSON record per pair. Never deletes anything outside WORK.
"""

import csv
import io
import json
import os
import re
import shutil
import struct
import subprocess
import sys
import tempfile
import zipfile
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

import yaml
from PIL import Image

# -I leaves this directory off sys.path; proctree.py lives next to this file.
sys.path.insert(0, str(Path(__file__).resolve().parent))
from proctree import run as run_tree  # noqa: E402

# `Path("./convt")` prints as "convt", which subprocess would look up on PATH.
CONVT = Path(sys.argv[1]).resolve() if os.sep in sys.argv[1] else Path(shutil.which(sys.argv[1]) or sys.argv[1])
FIX, WORK, REPORT = (Path(a) for a in sys.argv[2:5])
# Each convt case can start FFmpeg or LibreOffice; keep the shared server calm.
JOBS = min(int(sys.argv[5]) if len(sys.argv) > 5 else 4, 4)
FFMPEG = os.environ.get("AUDIT_FFMPEG", "ffmpeg")
# Validation decodes run JOBS at a time; without a cap each sizes its pools to every CPU.
FF_THREADS = ("-threads", "2", "-filter_threads", "1")
HEIF_HELPER = Path(__file__).resolve().parents[1] / "matrix-fixtures.py"
FFPROBE = os.environ.get("AUDIT_FFPROBE", "ffprobe")

PAGE_WORDS = ["ALPHAPAGE", "BRAVOPAGE", "CHARLIEPAGE"]
SLIDE_WORDS = ["FIRSTSLIDE", "SECONDSLIDE"]
QUADS = [(255, 0, 0), (0, 200, 0), (0, 0, 255), (255, 255, 0)]
W, H = 64, 48

FORMATS = {f["id"]: f for f in json.loads(subprocess.check_output([str(CONVT), "formats", "--json"]))}


class Fail(Exception):
    pass


def sh(*args, timeout=180, check=True):
    p = run_tree([str(a) for a in args], capture_output=True, text=True, timeout=timeout)
    if check and p.returncode:
        raise Fail(f"{Path(str(args[0])).name} exit {p.returncode}: {(p.stderr or p.stdout).strip()[-600:]}")
    return p


def category(fid):
    return FORMATS[fid]["category"].lower()


# What each fixture holds, so checks know what must survive.
def expected_words(src):
    if src in ("pptx", "ppt", "odp"):
        return SLIDE_WORDS
    if src in ("xlsx", "xls", "ods", "csv", "tsv", "json", "yaml"):
        return ["Apple", "42", "Pear"]
    if src in ("srt", "vtt") or category(src) == "video":
        return ["ALPHAPAGE", "BRAVOPAGE"]
    return PAGE_WORDS + ["convtmarker"]


def soffice_text(path: Path, tmp: Path) -> str:
    """Text of an office file via an independent LibreOffice run (to PDF, then pdftotext)."""
    d = tmp / "lo"
    d.mkdir(exist_ok=True)
    sh(
        "soffice",
        f"-env:UserInstallation=file://{tmp}/lo-profile",
        "--headless",
        "--norestore",
        "--convert-to",
        "pdf",
        "--outdir",
        d,
        path,
        timeout=240,
    )
    pdfs = list(d.glob("*.pdf"))
    if not pdfs:
        raise Fail("LibreOffice could not open the output")
    return sh("pdftotext", "-layout", pdfs[0], "-").stdout


def check_words(text, words, what):
    missing = [w for w in words if w not in text]
    if missing:
        raise Fail(f"{what}: missing {missing}; got {text[:200]!r}")


def decode_image(path: Path, tmp: Path) -> Image.Image:
    if path.suffix == ".heic":
        return heif_decode(path)
    try:
        im = Image.open(path)
        im.load()
        return im
    except Exception:
        if path.suffix == ".tiff":
            return libtiff_decode(path)
        png = tmp / (path.stem + ".decoded.png")
        sh(FFMPEG, "-v", "error", "-y", *FF_THREADS, "-i", path, "-frames:v", "1", png)
        return Image.open(png)


def libtiff_decode(path: Path) -> Image.Image:
    """Reads float or integer RGB(A) TIFF scanlines through libtiff."""
    import ctypes as c

    t = c.CDLL("libtiff.so.6")
    t.TIFFOpen.restype = c.c_void_p
    t.TIFFOpen.argtypes = [c.c_char_p, c.c_char_p]
    t.TIFFGetField.argtypes = [c.c_void_p, c.c_uint32, c.c_void_p]
    t.TIFFScanlineSize.argtypes = [c.c_void_p]
    t.TIFFReadScanline.argtypes = [c.c_void_p, c.c_void_p, c.c_uint32, c.c_uint16]
    t.TIFFClose.argtypes = [c.c_void_p]
    h = t.TIFFOpen(str(path).encode(), b"r")
    if not h:
        raise Fail("libtiff cannot open the TIFF")
    try:
        w, ht, spp, bps, fmt = c.c_uint32(), c.c_uint32(), c.c_uint16(), c.c_uint16(), c.c_uint16(1)
        t.TIFFGetField(h, 256, c.byref(w))
        t.TIFFGetField(h, 257, c.byref(ht))
        t.TIFFGetField(h, 277, c.byref(spp))
        t.TIFFGetField(h, 258, c.byref(bps))
        t.TIFFGetField(h, 339, c.byref(fmt))
        n = t.TIFFScanlineSize(h)
        buf = c.create_string_buffer(n)
        code = {(3, 32): "f", (1, 16): "H", (1, 8): "B"}[(fmt.value, bps.value)]
        scale = {"f": 255.0, "H": 1 / 257, "B": 1}[code]
        out = Image.new("RGBA", (w.value, ht.value))
        for y in range(ht.value):
            if t.TIFFReadScanline(h, buf, y, 0) != 1:
                raise Fail(f"libtiff scanline {y} failed")
            vals = struct.unpack(f"<{n // struct.calcsize(code)}{code}", buf.raw)
            for x in range(w.value):
                px = [max(0, min(255, round(v * scale))) for v in vals[x * spp.value:(x + 1) * spp.value]]
                out.putpixel((x, y), tuple(px[:3]) + ((px[3],) if spp.value > 3 else (255,)))
        out.format = f"TIFF {bps.value}-bit {'float' if code == 'f' else 'int'} (libtiff)"
        return out
    finally:
        t.TIFFClose(h)


def heif_decode(path: Path) -> Image.Image:
    # libheif through ctypes (scripts/matrix-fixtures.py), not convt's HEIC
    # engine. FFmpeg 6/7 here cannot demux HEIF.
    raw = sh_bytes("python3", "-I", HEIF_HELPER, "heif-pixels", path)
    w, h = struct.unpack("<II", raw[:8])
    return Image.frombytes("RGBA", (w, h), raw[8:])


def check_quadrants(im: Image.Image, src: str):
    rgb = im.convert("RGBA")
    if rgb.size != (W, H):
        if src == "pdf":
            return
        raise Fail(f"size {rgb.size}, expected {(W, H)}")
    pts = [(W * 3 // 8, H // 4), (W * 3 // 4, H // 4), (W * 3 // 8, H * 3 // 4), (W * 3 // 4, H * 3 // 4)]
    for (x, y), want in zip(pts, QUADS):
        got = rgb.getpixel((x, y))[:3]
        if max(abs(a - b) for a, b in zip(got, want)) > 40:
            raise Fail(f"pixel {(x, y)} is {got}, expected about {want}")


def ink_ratio(im: Image.Image) -> float:
    # Share of clearly non-white pixels. Small text on a page scaled to an
    # icon is light gray, so the cut is 200, not mid-gray.
    g = im.convert("L")
    hist = g.histogram()
    return sum(hist[:200]) / max(1, sum(hist))


# Pages a document fixture renders to: the word-processor fixtures have three
# page-separated sections, the slides two, everything else at least one.
def expected_pages(src):
    if src in ("pdf", "docx", "doc", "odt", "rtf", "html"):
        return 3
    if src in ("pptx", "ppt", "odp"):
        return 2
    return None


def check_image_outputs(src, to, files, tmp):
    if category(src) in ("pdf", "document", "presentation", "spreadsheet"):
        want = expected_pages(src)
        if want is not None and len(files) != want:
            raise Fail(f"{len(files)} images for a {want}-page source: {[f.name for f in files]}")
        sizes = set()
        for f in files:
            im = decode_image(f, tmp)
            sizes.add(im.size)
            r = ink_ratio(flatten(im.convert("RGBA")))
            if not 0.0003 < r < 0.5:
                raise Fail(f"{f.name}: dark-pixel ratio {r:.4f}; page text not rendered")
        return f"{len(files)} page image(s), {sorted(sizes)}"
    if len(files) != 1:
        raise Fail(f"expected 1 file, got {[f.name for f in files]}")
    im = decode_image(files[0], tmp)
    if category(src) == "video":
        if im.size != (160, 120):
            raise Fail(f"frame size {im.size}")
        return f"frame {im.size}"
    check_quadrants(im, src)
    return f"{im.format or 'decoded'} {im.size}"


def flatten(im):
    bg = Image.new("RGB", im.size, (255, 255, 255))
    bg.paste(im, mask=im.getchannel("A"))
    return bg


def probe(path):
    out = sh(FFPROBE, "-v", "error", "-show_entries", "stream=codec_type,codec_name,width,height:format=format_name,duration", "-of", "json", path).stdout
    return json.loads(out)


def check_media(src, to, files, tmp):
    if len(files) != 1:
        raise Fail(f"expected 1 file, got {[f.name for f in files]}")
    f = files[0]
    info = probe(f)
    streams = info.get("streams", [])
    kinds = {s["codec_type"] for s in streams}
    dur = float(info.get("format", {}).get("duration") or 0)
    # A full decode catches truncated or corrupt output that probes fine.
    dec = sh(FFMPEG, "-v", "error", "-xerror", *FF_THREADS, "-i", f, "-f", "null", "-", check=False)
    if dec.returncode:
        raise Fail(f"decode error: {dec.stderr.strip()[-300:]}")
    to_cat = category(to)
    if to_cat == "audio":
        if kinds != {"audio"}:
            raise Fail(f"streams {kinds}")
    elif to == "gif":
        if "video" not in kinds:
            raise Fail(f"streams {kinds}")
    else:
        want = {"video", "audio"} if category(src) == "video" else {"video"}
        if not want <= kinds:
            raise Fail(f"streams {kinds}, wanted {want}")
    if to != "gif" and not 1.5 < dur < 3.5:
        raise Fail(f"duration {dur}")
    codecs = ",".join(f"{s['codec_type'][0]}:{s['codec_name']}" for s in streams)
    return f"{info['format']['format_name']} {codecs} {dur:.2f}s"


def check_pdf(src, files, tmp):
    if len(files) != 1:
        raise Fail(f"expected 1 file, got {[f.name for f in files]}")
    f = files[0]
    info = sh("pdfinfo", f).stdout
    pages = int(re.search(r"Pages:\s+(\d+)", info).group(1))
    text = sh("pdftotext", "-layout", f, "-").stdout
    cat = category(src)
    if cat in ("image", "vector"):
        if pages != 1:
            raise Fail(f"{pages} pages")
        sh("pdftoppm", "-r", "72", "-png", "-singlefile", f, tmp / "page")
        im = Image.open(tmp / "page.png")
        # Image is scaled onto a page; check it's not blank and has the colors.
        colors = {c for _, c in (im.convert("RGB").getcolors(1 << 20) or [])}
        hits = sum(any(max(abs(a - b) for a, b in zip(c, q)) < 40 for c in colors) for q in QUADS)
        if hits < 4:
            raise Fail(f"rendered page lacks the four quadrant colors ({hits}/4)")
        return f"1 page, image present"
    words = expected_words(src)
    check_words(text, words, "pdftotext")
    if src in ("docx", "doc", "odt", "rtf", "html") and pages < 3:
        raise Fail(f"{pages} pages; source has 3 page-separated sections")
    if src in ("pptx", "ppt", "odp") and pages != 2:
        raise Fail(f"{pages} pages for 2 slides")
    return f"{pages} pages, text ok"


def check_office(src, to, files, tmp):
    if len(files) != 1:
        raise Fail(f"expected 1 file, got {[f.name for f in files]}")
    f = files[0]
    if to in ("docx", "xlsx", "pptx", "odt", "ods", "odp"):
        try:
            z = zipfile.ZipFile(f)
            if z.testzip() is not None:
                raise Fail("corrupt zip member")
        except zipfile.BadZipFile as e:
            raise Fail(f"not a zip: {e}")
    if to in ("doc", "xls", "ppt"):
        if not f.read_bytes().startswith(bytes.fromhex("d0cf11e0a1b11ae1")):
            raise Fail("not an OLE compound file")
    if to == "rtf" and not f.read_bytes().startswith(b"{\\rtf"):
        raise Fail("not RTF")
    text = soffice_text(f, tmp)
    words = expected_words(src)
    if src == "pdf":
        words = PAGE_WORDS
    check_words(text, words, "LibreOffice re-read")
    return "re-read by LibreOffice, text ok"


def check_text(src, to, files, tmp):
    if len(files) != 1:
        raise Fail(f"expected 1 file, got {[f.name for f in files]}")
    raw = files[0].read_bytes()
    try:
        text = raw.decode("utf-8-sig")
    except UnicodeDecodeError as e:
        raise Fail(f"not UTF-8: {e}")
    words = expected_words(src)
    if src == "pdf":
        words = PAGE_WORDS
    if to == "json":
        data = json.loads(text)
        check_words(json.dumps(data), words, "json")
    elif to == "yaml":
        data = yaml.safe_load(text)
        check_words(json.dumps(data, default=str), words, "yaml")
    elif to in ("csv", "tsv"):
        rows = list(csv.reader(io.StringIO(text), delimiter="," if to == "csv" else "\t"))
        flat = [c for r in rows for c in r]
        if not {"Apple", "42"} <= set(flat):
            raise Fail(f"cells {rows[:4]}")
    elif to == "srt":
        if not re.search(r"\d\d:\d\d:\d\d,\d\d\d --> ", text):
            raise Fail("no SRT timing line")
        check_words(text, words, "srt")
    elif to == "vtt":
        if not text.startswith("WEBVTT"):
            raise Fail("no WEBVTT header")
        check_words(text, words, "vtt")
    elif to == "html":
        if "<" not in text:
            raise Fail("no markup")
        check_words(text, words, "html")
    else:
        check_words(text, words, to)
    return f"{len(raw)} bytes, content ok"


def check_heic_brand(files):
    for f in files:
        head = f.read_bytes()[:32]
        if b"ftyp" not in head or not any(b in head for b in (b"heic", b"heix", b"mif1")):
            raise Fail(f"{f.name}: no HEIF ftyp brand: {head!r}")


def sh_bytes(*args):
    p = run_tree([str(a) for a in args], capture_output=True, timeout=120)
    if p.returncode:
        raise Fail(f"{args[2] if len(args) > 2 else args[0]} exit {p.returncode}: {p.stderr.decode()[-400:]}")
    return p.stdout


# Leading bytes each target must start with, so a file of the wrong type under the
# right name fails before any decoder (which reads the real bytes) can accept it.
MAGIC = {
    "png": [(0, b"\x89PNG\r\n\x1a\n")],
    "jpeg": [(0, b"\xff\xd8\xff")],
    "gif": [(0, b"GIF87a"), (0, b"GIF89a")],
    "webp": [(8, b"WEBP")],
    "bmp": [(0, b"BM")],
    "tiff": [(0, b"II*\x00"), (0, b"MM\x00*")],
    "qoi": [(0, b"qoif")],
    "exr": [(0, b"\x76\x2f\x31\x01")],
    "ico": [(0, b"\x00\x00\x01\x00")],
    "ppm": [(0, b"P6"), (0, b"P3")],
    "avif": [(4, b"ftypavi")],
    "pdf": [(0, b"%PDF-")],
    "mp4": [(4, b"ftyp")],
    "mov": [(4, b"ftyp"), (4, b"wide"), (4, b"moov"), (4, b"mdat")],
    "m4a": [(4, b"ftyp")],
    "m4r": [(4, b"ftyp")],
    "mkv": [(0, b"\x1a\x45\xdf\xa3")],
    "webm": [(0, b"\x1a\x45\xdf\xa3")],
    "avi": [(8, b"AVI ")],
    "wav": [(8, b"WAVE")],
    "aiff": [(8, b"AIFF"), (8, b"AIFC")],
    "caf": [(0, b"caff")],
    "flac": [(0, b"fLaC")],
    "ogg": [(0, b"OggS")],
    "opus": [(0, b"OggS")],
    "mp3": [(0, b"ID3"), (0, b"\xff\xfb"), (0, b"\xff\xf3"), (0, b"\xff\xf2"), (0, b"\xff\xfa")],
    "aac": [(0, b"\xff\xf1"), (0, b"\xff\xf9")],
    "docx": [(0, b"PK\x03\x04")],
    "xlsx": [(0, b"PK\x03\x04")],
    "pptx": [(0, b"PK\x03\x04")],
    "odt": [(0, b"PK\x03\x04")],
    "ods": [(0, b"PK\x03\x04")],
    "odp": [(0, b"PK\x03\x04")],
    "doc": [(0, b"\xd0\xcf\x11\xe0")],
    "xls": [(0, b"\xd0\xcf\x11\xe0")],
    "ppt": [(0, b"\xd0\xcf\x11\xe0")],
    "rtf": [(0, b"{\\rtf")],
}


def check_magic(to, files):
    for f in files:
        head = f.read_bytes()[:16]
        sigs = MAGIC.get(to, [])
        if sigs and not any(head[o : o + len(b)] == b for o, b in sigs):
            raise Fail(f"{f.name} is not {to}: starts {head!r}")


def check(src, to, files, tmp):
    for f in files:
        if f.stat().st_size == 0:
            raise Fail(f"{f.name} is empty")
    check_magic(to, files)
    if to == "heic":
        check_heic_brand(files)
    to_cat = category(to)
    if to == "pdf":
        return check_pdf(src, files, tmp)
    if to_cat in ("video", "audio") or (to == "gif" and category(src) == "video"):
        return check_media(src, to, files, tmp)
    if to_cat == "image":
        return check_image_outputs(src, to, files, tmp)
    if to_cat == "vector":
        if len(files) != 1 or b"<svg" not in files[0].read_bytes()[:2000]:
            raise Fail("not SVG")
        return "svg"
    if to in ("docx", "doc", "odt", "rtf", "pptx", "ppt", "odp", "xlsx", "xls", "ods"):
        return check_office(src, to, files, tmp)
    return check_text(src, to, files, tmp)


def fixture(fid):
    for ext in FORMATS[fid]["extensions"]:
        p = FIX / f"sample.{ext}"
        if p.exists():
            return p
    return None


def run_pair(src, to, path):
    case = WORK / f"{src}-{to}"
    case.mkdir()
    tmp = Path(tempfile.mkdtemp(dir=case, prefix="check-"))
    out = case / "out"
    out.mkdir()
    rec = {"from": src, "to": to, "family": category(src)}
    try:
        p = run_tree([str(CONVT), str(path), "--to", to, "-o", str(out)], capture_output=True, text=True, timeout=300)
        rec["exit"] = p.returncode
        if p.returncode:
            raise Fail(f"convt exit {p.returncode}: {(p.stderr or p.stdout).strip()[-800:]}")
        files = sorted(out.iterdir(), key=lambda f: (len(f.name), f.name))
        if not files:
            raise Fail("exit 0 but no output file")
        rec["outputs"] = [f.name for f in files]
        rec["detail"] = check(src, to, files, tmp)
        rec["result"] = "pass"
    except Fail as e:
        rec["result"] = "fail"
        rec["detail"] = str(e)
    except subprocess.TimeoutExpired as e:
        rec["result"] = "fail"
        rec["detail"] = f"timeout: {e}"
    except Exception as e:
        rec["result"] = "fail"
        rec["detail"] = f"checker error {type(e).__name__}: {e}"
    shutil.rmtree(tmp, ignore_errors=True)
    print(f"{rec['result'].upper():4} {src:>5} -> {to:<5} {rec['detail'][:160]}", flush=True)
    return rec


def main():
    if not FIX.is_dir():
        sys.exit(f"audit: fixtures directory {FIX} does not exist")
    # Each pair gets a fresh directory; a reused workspace would stop the run.
    if WORK.exists() and any(WORK.iterdir()):
        sys.exit(f"audit: {WORK} is not empty; pass a new work directory")
    only = os.environ.get("AUDIT_ONLY")
    cases, skipped, menus = [], [], {}
    for fid in FORMATS:
        if only and fid not in only.split(","):
            continue
        path = fixture(fid)
        if path is None:
            skipped.append({"from": fid, "to": "*", "family": category(fid), "result": "skipped", "detail": "no fixture"})
            continue
        targets = sh(CONVT, "targets", path).stdout.split()
        menus[fid] = {"targets": targets, "menu": sh(CONVT, "targets", "--menu", path).stdout.split()}
        for to in targets:
            cases.append((fid, to, path))
    if not cases:
        sys.exit("audit: no conversion ran; every format lacks a fixture")
    WORK.mkdir(exist_ok=True)
    with ThreadPoolExecutor(JOBS) as pool:
        results = list(pool.map(lambda c: run_pair(*c), cases))
    REPORT.write_text(json.dumps({"results": results + skipped, "targets": menus}, indent=1))
    tally = {}
    for r in results + skipped:
        tally.setdefault(r["family"], {}).setdefault(r["result"], 0)
        tally[r["family"]][r["result"]] += 1
    print(json.dumps(tally, indent=1))
    if any(r["result"] == "fail" for r in results):
        sys.exit(1)


if __name__ == "__main__":
    main()
