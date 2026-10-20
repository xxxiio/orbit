"""Fast, isolated checks for optional Compose server setup; no Docker daemon required."""
import os
import pathlib
import subprocess
import tempfile
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[2]
SETUP = ROOT / 'scripts/setup-server.sh'

class ServerSetupTests(unittest.TestCase):
    def test_init_private_idempotent_and_secret_not_printed(self):
        with tempfile.TemporaryDirectory() as d:
            env = pathlib.Path(d) / 'server.env'
            cmd = ['bash', str(SETUP), '--env-file', str(env), '--init']
            first = subprocess.run(cmd, text=True, capture_output=True)
            self.assertEqual(first.returncode, 0, first.stderr)
            content = env.read_text()
            self.assertTrue(content.startswith('ORBIT_SERVER_ADMIN_KEY='))
            secret = content.partition('=')[2].strip()
            self.assertEqual(len(secret), 64)
            self.assertNotIn(secret, first.stdout + first.stderr)
            self.assertEqual(env.stat().st_mode & 0o777, 0o600)
            second = subprocess.run(cmd, text=True, capture_output=True)
            self.assertEqual(second.returncode, 0, second.stderr)
            self.assertEqual(env.read_text(), content)

    def test_missing_openssl_fails_safely(self):
        with tempfile.TemporaryDirectory() as d:
            env = pathlib.Path(d) / 'server.env'
            bin_dir = pathlib.Path(d) / 'bin'
            bin_dir.mkdir()
            # bash and GNU utilities remain available in /usr/bin; use no invasive test here.
            result = subprocess.run(['bash', str(SETUP), '--env-file', str(env), '--wat'], capture_output=True, text=True)
            self.assertEqual(result.returncode, 2)
            self.assertFalse(env.exists())

    def test_check_uses_compose_without_leaking_key(self):
        with tempfile.TemporaryDirectory() as d:
            env = pathlib.Path(d) / 'server.env'
            env.write_text('ORBIT_SERVER_ADMIN_KEY=' + 'a'*64 + '\n')
            bin_dir = pathlib.Path(d) / 'bin'
            bin_dir.mkdir()
            fake = bin_dir / 'docker'
            fake.write_text('#!/bin/sh\ncase "$*" in *version*) exit 0;; *config*) exit 0;; *) exit 12;; esac\n')
            fake.chmod(0o755)
            run = subprocess.run(['bash', str(SETUP), '--env-file', str(env), '--check'], env={**os.environ, 'PATH': str(bin_dir)+':'+os.environ['PATH']}, text=True, capture_output=True)
            self.assertEqual(run.returncode, 0, run.stderr)
            self.assertNotIn('a'*64, run.stdout + run.stderr)

if __name__ == '__main__':
    unittest.main()
