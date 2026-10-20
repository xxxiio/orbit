"""Source-backed acceptance for MCP permissions and upgrade documentation."""
import re
import unittest
from pathlib import Path
ROOT = Path(__file__).resolve().parents[2]
DOCS = ROOT / 'docs'
SOURCE = (ROOT / 'crates/orbit-mcp/src/main.rs').read_text()
class McpAndUpgradeDocs(unittest.TestCase):
    def test_documented_mcp_registrations_exist(self):
        text = (DOCS / 'reference/mcp.md').read_text()
        names = set(re.findall(r'`([a-z][a-z_]*\.[a-z_]+)`', text))
        # Only the explicit registration inventory is a completeness contract.
        for group in ('Connected registry', 'Local registry'):
            self.assertIn(group, text)
        registered = set(re.findall(r'tool\(\s*"([\w.]+)"', SOURCE))
        self.assertGreater(len(names & registered), 20)
        for n in names - {'sqlite.backup'}:
            self.assertIn(n, registered, n)
    def test_scopes_and_authority_documented(self):
        text = (DOCS/'reference/mcp.md').read_text()
        for x in ('ORBIT_MCP_MODE','ORBIT_MCP_PROJECTS','ORBIT_DEVICE_ID','ORBIT_DEVICE_TOKEN','read_only','authority.flush'):
            self.assertIn(x,text)
    def test_upgrade_boundaries_documented(self):
        text = (DOCS/'releasing/upgrades-and-recovery.md').read_text()
        for x in ('SQLite','outbox','server SQLite','schema','backup','M15'):
            self.assertIn(x,text)
    def test_index_references_exist(self):
        text=(DOCS/'README.md').read_text()
        for name in ('reference/mcp.md','developers/capability-provider-example.md','releasing/upgrades-and-recovery.md'):
            self.assertIn(name,text)
            self.assertTrue((DOCS/name).is_file())
if __name__ == '__main__': unittest.main()
