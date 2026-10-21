#!/usr/bin/env python3
"""Build the Tanks user guide: docs/guide/chapters/*.md -> out/guide/tanks-user-guide.pdf.

Runs inside the localhost/tanks-guide image (WeasyPrint, python-markdown, Pillow); see
docs/guide/README.md. Screenshots come from out/guide/img (made by the shots-*.sh scripts).

Chapters are concatenated in file-name order. Markdown extensions used by the chapters:
  ::fig <name> | crop=x,y,w,h | pins=1@x,y;2@x,y | w=80 | Caption text
      A screenshot from out/guide/img/<name>.png, cropped (source pixels), with numbered
      callouts at source-pixel positions and a caption. Every field but the name is optional.
  ::formations <pack-id>
      Formation diagrams drawn from data/doctrine/<pack-id>/doctrine.ron.
  {{img:<name>[:x,y,w,h]}}
      URI of screenshot <name> (optionally cropped) as a JPEG (cover art).
  {{ch:<chapter title>}} / {{chn:<chapter title>}}
      "Chapter N" (linked) / "N" for the chapter whose title is or starts with the words.
"""
import html
import re
import sys
from pathlib import Path

import markdown
from PIL import Image
from weasyprint import HTML

ROOT = Path(__file__).resolve().parents[2]
SRC = ROOT / "docs/guide"
RAW = ROOT / "out/guide/img"
OUT = ROOT / "out/guide"
BUILD = OUT / "build"
FIG_N = [0]


def figure(spec: str) -> str:
    parts = [p.strip() for p in spec.split("|")]
    name, opts, caption = parts[0], {}, ""
    for p in parts[1:]:
        m = re.match(r"^(crop|pins|w)=(.*)$", p)
        if m:
            opts[m.group(1)] = m.group(2)
        else:
            caption = p
    src = RAW / f"{name}.png"
    if not src.exists():
        sys.exit(f"missing screenshot {src}")
    im = Image.open(src).convert("RGB")
    x, y, w, h = (int(v) for v in opts.get("crop", f"0,0,{im.width},{im.height}").split(","))
    im = im.crop((x, y, x + w, y + h))
    tag = f"{name}-{x}-{y}-{w}-{h}"
    dst = BUILD / "img" / f"{tag}.jpg"
    dst.parent.mkdir(parents=True, exist_ok=True)
    im.save(dst, "JPEG", quality=86, optimize=True)
    pins = ""
    for pin in filter(None, opts.get("pins", "").split(";")):
        label, at = pin.split("@")
        px, py = (float(v) for v in at.split(","))
        left, top = (px - x) / w * 100, (py - y) / h * 100
        pins += f'<span class="pin" style="left:{left:.2f}%;top:{top:.2f}%">{html.escape(label)}</span>'
    FIG_N[0] += 1
    width = opts.get("w", "100")
    side = ";display:inline-block;vertical-align:top;margin:2mm 0.5%" if float(width) <= 50 else ""
    cap = f'<figcaption><b>Figure {FIG_N[0]}.</b> {markdown.markdown(caption)[3:-4]}</figcaption>' if caption else ""
    return (
        f'<figure class="shot" style="width:{width}%{side}">'
        f'<div class="frame"><img src="{dst.as_uri()}" alt="{html.escape(name)}">{pins}</div>{cap}</figure>'
    )


def formations(pack: str) -> str:
    """Formation diagrams from a doctrine pack's slot table (x right, y forward, in spacings)."""
    text = (ROOT / f"data/doctrine/{pack}/doctrine.ron").read_text()
    rows = re.findall(r"\(formation:\s*(\w+),\s*slots:\s*\[([^\]]*)\]\)", text)
    cells = []
    for name, slots in rows:
        pts = [(float(a), float(b)) for a, b in re.findall(r"\(\s*(-?[\d.]+),\s*(-?[\d.]+)\s*\)", slots)]
        s, cx, cy = 17.0, 80.0, 28.0
        dots = []
        for k, (sx, sy) in enumerate(pts):
            px, py = cx + sx * s, cy - sy * s
            ring = '<circle cx="{0:.1f}" cy="{1:.1f}" r="9" fill="none" stroke="#c9a54b" stroke-width="1.4"/>'.format(px, py) if k == 0 else ""
            dots.append(
                f'{ring}<rect x="{px - 7:.1f}" y="{py - 5:.1f}" width="14" height="10" fill="#dfe9f2" stroke="#2f5d86" stroke-width="1.2"/>'
                f'<text x="{px:.1f}" y="{py + 3.2:.1f}" font-size="8" text-anchor="middle" fill="#1d3550">{k + 1}</text>'
            )
        label = name.replace("_", " ").upper()
        svg = (
            f'<svg viewBox="0 0 160 140" width="150" height="131" xmlns="http://www.w3.org/2000/svg">'
            f'<line x1="80" y1="22" x2="80" y2="6" stroke="#777" stroke-width="1"/>'
            f'<polygon points="76,10 80,3 84,10" fill="#777"/>'
            f'<g transform="translate(0,22)">{"".join(dots)}</g>'
            f'<text x="80" y="136" font-size="10" text-anchor="middle" font-family="IBM Plex Mono" fill="#333">{label}</text>'
            f"</svg>"
        )
        cells.append(f'<div class="fcell">{svg}</div>')
    return f'<div class="formations">{"".join(cells)}</div>'


