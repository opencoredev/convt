"""Finish the static coming-soon build in dist/client for Vercel.

Copies landing-static/ (vercel.json, llms.txt, robots.txt and the Markdown pages),
renders about, privacy and 404 to HTML with page.html, inlines the home page CSS,
and writes sitemap.xml.
The Markdown files stay in the output: Vercel serves them to agents that send
Accept: text/markdown (see the routes in vercel.json).
"""

import datetime
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


template = (src / "page.html").read_text()
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

# Inline the stylesheet into the prerendered home page. Its URLs are absolute (/assets/...),
# and on a slow phone the separate request cost a full round trip before first paint.
index = out / "index.html"
page = index.read_text()
link = re.search(r'<link rel="stylesheet" href="(/assets/styles-[^"]+\.css)"[^>]*/>', page)
assert link, "stylesheet link not found in index.html"
css = (out / link[1].lstrip("/")).read_text()
page = page.replace(link[0], f"<style>{css}</style>", 1)
# The modulepreload hints fetch the hydration JavaScript at high priority, where it
# competes with the fonts and images of the first paint on a slow phone.
page = re.sub(r'<link rel="modulepreload"[^>]*/>', "", page)
# Start hydration once the page has loaded and the browser is idle. Without JavaScript
# the page is complete; hydration only wires up the Monthly/Yearly switch.
entry = re.search(r'<script type="module" async="" src="(/assets/index-[^"]+\.js)"></script>', page)
assert entry, "entry script not found in index.html"
loader = (
    "<script>addEventListener('load',function(){var go=function(){import('%s')};"
    "'requestIdleCallback' in window?requestIdleCallback(go,{timeout:2000}):setTimeout(go,200)})</script>"
) % entry[1]
page = page.replace(entry[0], loader, 1)
index.write_text(page)

today = datetime.date.today().isoformat()
urls = "".join(
    f"  <url><loc>{origin}{path}</loc><lastmod>{today}</lastmod></url>\n" for path in ["/", "/about", "/privacy"]
)
(out / "sitemap.xml").write_text(
    '<?xml version="1.0" encoding="UTF-8"?>\n'
    '<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">\n' + urls + "</urlset>\n"
)
print(f"Static extras written to {out}")
