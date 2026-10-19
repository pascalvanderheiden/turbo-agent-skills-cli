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

    def test_markdown_images_rewrite_site_assets_and_escape_alt_text(self):
        content = render_markdown(
            '![Session "preview" & <events>](docs/site/assets/session.png)'
        )
        self.assertEqual(
            content,
            '<p><img class="product-shot" src="assets/session.png" '
            'alt="Session &quot;preview&quot; &amp; &lt;events&gt;"></p>',
        )

    def test_markdown_images_preserve_other_paths_and_escape_sources(self):
        content = render_markdown('![Session](assets/session.png?label="preview"&v=1)')
        self.assertIn('src="assets/session.png?label=&quot;preview&quot;&amp;v=1"', content)

    def test_markdown_links_and_images_reject_unsafe_urls(self):
        for prefix in ("", "!"):
            for href in (
                "javascript:alert",
                "data:image/png;base64,abc",
                "file:///tmp/session.png",
                "//example.com/session.png",
                " //example.com/session.png",
                "\\\\example.com/session.png",
                "/\\example.com/session.png",
                "java\tscript:alert",
            ):
                with self.subTest(prefix=prefix, href=href):
                    with self.assertRaises(ValueError):
                        render_markdown(f"{prefix}[Session]({href})")

    def test_inline_code_does_not_render_images(self):
        self.assertEqual(
            render_markdown("`![Session](docs/site/assets/session.png)`"),
            "<p><code>![Session](docs/site/assets/session.png)</code></p>",
        )

    def test_markdown_links_still_render_without_image_path_rewriting(self):
        self.assertEqual(
            render_markdown("[Asset](docs/site/assets/session.png)"),
            '<p><a href="docs/site/assets/session.png">Asset</a></p>',
        )

    def test_product_shots_are_responsive_and_bordered(self):
        template = (SITE / "index.html").read_text(encoding="utf-8")
        self.assertRegex(
            template,
            r"\.product-shot\s*\{[^}]*max-width: 100%;[^}]*height: auto;[^}]*border: 1px solid",
        )

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
