import os
import unittest
from unittest.mock import patch

import validate


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
