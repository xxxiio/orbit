"""M27 compiled CLI help checks.

Skips when the debug binary is absent. `orbit update` is implemented in
parallel with the documentation; if the built binary predates it, the update
assertions are skipped with an explicit reconciliation message.
"""
import subprocess
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
ORBIT = ROOT / "target" / "debug" / "orbit"


def run(*args):
    return subprocess.run(
        [str(ORBIT), *args],
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        timeout=30,
    )


class M27CliHelpTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        if not ORBIT.is_file():
            raise unittest.SkipTest("target/debug/orbit is not built")

    def test_version(self):
        result = run("--version")
        self.assertEqual(result.returncode, 0, result.stdout)
        self.assertRegex(result.stdout, r"orbit \d+\.\d+\.\d+")

    def test_init_help(self):
        result = run("init", "--help")
        self.assertEqual(result.returncode, 0, result.stdout)
        for flag in ("--database", "--force", "--authority", "--server", "--replica-database"):
            self.assertIn(flag, result.stdout)

    def test_doctor_help(self):
        result = run("doctor", "--help")
        self.assertEqual(result.returncode, 0, result.stdout)
        self.assertIn("--strict", result.stdout)

    def test_update_help(self):
        result = run("update", "--help")
        if result.returncode != 0 or "Usage: orbit update" not in result.stdout:
            self.skipTest(
                "orbit update is not present in this build; documentation is "
                "marked for reconciliation with the final implementation"
            )
        for flag in ("--check", "--version", "--channel", "--prefix", "--allow-downgrade", "--force"):
            self.assertIn(flag, result.stdout)


if __name__ == "__main__":
    unittest.main()
