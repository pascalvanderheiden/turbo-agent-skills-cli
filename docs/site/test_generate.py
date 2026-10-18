import unittest
from pathlib import Path

from generate import END, START, generated_content, render_markdown, render_site


SITE = Path(__file__).parent


class LandingPageTests(unittest.TestCase):
    def test_landing_content_is_rendered_from_readme(self):
        content = generated_content()
        self.assertIn("Turbo Agent Skills CLI", content)
        self.assertIn("tasc setup", content)
        self.assertIn("tasc discover", content)
        self.assertIn("v0.1.0/index.html", content)

    def test_markdown_rendering_escapes_html_and_assigns_unique_headings(self):
        content = render_markdown("# Hello\n\n# Hello\n\n<script>alert(1)</script>")
        self.assertIn('id="hello-2"', content)
        self.assertNotIn("<script>", content)
        self.assertIn("&lt;script&gt;", content)

    def test_site_template_generation_is_deterministic(self):
        template = (SITE / "index.html").read_text(encoding="utf-8")
        content = generated_content()
        first = render_site(template, content)
        self.assertEqual(first, render_site(first, content))
        self.assertIn(START, first)
        self.assertIn(END, first)

    def test_ascii_banner_is_shared_by_readme_and_landing_page(self):
        readme = SITE.parents[1] / "README.md"
        template = (SITE / "index.html").read_text(encoding="utf-8")
        banner = (SITE / "assets" / "tasc-speed-banner.svg").read_text(encoding="utf-8")

        self.assertIn("docs/site/assets/tasc-speed-banner.svg", readme.read_text(encoding="utf-8"))
        self.assertIn('src="assets/tasc-speed-banner.svg"', template)
        self.assertIn("TASC speed ASCII art banner", banner)
        self.assertIn("______________________________", banner)
        self.assertNotIn("\U0001f680", banner)


if __name__ == "__main__":
    unittest.main()
