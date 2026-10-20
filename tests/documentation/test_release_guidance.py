"""Release guidance must agree with the maintained install and packaging interfaces."""
import re
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
DOCS = ROOT / 'docs'


class ReleaseGuidanceTests(unittest.TestCase):
    def test_release_pages_linked_from_navigation(self):
        docs_index = (DOCS / 'README.md').read_text()
        root_index = (ROOT / 'README.md').read_text()
        for name in ('platforms-and-components.md', 'release-checklist.md', 'binary-distribution.md'):
            self.assertTrue((DOCS / 'releasing' / name).is_file())
            self.assertIn(name, docs_index)
            self.assertIn(name, root_index)

    def test_documented_release_commands_exist(self):
        package = (ROOT / 'scripts/package-release.sh').read_text()
        verify = (ROOT / 'scripts/verify-release.sh').read_text()
        install = (ROOT / 'scripts/install.sh').read_text()
        env = (ROOT / 'scripts/orbit-env.sh').read_text()
        guidance = (DOCS / 'releasing/release-checklist.md').read_text()
        self.assertIn('--build', package)
        self.assertIn('--from-archive', install)
        self.assertIn('--check', env)
        for name in ('package-release.sh', 'verify-release.sh', 'install.sh', 'orbit-env.sh'):
            self.assertIn(name, guidance)
        self.assertIn('SHA256SUMS', verify)

    def test_no_stale_archive_installation_warning(self):
        dist = (DOCS / 'releasing/binary-distribution.md').read_text()
        self.assertNotIn('until that installer supports release archive layouts', dist)
        self.assertIn('install.sh --from-archive', dist)
        self.assertIn('not publisher authenticity', dist)

    def test_limits_and_optional_components_explicit(self):
        guide = (DOCS / 'releasing/platforms-and-components.md').read_text()
        for term in ('Linux', 'macOS', 'Windows', 'Python', 'Herdr', 'M15', 'cli', 'mcp', 'tui', 'server'):
            self.assertIn(term, guide)
        self.assertRegex(guide, r'not.*native Windows installer')
        self.assertIn('not architecture-probed', guide)


if __name__ == '__main__':
    unittest.main()
