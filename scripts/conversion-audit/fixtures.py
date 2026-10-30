"""Builds one sample per input format for the conversion audit.

Independent of convt: samples come from FFmpeg, Pillow and LibreOffice, so a
convt bug can't hide in its own fixtures. HEIC is the exception (no standalone
encoder here); audit.py makes it from the PNG with convt and says so.

    python3 -I scripts/conversion-audit/fixtures.py OUT_DIR
"""

import os
import subprocess
import sys
import tempfile
import zipfile
from pathlib import Path

from PIL import Image

W, H = 64, 48
# Quadrant colors: top-left, top-right, bottom-left, bottom-right.
QUADS = [(255, 0, 0), (0, 200, 0), (0, 0, 255), (255, 255, 0)]

# Marker words a checker looks for. Each page or slide has its own.
PAGE_WORDS = ["ALPHAPAGE", "BRAVOPAGE", "CHARLIEPAGE"]
DOC_WORDS = ["Quarterly", "convtmarker"]
SHEET = [["Fruit", "Count"], ["Apple", "42"], ["Pear", "7"]]
SLIDE_WORDS = ["FIRSTSLIDE", "SECONDSLIDE"]
SRT = "1\n00:00:00,500 --> 00:00:01,000\nALPHAPAGE subtitle line\n\n2\n00:00:01,200 --> 00:00:01,900\nBRAVOPAGE second cue\n"


def run(*args, **kw):
    subprocess.run(args, check=True, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE, timeout=180, **kw)


def ffmpeg(*args):
    run("ffmpeg", "-hide_banner", "-loglevel", "error", "-y", *args)


def soffice(src: Path, to: str, outdir: Path):
    with tempfile.TemporaryDirectory() as profile:
        run(
            "soffice",
            f"-env:UserInstallation=file://{profile}",
            "--headless",
            "--norestore",
            "--convert-to",
            to,
            "--outdir",
            str(outdir),
            str(src),
        )


