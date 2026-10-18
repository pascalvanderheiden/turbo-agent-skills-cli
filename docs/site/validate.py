#!/usr/bin/env python3

import argparse
import json
import os
import re
import shlex
import subprocess
import sys
import urllib.error
import urllib.request
from html.parser import HTMLParser
from pathlib import Path
from urllib.parse import quote, unquote, urlencode, urlparse


SITE = Path(__file__).parent
VOID_ELEMENTS = {
    "area",
    "base",
    "br",
    "col",
    "embed",
    "hr",
    "img",
    "input",
    "link",
    "meta",
    "param",
    "source",
    "track",
    "wbr",
}
REQUIRED_TOPICS = {
    "installation": ("install", "install"),
    "lifecycle safety": ("lifecycle-safety", "lifecycle"),
    "discovery": ("discovery", "search"),
    "watchlists": ("watchlists", "watch"),
    "evaluation": ("evaluation", "evaluation"),
    "consistency": ("consistency", "consistency"),
    "insights": ("insights", "insight"),
    "authoring": ("authoring", "author"),
    "security": ("security", "security"),
    "architecture": ("architecture", "architecture"),
    "troubleshooting": ("troubleshooting", "troubleshoot"),
    "releases": ("releases", "release"),
}
EXTERNAL_LINKS = {}


class DocumentParser(HTMLParser):
    def __init__(self, path):
        super().__init__(convert_charrefs=True)
        self.path = path
        self.errors = []
        self.stack = []
        self.ids = set()
        self.links = []
        self.command_examples = []
        self.html_lang = False
        self.main_count = 0
        self.nav_count = 0
        self.title_count = 0
        self.h1_count = 0
        self.link_text = []
        self.current_link_text = []
        self.current_title = False

    def handle_starttag(self, tag, attributes):
        attributes = dict(attributes)
        if tag == "html" and attributes.get("lang"):
            self.html_lang = True
        if tag == "main":
            self.main_count += 1
        if tag == "nav":
            self.nav_count += 1
            if not attributes.get("aria-label"):
                self.errors.append(f"{self.path}: nav must have an accessible label")
        if tag == "title":
            self.title_count += 1
            self.current_title = True
        if tag == "h1":
            self.h1_count += 1
        if tag == "img" and "alt" not in attributes:
            self.errors.append(f"{self.path}: img is missing alt text")
        if tag == "a":
            self.links.append((attributes.get("href", ""), ""))
            self.current_link_text.append("")
        if tag == "code" and attributes.get("data-validate-command"):
            self.command_examples.append(attributes["data-validate-command"])
        if attributes.get("id"):
            identifier = attributes["id"]
            if identifier in self.ids:
                self.errors.append(f"{self.path}: duplicate id {identifier!r}")
            self.ids.add(identifier)
        if tag not in VOID_ELEMENTS:
            self.stack.append(tag)

    def handle_endtag(self, tag):
        if tag in VOID_ELEMENTS:
            self.errors.append(f"{self.path}: void element </{tag}> must not have a closing tag")
            return
        if not self.stack or self.stack[-1] != tag:
            self.errors.append(f"{self.path}: unexpected closing tag </{tag}>")
            return
        self.stack.pop()
        if tag == "a":
            text = self.current_link_text.pop().strip()
            self.links[-1] = (self.links[-1][0], text)
            if not text:
                self.errors.append(f"{self.path}: link must have visible text")
        if tag == "title":
            self.current_title = False

    def handle_data(self, data):
        if self.current_link_text and data.strip():
            self.current_link_text[-1] += data.strip()

    def finish(self):
        self.close()
        if self.stack:
            self.errors.append(f"{self.path}: unclosed tags: {', '.join(self.stack)}")
        if not self.html_lang:
            self.errors.append(f"{self.path}: html must declare a language")
        if self.main_count != 1:
            self.errors.append(f"{self.path}: expected exactly one main landmark")
        if not self.nav_count:
            self.errors.append(f"{self.path}: expected a labeled navigation landmark")
        if self.title_count != 1:
            self.errors.append(f"{self.path}: expected exactly one title")
        if self.h1_count != 1:
            self.errors.append(f"{self.path}: expected exactly one h1")


