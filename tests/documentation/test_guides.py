"""Cheap source-backed guide checks, not a substitute for compiled CLI acceptance."""
from pathlib import Path
import re
import unittest

ROOT = Path(__file__).resolve().parents[2]
DOCS = [ROOT / 'README.md', *(ROOT / 'docs').rglob('*.md')]
GUIDES = [ROOT / 'docs' / 'README.md', *(ROOT / 'docs' / 'guides').glob('*.md'), *(ROOT / 'docs' / 'developers').glob('*.md')]


def variants(enum_name, source):
    match = re.search(r'pub enum ' + re.escape(enum_name) + r'\s*\{', source)
    if not match:
        raise AssertionError(f'missing enum {enum_name}')
    text = source[match.end():]
    depth, names = 1, []
    for line in text.splitlines():
        if depth == 1:
            found = re.match(r'^    ([A-Z][A-Za-z0-9_]*)\s*(?:\{|\(|,)', line)
            if found:
                names.append(re.sub(r'(?<!^)(?=[A-Z])', '-', found.group(1)).lower())
        depth += line.count('{') - line.count('}')
        if depth <= 0: break
    return set(names)



class DocumentationIntegrity(unittest.TestCase):
    def test_user_guide_relative_links(self):
        missing = []
        for doc in GUIDES:
            for target in re.findall(r'\]\(([^)]+)\)', doc.read_text()):
                if target.startswith(('https:', 'http:', 'mailto:', '#')): continue
                if not (doc.parent / target.split('#')[0]).exists():
                    missing.append((str(doc.relative_to(ROOT)), target))
        self.assertFalse(missing, missing)

    def test_user_guides_only_use_supported_commands(self):
        cli = (ROOT / 'crates/orbit-cli/src/commands.rs').read_text()
        blueprint = (ROOT / 'crates/orbit-cli/src/blueprint_commands.rs').read_text()
        commands = {'project': variants('ProjectSubcommand', cli),
                    'task': variants('TaskSubcommand', cli),
                    'agent': variants('AgentSubcommand', cli),
                    'blueprint': variants('BlueprintSubcommand', blueprint)}
        bad = []
        for doc in GUIDES:
            for group, subcommand in re.findall(r'\borbit (project|task|agent|blueprint) ([a-z][a-z-]*)', doc.read_text()):
                if subcommand not in commands[group]:
                    bad.append((doc.name, group, subcommand))
        self.assertFalse(bad, bad)

    def test_index_links_core_workflows(self):
        index = (ROOT / 'docs/README.md').read_text()
        for topic in ('projects-and-planning', 'agent-review-and-resumption', 'blueprints-vs-tasks', 'extensions-and-capabilities'):
            self.assertIn(topic, index)


if __name__ == '__main__': unittest.main()