def quad_image(alpha: bool) -> Image.Image:
    mode = "RGBA" if alpha else "RGB"
    im = Image.new(mode, (W, H))
    px = im.load()
    for y in range(H):
        for x in range(W):
            c = QUADS[(y >= H // 2) * 2 + (x >= W // 2)]
            px[x, y] = c + (255,) if alpha else c
    if alpha:
        # A transparent strip down the middle-left so alpha handling shows.
        for y in range(H):
            for x in range(4):
                px[x, y] = (0, 0, 0, 0)
    return im


def images(out: Path):
    rgba = quad_image(True)
    rgb = quad_image(False)
    rgba.save(out / "sample.png")
    rgb.save(out / "sample.jpg", quality=95)
    rgba.save(out / "sample.webp", lossless=True)
    # Two seconds of animation; the first frame is the quadrant pattern, so
    # still targets can be checked, and later frames rotate the colors.
    frames = [rgb] + [rgb.rotate(180 * (i % 2)) for i in range(1, 20)]
    frames[0].save(out / "sample.gif", save_all=True, append_images=frames[1:], duration=100, loop=0)
    rgba.save(out / "sample.tiff")
    rgb.save(out / "sample.bmp")
    rgba.save(out / "sample.ico", sizes=[(W, H)])
    rgba.save(out / "sample.tga")
    rgb.save(out / "sample.ppm")
    ffmpeg("-i", str(out / "sample.png"), str(out / "sample.qoi"))
    ffmpeg("-i", str(out / "sample.png"), "-pix_fmt", "gbrpf32le", "-compression", "0", str(out / "sample.exr"))
    ffmpeg("-i", str(out / "sample.png"), "-c:v", "libaom-av1", "-still-picture", "1", "-pix_fmt", "yuv444p", str(out / "sample.avif"))


SVG = f"""<svg xmlns="http://www.w3.org/2000/svg" width="{W}" height="{H}" viewBox="0 0 {W} {H}">
<rect x="0" y="0" width="32" height="24" fill="#ff0000"/>
<rect x="32" y="0" width="32" height="24" fill="#00c800"/>
<rect x="0" y="24" width="32" height="24" fill="#0000ff"/>
<rect x="32" y="24" width="32" height="24" fill="#ffff00"/>
</svg>
"""


def media(out: Path, tmp: Path):
    # 2 s of moving test pattern, a 440 Hz tone and (where the container can
    # hold one) a text subtitle track, so subtitle extraction is testable.
    srt = tmp / "subs.srt"
    srt.write_text(SRT)
    v = ["-f", "lavfi", "-i", "testsrc2=size=160x120:rate=15:duration=2", "-f", "lavfi", "-i", "sine=frequency=440:duration=2"]
    s = ["-i", str(srt), "-map", "0", "-map", "1", "-map", "2"]
    h264 = ["-c:v", "libx264", "-pix_fmt", "yuv420p"]
    ffmpeg(*v, *s, *h264, "-c:a", "aac", "-c:s", "mov_text", "-t", "2", str(out / "sample.mp4"))
    ffmpeg(*v, *s, *h264, "-c:a", "aac", "-c:s", "mov_text", "-t", "2", str(out / "sample.mov"))
    ffmpeg(*v, *s, "-c:v", "libvpx-vp9", "-b:v", "200k", "-c:a", "libopus", "-c:s", "webvtt", "-t", "2", str(out / "sample.webm"))
    ffmpeg(*v, *s, *h264, "-c:a", "flac", "-c:s", "srt", "-t", "2", str(out / "sample.mkv"))
    ffmpeg(*v, "-c:v", "mpeg4", "-c:a", "mp3", "-shortest", str(out / "sample.avi"))
    a = ["-f", "lavfi", "-i", "sine=frequency=440:duration=2"]
    ffmpeg(*a, "-c:a", "libmp3lame", str(out / "sample.mp3"))
    ffmpeg(*a, str(out / "sample.wav"))
    ffmpeg(*a, str(out / "sample.flac"))
    ffmpeg(*a, "-c:a", "aac", "-f", "adts", str(out / "sample.aac"))
    ffmpeg(*a, "-c:a", "aac", str(out / "sample.m4a"))
    ffmpeg(*a, "-c:a", "libvorbis", str(out / "sample.ogg"))
    ffmpeg(*a, "-c:a", "libopus", str(out / "sample.opus"))
    ffmpeg(*a, str(out / "sample.aiff"))
    ffmpeg(*a, "-c:a", "pcm_s16le", str(out / "sample.caf"))
    ffmpeg(*a, "-c:a", "aac", "-f", "ipod", str(out / "sample.m4r"))


def doc_html() -> str:
    pages = "".join(
        f'<h1 style="page-break-before:{"always" if i else "auto"}">{w}</h1>'
        f"<p>{DOC_WORDS[0]} report page {i + 1}. The {DOC_WORDS[1]} word must survive.</p>"
        for i, w in enumerate(PAGE_WORDS)
    )
    table = "<table border=1>" + "".join(
        "<tr>" + "".join(f"<td>{c}</td>" for c in row) + "</tr>" for row in SHEET
    ) + "</table>"
    return f"<!DOCTYPE html><html><head><meta charset=utf-8><title>Sample</title></head><body>{pages}{table}</body></html>\n"


FODP = """<?xml version="1.0" encoding="UTF-8"?>
<office:document xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0"
 xmlns:draw="urn:oasis:names:tc:opendocument:xmlns:drawing:1.0"
 xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0"
 xmlns:svg="urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0"
 xmlns:presentation="urn:oasis:names:tc:opendocument:xmlns:presentation:1.0"
 office:version="1.3" office:mimetype="application/vnd.oasis.opendocument.presentation">
<office:body><office:presentation>
<draw:page draw:name="one"><draw:frame svg:x="2cm" svg:y="2cm" svg:width="20cm" svg:height="3cm"><draw:text-box><text:p>FIRSTSLIDE title</text:p></draw:text-box></draw:frame></draw:page>
<draw:page draw:name="two"><draw:frame svg:x="2cm" svg:y="2cm" svg:width="20cm" svg:height="3cm"><draw:text-box><text:p>SECONDSLIDE body</text:p></draw:text-box></draw:frame></draw:page>
</office:presentation></office:body></office:document>
"""


def documents(out: Path, tmp: Path):
    html = doc_html()
    (out / "sample.html").write_text(html)
    src = tmp / "doc.html"
    src.write_text(html)
    # Writer import of HTML, then export to each word-processor format.
    for to, name in [
        ("docx:MS Word 2007 XML", "docx"),
        ("doc:MS Word 97", "doc"),
        ("odt:writer8", "odt"),
        ("rtf:Rich Text Format", "rtf"),
    ]:
        d = tmp / name
        d.mkdir()
        run(
            "soffice",
            f"-env:UserInstallation=file://{tmp}/profile-{name}",
            "--headless",
            "--norestore",
            "--infilter=HTML (StarWriter)",
            "--convert-to",
            to,
            "--outdir",
            str(d),
            str(src),
        )
        (d / f"doc.{name}").rename(out / f"sample.{name}")
    d = tmp / "pdf"
    d.mkdir()
    run(
        "soffice",
        f"-env:UserInstallation=file://{tmp}/profile-pdf",
        "--headless",
        "--norestore",
        "--infilter=HTML (StarWriter)",
        "--convert-to",
        "pdf:writer_pdf_Export",
        "--outdir",
        str(d),
        str(src),
    )
    (d / "doc.pdf").rename(out / "sample.pdf")

    (out / "sample.txt").write_text(
        "\n\n".join(f"{w}\n{DOC_WORDS[0]} report page {i + 1}. The {DOC_WORDS[1]} word must survive." for i, w in enumerate(PAGE_WORDS)) + "\n"
    )
    (out / "sample.md").write_text(
        "\n\n".join(f"# {w}\n\n{DOC_WORDS[0]} report page {i + 1}. The **{DOC_WORDS[1]}** word must survive." for i, w in enumerate(PAGE_WORDS))
        + "\n\n| Fruit | Count |\n|---|---|\n| Apple | 42 |\n| Pear | 7 |\n"
    )
    (out / "sample.json").write_text('[{"Fruit": "Apple", "Count": 42}, {"Fruit": "Pear", "Count": 7}]\n')
    (out / "sample.yaml").write_text("- Fruit: Apple\n  Count: 42\n- Fruit: Pear\n  Count: 7\n")
    (out / "sample.csv").write_text("".join(",".join(r) + "\n" for r in SHEET))
    (out / "sample.tsv").write_text("".join("\t".join(r) + "\n" for r in SHEET))
    (out / "sample.srt").write_text(SRT)
    (out / "sample.vtt").write_text(
        "WEBVTT\n\n00:00:00.500 --> 00:00:01.000\nALPHAPAGE subtitle line\n\n00:00:01.200 --> 00:00:01.900\nBRAVOPAGE second cue\n"
    )

    for to, name in [("xlsx:Calc MS Excel 2007 XML", "xlsx"), ("xls:MS Excel 97", "xls"), ("ods:calc8", "ods")]:
        d = tmp / name
        d.mkdir()
        run(
            "soffice",
            f"-env:UserInstallation=file://{tmp}/profile-{name}",
            "--headless",
            "--norestore",
            "--convert-to",
            to,
            "--outdir",
            str(d),
            str(out / "sample.csv"),
        )
        (d / f"sample.{name}").rename(out / f"sample.{name}")

    fodp = tmp / "slides.fodp"
    fodp.write_text(FODP)
    for to, name in [("pptx:Impress MS PowerPoint 2007 XML", "pptx"), ("ppt:MS PowerPoint 97", "ppt"), ("odp:impress8", "odp")]:
        d = tmp / name
        d.mkdir()
        run(
            "soffice",
            f"-env:UserInstallation=file://{tmp}/profile-{name}",
            "--headless",
            "--norestore",
            "--convert-to",
            to,
            "--outdir",
            str(d),
            str(fodp),
        )
        (d / f"slides.{name}").rename(out / f"sample.{name}")


def main():
    out = Path(sys.argv[1])
    out.mkdir(parents=True, exist_ok=False)
    images(out)
    (out / "sample.svg").write_text(SVG)
    with tempfile.TemporaryDirectory() as tmp:
        media(out, Path(tmp))
    with tempfile.TemporaryDirectory() as tmp:
        documents(out, Path(tmp))
    for p in sorted(out.iterdir()):
        assert p.stat().st_size > 0, p
        if p.suffix in (".docx", ".xlsx", ".pptx", ".odt", ".ods", ".odp"):
            assert zipfile.ZipFile(p).testzip() is None, p
    print("\n".join(sorted(p.name for p in out.iterdir())))


if __name__ == "__main__":
    main()