def check_external_link(href):
    if href in EXTERNAL_LINKS:
        return EXTERNAL_LINKS[href]
    original = href
    parsed = urlparse(href)
    token = os.environ.get("GH_TOKEN")
    is_github = parsed.hostname == "github.com"
    if is_github:
        if not token:
            EXTERNAL_LINKS[href] = None
            return None
        parts = [unquote(part) for part in parsed.path.strip("/").split("/")]
        if len(parts) < 2:
            EXTERNAL_LINKS[href] = "GitHub link must identify a repository"
            return EXTERNAL_LINKS[href]
        api_path = f"/repos/{quote(parts[0])}/{quote(parts[1])}"
        if len(parts) >= 4 and parts[2] == "blob":
            api_path += f"/contents/{quote('/'.join(parts[4:]), safe='/')}"
            api_path += "?" + urlencode({"ref": parts[3]})
        elif len(parts) >= 3 and parts[2] == "releases":
            api_path += "/releases?per_page=1"
        href = f"https://api.github.com{api_path}"
        headers = {
            "Accept": "application/vnd.github+json",
            "Authorization": "Bearer " + token,
            "User-Agent": "TASC-docs-link-check",
            "X-GitHub-Api-Version": "2022-11-28",
        }
        request = urllib.request.Request(href, headers=headers)
    else:
        request = urllib.request.Request(
            href,
            method="HEAD",
            headers={"User-Agent": "TASC-docs-link-check"},
        )
    try:
        with urllib.request.urlopen(request, timeout=15) as response:
            if response.status >= 400:
                error = f"HTTP {response.status}"
            else:
                error = None
    except urllib.error.HTTPError as response:
        if is_github or response.code not in (403, 405, 501):
            error = f"HTTP {response.code}"
        else:
            request = urllib.request.Request(
                href,
                headers={"Range": "bytes=0-0", "User-Agent": "TASC-docs-link-check"},
            )
            try:
                with urllib.request.urlopen(request, timeout=15) as response:
                    error = f"HTTP {response.status}" if response.status >= 400 else None
            except (urllib.error.URLError, TimeoutError) as failure:
                error = str(failure)
    except (urllib.error.URLError, TimeoutError) as failure:
        error = str(failure)
    EXTERNAL_LINKS[original] = error
    return error


def check_links(page, parser, check_external):
    errors = []
    for href, _ in parser.links:
        parsed = urlparse(href)
        if parsed.scheme in ("http", "https"):
            if not parsed.netloc:
                errors.append(f"{page}: invalid external link {href!r}")
            elif parsed.hostname == "pascalvanderheiden.github.io":
                site_prefix = "/turbo-agent-skills-cli"
                if not parsed.path.startswith(site_prefix):
                    errors.append(f"{page}: invalid Pages link {href!r}")
                    continue
                path_part = unquote(parsed.path[len(site_prefix) :]).lstrip("/")
                target = SITE / path_part if path_part else SITE / "index.html"
                if not target.exists():
                    errors.append(f"{page}: broken Pages link target {href!r}")
                    continue
                if parsed.fragment:
                    target_page = target if target.is_file() else target / "index.html"
                    if not target_page.is_file():
                        errors.append(f"{page}: Pages fragment target has no HTML page: {href!r}")
                        continue
                    linked_parser = parse_page(target_page)
                    if parsed.fragment not in linked_parser.ids:
                        errors.append(f"{page}: broken Pages fragment target {href!r}")
            elif check_external:
                error = check_external_link(href)
                if error:
                    errors.append(f"{page}: unreachable external link {href!r}: {error}")
            continue
        if parsed.scheme or href.startswith("mailto:"):
            continue
        if not href:
            errors.append(f"{page}: empty link target")
            continue
        path_part = unquote(parsed.path)
        target = SITE / path_part.lstrip("/") if path_part.startswith("/") else page.parent / path_part
        if path_part and not target.exists():
            errors.append(f"{page}: broken link target {href!r}")
            continue
        if parsed.fragment:
            target_page = target if target.is_file() else target / "index.html"
            if not target_page.is_file():
                errors.append(f"{page}: fragment target has no HTML page: {href!r}")
                continue
            linked_parser = parse_page(target_page)
            if parsed.fragment not in linked_parser.ids:
                errors.append(f"{page}: broken fragment target {href!r}")
    return errors


