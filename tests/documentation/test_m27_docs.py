"""M27 end-user documentation contract checks."""
import re
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
DOCS = ROOT / "docs"

# Current user-facing documentation. Historical/milestone files are excluded.
CURRENT_DOCS = [
    ROOT / "README.md",
    DOCS / "README.md",
    DOCS / "cli-contract.md",
    DOCS / "server-api.md",
    *(DOCS / "getting-started").glob("*.md"),
    *(DOCS / "guides").glob("*.md"),
    *(DOCS / "reference").glob("*.md"),
    *(DOCS / "releasing").glob("*.md"),
    *(DOCS / "developers").glob("*.md"),
    *(DOCS / "contributing").glob("*.md"),
]


def text(path):
    return path.read_text()


class M27DocumentationTests(unittest.TestCase):
    def test_navigation_links_resolve(self):
        index = text(DOCS / "README.md")
        for href in re.findall(r"\]\(([^)]+)\)", index):
            if href.startswith(("https:", "http:", "#", "mailto:")):
                continue
            target = (DOCS / href.split("#")[0]).resolve()
            self.assertTrue(target.exists(), f"docs/README.md: broken link {href}")

        for page in CURRENT_DOCS:
            for href in re.findall(r"\]\(([^)]+)\)", text(page)):
                if href.startswith(("https:", "http:", "#", "mailto:")):
                    continue
                target = (page.parent / href.split("#")[0]).resolve()
                self.assertTrue(target.exists(), f"{page.relative_to(ROOT)}: broken link {href}")

    def test_installation_entrypoints(self):
        install = text(DOCS / "getting-started" / "installation.md")
        self.assertIn(
            "curl -fsSL https://github.com/xxxiio/orbit/releases/latest/download/install.sh | sh",
            install,
        )
        for term in ("--version", "--prefix", "checksum", "orbit update", "orbit init", "orbit doctor"):
            self.assertIn(term, install)
        self.assertIn("not publisher authenticity", install)

    def test_configuration_reference_keys_and_precedence(self):
        config = text(DOCS / "reference" / "configuration.md")
        for term in (
            "version = 1",
            "[storage]",
            "[mode]",
            "[connected]",
            "[agent]",
            "authority",
            "database",
            "replica_database",
            "default_provider",
        ):
            self.assertIn(term, config)
        for term in ("--db", "ORBIT_DB", "[storage].database", "~/.orbit/orbit.db", "ORBIT_CONFIG"):
            self.assertIn(term, config)

    def test_everyday_feature_pages_exist(self):
        pages = (
            "projects-and-planning.md",
            "habits-routines-schedules.md",
            "notes-and-folders.md",
            "search-and-navigation.md",
            "backup-and-recovery.md",
            "switching-projects-and-workspaces.md",
            "development-projects.md",
            "learning-projects.md",
        )
        for name in pages:
            path = DOCS / "guides" / name
            self.assertTrue(path.is_file(), name)
            self.assertGreater(len(text(path)), 500, name)

    def test_no_fictional_commands_as_working_commands(self):
        forbidden = (
            "orbit knowledge",
            "orbit sync init",
            "orbit sync http",
            "orbit project switch",
            "orbit milestone",
            "orbit priority",
            "orbit backup",
        )
        for page in CURRENT_DOCS:
            body = text(page)
            for command in forbidden:
                self.assertNotIn(
                    command,
                    body,
                    f"{page.relative_to(ROOT)} presents fictional command {command!r}",
                )

    def test_no_unverified_macos_or_release_claims(self):
        combined = "\n".join(text(page) for page in CURRENT_DOCS)
        # Unsupported commands are named in the limitations section instead.
        self.assertIn("not runtime-verified", combined)
        self.assertIn("not verified", combined)
        self.assertNotRegex(
            combined,
            r"(?i)macOS[^.\n]{0,80}\b(?:is|was|has been)\s+(?:\*\*)?(?:runtime-)?verified\b",
        )
        self.assertNotRegex(combined, r"(?i)verified on macOS")
        self.assertNotRegex(combined, r"(?i)macOS (?:is )?supported")
        # Any mention of a real published release must be explicitly negated.
        for match in re.finditer(r"(?i)real public GitHub Release has been published", combined):
            prefix = combined[max(0, match.start() - 80):match.start()]
            self.assertRegex(prefix, r"(?i)(does not claim|no claim|never)")
        self.assertIn("draft", combined.lower())
        self.assertIn("manual authorization", combined.lower())


if __name__ == "__main__":
    unittest.main()