def img_uri(name: str, crop: str | None) -> str:
    dst = BUILD / "img" / f"{name}-{(crop or 'full').replace(',', '-')}.jpg"
    dst.parent.mkdir(parents=True, exist_ok=True)
    im = Image.open(RAW / f"{name}.png").convert("RGB")
    if crop:
        x, y, w, h = (int(v) for v in crop.split(","))
        im = im.crop((x, y, x + w, y + h))
    im.save(dst, "JPEG", quality=86, optimize=True)
    return dst.as_uri()


def preprocess(md: str) -> str:
    md = re.sub(r"\{\{img:([\w-]+)(?::([\d,]+))?\}\}", lambda m: img_uri(m.group(1), m.group(2)), md)
    out = []
    for line in md.splitlines():
        if line.startswith("::fig "):
            out.append(figure(line[6:]))
        elif line.startswith("::formations "):
            out.append(formations(line[13:].strip()))
        else:
            out.append(line)
    return "\n".join(out)


CHAPTERS: list[tuple[str, str, str]] = []  # (number, title, id) of every h1


def chapter_refs(body: str) -> str:
    """{{ch:Title}} -> "Chapter N" (linked), {{chn:Title}} -> "N"; Title = exact or leading words."""

    def find(key: str):
        k = key.strip().lower()
        hit = [c for c in CHAPTERS if c[1].lower() == k] or [c for c in CHAPTERS if c[1].lower().startswith(k)]
        if len(hit) != 1:
            sys.exit(f"chapter reference {key!r} matches {len(hit)} chapters")
        return hit[0]

    body = re.sub(r"\{\{ch:([^}]+)\}\}", lambda m: (lambda c: f'<a href="#{c[2]}">Chapter {c[0]}</a>')(find(m.group(1))), body)
    return re.sub(r"\{\{chn:([^}]+)\}\}", lambda m: find(m.group(1))[0], body)


def toc(body: str) -> tuple[str, str]:
    """Number h1/h2, give them ids and build a table of contents with page numbers."""
    items, n1, n2 = [], 0, 0

    def sub(m):
        nonlocal n1, n2
        level, attrs, text = m.group(1), m.group(2) or "", m.group(3)
        if "nonum" in attrs:
            return m.group(0)
        if level == "1":
            n1, n2 = n1 + 1, 0
            num = f"{n1}"
        else:
            n2 += 1
            num = f"{n1}.{n2}"
        hid = f"s{num.replace('.', '-')}"
        items.append((level, num, re.sub("<[^>]+>", "", text), hid))
        if level == "1":
            CHAPTERS.append((num, html.unescape(re.sub("<[^>]+>", "", text)), hid))
        return f'<h{level} id="{hid}"{attrs}><span class="num">{num}</span> {text}</h{level}>'

    body = re.sub(r"<h([12])((?:\s[^>]*)?)>(.*?)</h\1>", sub, body)
    lis = "".join(
        f'<li class="t{lv}"><a href="#{hid}"><span class="tn">{num}</span> {html.escape(t)}</a></li>'
        for lv, num, t, hid in items
    )
    return body, f'<nav class="toc"><h1 class="nonum">Contents</h1><ul>{lis}</ul></nav>'


def main():
    BUILD.mkdir(parents=True, exist_ok=True)
    chapters = sorted((SRC / "chapters").glob("*.md"))
    md = preprocess("\n\n".join(c.read_text() for c in chapters))
    # The cover and part pages are raw HTML in the chapters; everything else is Markdown.
    body = markdown.markdown(md, extensions=["extra", "sane_lists", "attr_list"])
    body, contents = toc(body)
    body = body.replace("<p>[[TOC]]</p>", contents)
    body = chapter_refs(body)
    css = (SRC / "style.css").read_text().replace("FONT_DIR", (ROOT / "data/fonts").as_uri())
    page = f'<!doctype html><html><head><meta charset="utf-8"><title>Tanks: User Guide</title><style>{css}</style></head><body>{body}</body></html>'
    (BUILD / "guide.html").write_text(page)
    pdf = OUT / "tanks-user-guide.pdf"
    HTML(string=page, base_url=str(SRC)).write_pdf(pdf)
    print(f"wrote {pdf} ({pdf.stat().st_size / 1e6:.1f} MB, {FIG_N[0]} figures)")


if __name__ == "__main__":
    main()