def parse_page(path):
    parser = DocumentParser(path.relative_to(SITE))
    parser.feed(path.read_text(encoding="utf-8"))
    parser.finish()
    return parser


def validate(version, binary, check_external):
    errors = []
    metadata_path = SITE / "versions.json"
    try:
        metadata = json.loads(metadata_path.read_text(encoding="utf-8"))
        current_version = metadata["currentVersion"]
    except (OSError, json.JSONDecodeError, KeyError, TypeError) as error:
        return [f"invalid version metadata: {error}"]
    if not re.fullmatch(r"(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)", current_version):
        errors.append(f"invalid currentVersion in {metadata_path.relative_to(SITE)}")
    if version and current_version != version:
        errors.append(f"docs version {current_version} does not match release version {version}")

    manifest = SITE.parent.parent / "crates" / "tasc-cli" / "Cargo.toml"
    match = re.search(r'^version\s*=\s*"([^"]+)"', manifest.read_text(encoding="utf-8"), re.MULTILINE)
    if not match:
        errors.append(f"could not read package version from {manifest}")
    elif current_version != match.group(1):
        errors.append(f"docs version {current_version} is stale; tasc package is {match.group(1)}")

    for relative_path, pattern in (
        ("install.sh", r'^VERSION="([^"]+)"'),
        ("install.ps1", r'^\$Version = "([^"]+)"'),
    ):
        installer = SITE.parent.parent / relative_path
        installer_text = installer.read_text(encoding="utf-8")
        installer_match = re.search(pattern, installer_text, re.MULTILINE)
        if not installer_match:
            errors.append(f"could not read installer version from {relative_path}")
        elif current_version != installer_match.group(1):
            errors.append(
                f"{relative_path} version {installer_match.group(1)} does not match docs "
                f"version {current_version}"
            )

    version_page = SITE / f"v{current_version}" / "index.html"
    if not version_page.is_file():
        errors.append(f"versioned documentation page is missing: {version_page.relative_to(SITE)}")
    else:
        text = version_page.read_text(encoding="utf-8")
        if f'name="tasc-docs-version" content="{current_version}"' not in text:
            errors.append(f"version page metadata does not match {current_version}")
        for label, (section_id, term) in REQUIRED_TOPICS.items():
            if f'id="{section_id}"' not in text or term.casefold() not in text.casefold():
                errors.append(f"versioned docs are missing the {label} section")

    root_page = SITE / "index.html"
    if root_page.is_file():
        root_text = root_page.read_text(encoding="utf-8")
        if 'href="docs/index.html"' not in root_text:
            errors.append("landing page does not link to the stable documentation index")
    else:
        errors.append("documentation landing page is missing")

    docs_index = SITE / "docs" / "index.html"
    if not docs_index.is_file():
        errors.append("stable documentation index is missing")
    elif f'href="../v{current_version}/index.html"' not in docs_index.read_text(encoding="utf-8"):
        errors.append(f"stable documentation index does not link to version {current_version}")

    pages = sorted(SITE.rglob("*.html"))
    parsed_pages = {}
    for page in pages:
        parser = parse_page(page)
        parsed_pages[page] = parser
        errors.extend(parser.errors)
        errors.extend(check_links(page, parser, check_external))

    if binary:
        for page, parser in parsed_pages.items():
            for commands in parser.command_examples:
                for command in commands.split(";"):
                    command = command.strip()
                    if not command:
                        continue
                    args = shlex.split(command)
                    result = subprocess.run(
                        [binary, *args, "--help"],
                        capture_output=True,
                        text=True,
                        check=False,
                    )
                    if result.returncode:
                        errors.append(
                            f"{page.relative_to(SITE)}: invalid CLI example {command!r}: "
                            f"{result.stderr.strip()}"
                        )
    return errors


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--version")
    parser.add_argument("--binary")
    parser.add_argument("--check-external", action="store_true")
    args = parser.parse_args()
    errors = validate(args.version, args.binary, args.check_external)
    if errors:
        print("\n".join(errors), file=sys.stderr)
        return 1
    print("Documentation validation passed.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
