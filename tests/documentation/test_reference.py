"""Documentation link and CLI surface checks, independent from compiled M15 tests."""
import re
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
DOCS = ROOT / 'docs'
SOURCE = (ROOT / 'crates/orbit-cli/src/commands.rs').read_text()
BLUEPRINT = (ROOT / 'crates/orbit-cli/src/blueprint_commands.rs').read_text()


def enum_variants(source, enum):
    m = re.search(r'pub enum ' + re.escape(enum) + r'\s*\{', source)
    if m is None:
        raise AssertionError('Missing CLI enum: ' + enum)
    depth = 1
    result = set()
    for line in source[m.end():].splitlines():
        if depth == 1:
            v = re.match(r'\s{4}([A-Z][A-Za-z0-9_]*)\s*(?:\{|\(|,)', line)
            if v:
                result.add(re.sub(r'(?<!^)(?=[A-Z])', '-', v.group(1)).lower())
        depth += line.count('{') - line.count('}')
        if depth <= 0:
            break
    return result


class ReferenceAcceptance(unittest.TestCase):
    def test_reference_command_inventory(self):
        reference = (DOCS / 'reference/cli.md').read_text()
        pairs = re.findall(r'`orbit (project|task|blueprint|agent|sync) ([a-z][a-z-]*)`', reference)
        self.assertGreaterEqual(len(pairs), 40, 'reference must cover substantial CLI surface')
        names = {
            'project': enum_variants(SOURCE, 'ProjectSubcommand'),
            'task': enum_variants(SOURCE, 'TaskSubcommand'),
            'agent': enum_variants(SOURCE, 'AgentSubcommand'),
            'sync': enum_variants(SOURCE, 'SyncSubcommand'),
            'blueprint': enum_variants(BLUEPRINT, 'BlueprintSubcommand'),
        }
        missing = [(group, name) for group, name in pairs if name not in names[group]]
        self.assertFalse(missing, f'Unknown CLI commands: {missing}')

    def test_onboarding_entrypoints_and_links(self):
        index = (DOCS / 'README.md').read_text()
        for path in ('getting-started/quickstart.md', 'getting-started/connected-mode.md',
                     'guides/research-workflow.md', 'reference/cli.md'):
            self.assertIn(path, index)
        for page in (DOCS / 'getting-started').glob('*.md'):
            for href in re.findall(r'\]\(([^)]+)\)', page.read_text()):
                if href.startswith(('https:', 'http:', '#', 'mailto:')):
                    continue
                self.assertTrue((page.parent / href.split('#')[0]).exists(), f'{page}: broken link {href}')

    def test_avoid_unverified_install_claims(self):
        docs = [(DOCS / p).read_text() for p in ('getting-started/quickstart.md', 'getting-started/connected-mode.md')]
        self.assertIn('server', docs[1].lower())
        self.assertIn('not accepted', docs[1].lower())
        self.assertIn('not', docs[0].lower())


if __name__ == '__main__':
    unittest.main()
