"""Finish the static coming-soon build in dist/client for Vercel.

Copies landing-static/ (vercel.json, llms.txt, robots.txt and the Markdown pages),
renders about, privacy and 404 to HTML with page.html, and post-processes the
prerendered pages (inline CSS, late hydration). scripts/landing-pages.ts then writes
the Markdown for the /convert pages, sitemap.xml and the conversions in llms.txt.
The Markdown files stay in the output: Vercel serves them to agents that send
Accept: text/markdown (see the routes in vercel.json).
"""

import html
import re
import shutil
import sys
from pathlib import Path

src = Path(__file__).resolve().parent.parent / "landing-static"
out = Path(sys.argv[1])
origin = "https://convt.app"

for name in ["vercel.json", "robots.txt", "llms.txt", "index.md", "about.md", "privacy.md", "404.md"]:
    shutil.copy(src / name, out / name)


def inline(text: str) -> str:
    text = html.escape(text, quote=False)
    text = re.sub(r"`([^`]+)`", r"<code>\1</code>", text)
    text = re.sub(r"\*\*([^*]+)\*\*", r"<strong>\1</strong>", text)
    return re.sub(r"\[([^\]]+)\]\(([^)]+)\)", lambda m: f'<a href="{html.escape(m[2])}">{m[1]}</a>', text)


def to_html(markdown: str) -> str:
    """Headings, paragraphs, "- " lists and inline code, bold and links: all these pages use."""
    blocks, para, items = [], [], []

    def flush():
        if para:
            blocks.append(f"<p>{inline(' '.join(para))}</p>")
            para.clear()
        if items:
            blocks.append("<ul>" + "".join(f"<li>{inline(i)}</li>" for i in items) + "</ul>")
            items.clear()

    for line in markdown.splitlines():
        if not line.strip():
            flush()
        elif m := re.match(r"(#{1,3}) (.*)", line):
            flush()
            level = len(m[1])
            blocks.append(f"<h{level}>{inline(m[2])}</h{level}>")
        elif line.startswith("- "):
            if para:
                flush()
            items.append(line[2:])
        else:
            para.append(line.strip())
    flush()
    return "\n".join(blocks)


# The Markdown pages use the same Geist file the home page ships (its name is hashed).
geist = next((out / "assets").glob("geist-latin-wght-normal-*.woff2")).name
template = (src / "page.html").read_text().replace("{{geist}}", f"/assets/{geist}")
pages = {
    "about": ("About convt", "Why convt exists and how it converts files on your own computer.", "/about"),
    "privacy": ("Privacy · convt", "What the convt.app website collects: nothing on purpose.", "/privacy"),
    "404": ("Page not found · convt", "There is nothing at this address on convt.app.", "/404"),
}
for name, (title, description, path) in pages.items():
    page = template
    for key, value in {
        "title": html.escape(title),
        "description": html.escape(description),
        "canonical": origin + path,
        "markdown": origin + f"/{name}.md",
        "content": to_html((src / f"{name}.md").read_text()),
    }.items():
        page = page.replace("{{" + key + "}}", value)
    (out / f"{name}.html").write_text(page)

# Post-process every prerendered page (the home page and the /convert pages); the
# About, Privacy and 404 pages above are plain HTML and are skipped.
ours = {"about.html", "privacy.html", "404.html"}
prerendered = [p for p in out.rglob("*.html") if p.relative_to(out).as_posix() not in ours]
assert prerendered, "no prerendered pages in dist/client"
for path in prerendered:
    page = path.read_text()
    # Inline the stylesheet. Its URLs are absolute (/assets/...), and on a slow phone the
    # separate request cost a full round trip before first paint.
    link = re.search(r'<link rel="stylesheet" href="(/assets/styles-[^"]+\.css)"[^>]*/>', page)
    assert link, f"stylesheet link not found in {path}"
    css = (out / link[1].lstrip("/")).read_text()
    page = page.replace(link[0], f"<style>{css}</style>", 1)
    # The modulepreload hints fetch the hydration JavaScript at high priority, where it
    # competes with the fonts and images of the first paint on a slow phone.
    page = re.sub(r'<link rel="modulepreload"[^>]*/>', "", page)
    # Start hydration once the page has loaded and the browser is idle. Without JavaScript
    # the pages are complete; hydration only wires up the Monthly/Yearly switch.
    entry = re.search(r'<script type="module" async="" src="(/assets/index-[^"]+\.js)"></script>', page)
    assert entry, f"entry script not found in {path}"
    loader = (
        "<script>addEventListener('load',function(){var go=function(){import('%s')};"
        "'requestIdleCallback' in window?requestIdleCallback(go,{timeout:2000}):setTimeout(go,200)})</script>"
    ) % entry[1]
    page = page.replace(entry[0], loader, 1)
    path.write_text(page)

print(f"Static extras written to {out}")
