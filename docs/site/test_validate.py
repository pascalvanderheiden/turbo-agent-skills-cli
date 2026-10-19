import os
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

import validate
from generate import render_markdown


class ImageTests(unittest.TestCase):
    def test_images_require_nonempty_alt_text(self):
        for attributes in ('', 'alt=""', 'alt="   "'):
            with self.subTest(attributes=attributes):
                parser = validate.DocumentParser("index.html")
                parser.feed(f'<img src="assets/session.png" {attributes}>')
                self.assertTrue(any("missing alt text" in error for error in parser.errors))

    def test_image_with_alt_text_is_accessible(self):
        parser = validate.DocumentParser("index.html")
        parser.feed('<img src="assets/session.png" alt="Agent Session">')
        self.assertEqual(parser.errors, [])

    def test_images_require_an_existing_local_file(self):
        with tempfile.TemporaryDirectory() as directory:
            site = Path(directory)
            (site / "assets").mkdir()
            (site / "assets" / "session.png").touch()
            cases = (
                ("assets/session.png", True),
                ("assets/session.png?v=1", True),
                ("/assets/session.png", True),
                ("assets/missing.png", False),
                ("assets", False),
                ("", False),
                ("https://example.com/session.png", False),
                ("//example.com/session.png", False),
                ("data:image/png;base64,abc", False),
                ("../session.png", False),
            )
            with patch.object(validate, "SITE", site):
                for src, valid in cases:
                    with self.subTest(src=src):
                        parser = validate.DocumentParser("index.html")
                        parser.feed(f'<img src="{src}" alt="Agent Session">')
                        errors = validate.check_images(site / "index.html", parser)
                        self.assertEqual(not errors, valid)

    def test_generated_images_are_validated(self):
        with tempfile.TemporaryDirectory() as directory:
            site = Path(directory)
            page = site / "index.html"
            page.write_text(
                render_markdown("![](docs/site/assets/missing.png)"), encoding="utf-8"
            )
            with patch.object(validate, "SITE", site):
                parser = validate.parse_page(page)
                self.assertTrue(any("missing alt text" in error for error in parser.errors))
                errors = validate.check_images(page, parser)
                self.assertTrue(any("missing local image source file" in error for error in errors))

    def test_images_are_resolved_relative_to_their_page(self):
        with tempfile.TemporaryDirectory() as directory:
            site = Path(directory)
            (site / "assets").mkdir()
            (site / "assets" / "session preview.png").touch()
            (site / "docs").mkdir()
            page = site / "docs" / "index.html"
            parser = validate.DocumentParser("docs/index.html")
            parser.feed('<img src="../assets/session%20preview.png" alt="Agent Session">')
            with patch.object(validate, "SITE", site):
                self.assertEqual(validate.check_images(page, parser), [])


class ExternalLinkTests(unittest.TestCase):
    def test_release_tag_links_check_the_exact_github_release(self):
        validate.EXTERNAL_LINKS.clear()
        with (
            patch.dict(os.environ, {"GH_TOKEN": "x"}),
            patch.object(validate.urllib.request, "urlopen") as urlopen,
        ):
            urlopen.return_value.__enter__.return_value.status = 200
            error = validate.check_external_link(
                "https://github.com/pascalvanderheiden/turbo-agent-skills-cli/releases/tag/v0.1.0"
            )

        self.assertIsNone(error)
        request = urlopen.call_args.args[0]
        self.assertEqual(
            request.full_url,
            "https://api.github.com/repos/pascalvanderheiden/turbo-agent-skills-cli/releases/tags/v0.1.0",
        )


if __name__ == "__main__":
    unittest.main()
