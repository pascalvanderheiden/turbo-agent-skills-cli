#!/usr/bin/env python3

import argparse
import html
import re
import sys
from pathlib import Path
from urllib.parse import urlparse


SITE = Path(__file__).parent
README = SITE.parents[1] / "README.md"
START = "<!-- README-CONTENT:START -->"
END = "<!-- README-CONTENT:END -->"
SOURCE_START = "<!-- pages-content:start -->"
SOURCE_END = "<!-- pages-content:end -->"


def render_inline(text):
    token = re.compile(r"`([^`]+)`|(!?)\[([^\]]*)\]\(([^)]+)\)|\*\*(.+?)\*\*")
    output = []
    position = 0
    for match in token.finditer(text):
        output.append(html.escape(text[position : match.start()]))
        code, image, label, href, strong = match.groups()
        if code is not None:
            output.append(f"<code>{html.escape(code)}</code>")
        elif label is not None:
            href = href.strip()
            parsed = urlparse(href)
            if (
                parsed.scheme not in ("", "https", "http", "mailto")
                or href.startswith("//")
                or "\\" in href
                or any(ord(character) < 32 for character in href)
            ):
                raise ValueError(f"unsupported link in README landing content: {href}")
            if image:
                if href.startswith("docs/site/assets/"):
                    href = href.removeprefix("docs/site/")
                output.append(
                    f'<img class="product-shot" src="{html.escape(href, quote=True)}" '
                    f'alt="{html.escape(label, quote=True)}">'
                )
            else:
                output.append(f'<a href="{html.escape(href, quote=True)}">{html.escape(label)}</a>')
        else:
            output.append(f"<strong>{html.escape(strong)}</strong>")
        position = match.end()
    output.append(html.escape(text[position:]))
    return "".join(output)


def render_markdown(source):
    lines = source.strip().splitlines()
    output = []
    paragraph = []
    list_open = False
    code = []
    in_code = False
    heading_ids = set()

    def flush_paragraph():
        if paragraph:
            output.append(f"<p>{render_inline(' '.join(line.strip() for line in paragraph))}</p>")
            paragraph.clear()

    def close_list():
        nonlocal list_open
        if list_open:
            output.append("</ul>")
            list_open = False

    for line in lines:
        if line.startswith("```"):
            if in_code:
                output.append(f"<pre><code>{html.escape(chr(10).join(code))}</code></pre>")
                code.clear()
                in_code = False
            else:
                flush_paragraph()
                close_list()
                in_code = True
            continue
        if in_code:
            code.append(line)
            continue

        heading = re.match(r"^(#{1,3})\s+(.+?)\s*$", line)
        if heading:
            flush_paragraph()
            close_list()
            level = len(heading.group(1))
            title = heading.group(2)
            base_id = re.sub(r"[^a-z0-9]+", "-", title.casefold()).strip("-")
            identifier = base_id
            suffix = 2
            while identifier in heading_ids:
                identifier = f"{base_id}-{suffix}"
                suffix += 1
            heading_ids.add(identifier)
            output.append(f'<h{level} id="{identifier}">{render_inline(title)}</h{level}>')
        elif line.startswith("> "):
            flush_paragraph()
            close_list()
            output.append(f"<p class=\"banner\">{render_inline(line[2:].strip())}</p>")
        elif line.startswith("- "):
            flush_paragraph()
            if not list_open:
                output.append("<ul>")
                list_open = True
            output.append(f"<li>{render_inline(line[2:].strip())}</li>")
        elif not line.strip():
            flush_paragraph()
            close_list()
        else:
            flush_paragraph()
            close_list()
            paragraph.append(line)

    if in_code:
        raise ValueError("unclosed code fence in README landing content")
    flush_paragraph()
    close_list()
    return "\n      ".join(output)


def generated_content():
    readme = README.read_text(encoding="utf-8")
    match = re.search(
        rf"{re.escape(SOURCE_START)}(.*?){re.escape(SOURCE_END)}",
        readme,
        re.DOTALL,
    )
    if not match:
        raise ValueError("README landing content markers are missing")
    return render_markdown(match.group(1))


def render_site(template, content):
    if template.count(START) != 1 or template.count(END) != 1:
        raise ValueError("site template must contain one generated-content marker pair")
    before, remainder = template.split(START, 1)
    _, after = remainder.split(END, 1)
    return f"{before}{START}\n      {content}\n      {END}{after}"


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    path = SITE / "index.html"
    try:
        expected = render_site(path.read_text(encoding="utf-8"), generated_content())
    except (OSError, ValueError) as error:
        print(error, file=sys.stderr)
        return 1
    if args.check:
        if path.read_text(encoding="utf-8") != expected:
            print("docs/site/index.html is stale; run python3 docs/site/generate.py", file=sys.stderr)
            return 1
    else:
        path.write_text(expected, encoding="utf-8")
    print("Pages landing content is current.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
