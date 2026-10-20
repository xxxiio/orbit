#!/usr/bin/env python3
"""M27 acceptance: safe `orbit update` against a local release fixture.

Usage: python3 tests/acceptance/m27_update.py --binary target/debug/orbit

Archives are built with the repository's own scripts/package-release.sh using
stub executables. A loopback HTTP server exposes a GitHub-shaped releases API
and the archive/checksum assets. Every invocation uses isolated HOME,
ORBIT_CONFIG and TMPDIR values, and ORBIT_GITHUB_* point at the fixture.
"""
from __future__ import annotations

import argparse
import hashlib
import http.server
import json
import os
from pathlib import Path
import shutil
import sqlite3
import subprocess
import tempfile
import threading
import unittest

ROOT = Path(__file__).resolve().parents[2]
TARGET = 'x86_64-unknown-linux-gnu'
COMPONENTS = ('orbit', 'orbit-mcp', 'orbit-tui', 'orbit-server')
CURRENT_VERSION = '0.1.0'
STABLE_VERSION = '0.2.0'
PRERELEASE_VERSION = '0.3.0-beta.1'
DOWNGRADE_VERSION = '0.0.9'
BAD_CHECKSUM_VERSION = '0.2.1'
TRUNCATED_VERSION = '0.2.2'


class FixtureServer:
    """Minimal GitHub releases API and asset server."""

    def __init__(self, releases: list[dict], tags: dict[str, dict], assets: dict[str, bytes]) -> None:
        self.releases = releases
        self.tags = tags
        self.assets = assets
        outer = self

        class Handler(http.server.BaseHTTPRequestHandler):
            def log_message(self, *args: object) -> None:  # noqa: D401 - silence test server
                pass

            def _send(self, status: int, payload: bytes, content_type: str) -> None:
                self.send_response(status)
                self.send_header('Content-Type', content_type)
                self.send_header('Content-Length', str(len(payload)))
                self.end_headers()
                self.wfile.write(payload)

            def _json(self, value: object) -> None:
                self._send(200, json.dumps(value).encode('utf-8'), 'application/json')

            def do_GET(self) -> None:  # noqa: N802 - stdlib handler API
                path = self.path.split('?', 1)[0]
                if path == '/releases':
                    self._json(outer.releases)
                    return
                if path.startswith('/releases/tags/'):
                    tag = path[len('/releases/tags/'):]
                    release = outer.tags.get(tag) or outer.tags.get(tag.lstrip('v'))
                    if release is None:
                        self._send(404, b'{"message":"Not Found"}', 'application/json')
                    else:
                        self._json(release)
                    return
                if path.startswith('/download/'):
                    name = path.rsplit('/', 1)[-1]
                    payload = outer.assets.get(name)
                    if payload is None:
                        self._send(404, b'missing asset', 'text/plain')
                    else:
                        self._send(200, payload, 'application/octet-stream')
                    return
                self._send(404, b'not found', 'text/plain')

        self.httpd = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Handler)
        self.thread = threading.Thread(target=self.httpd.serve_forever, daemon=True)

    @property
    def base(self) -> str:
        host, port = self.httpd.server_address[:2]
        return f'http://{host}:{port}'

    def start(self) -> None:
        self.thread.start()

    def stop(self) -> None:
        self.httpd.shutdown()
        self.httpd.server_close()
        self.thread.join(timeout=5)


def build_archive(workdir: Path, version: str) -> tuple[bytes, str]:
    """Create a real release archive through scripts/package-release.sh."""
    binary_dir = workdir / f'bin-{version}'
    binary_dir.mkdir()
    for name in COMPONENTS:
        report = f'orbit {version}' if name == 'orbit' else f'{name} {version}'
        executable = binary_dir / name
        executable.write_text(f'#!/bin/sh\necho "{report}"\n', encoding='utf-8')
        executable.chmod(0o755)
    output = workdir / f'dist-{version}'
    output.mkdir()
    completed = subprocess.run(
        [
            str(ROOT / 'scripts' / 'package-release.sh'),
            '--from-dir', str(binary_dir),
            '--output', str(output),
            '--version', version,
            '--target', TARGET,
            '--components', 'cli,mcp,tui,server',
        ],
        text=True,
        capture_output=True,
        timeout=60,
    )
    if completed.returncode != 0:
        raise RuntimeError(
            f'package-release.sh failed for {version}: {completed.stdout}{completed.stderr}'
        )
    archive = output / f'orbit-{version}-{TARGET}.tar.gz'
    checksum = output / f'{archive.name}.sha256'
    return archive.read_bytes(), checksum.read_text(encoding='utf-8')


class UpdateAcceptance(unittest.TestCase):
    binary = Path('target/debug/orbit').resolve()
    server: FixtureServer
    shared_dir: Path

    @classmethod
    def setUpClass(cls) -> None:
        if not cls.binary.is_file():
            raise unittest.SkipTest(f'compiled binary not found: {cls.binary}')
        # The release-evidence runner also runs pytest before `cargo build`; a
        # stale binary without the update command must skip rather than fail.
        probe = subprocess.run(
            [str(cls.binary), 'update', '--help'],
            text=True,
            capture_output=True,
            timeout=30,
        )
        if probe.returncode != 0:
            raise unittest.SkipTest('compiled binary does not support `orbit update`')
        cls.shared_dir = Path(tempfile.mkdtemp(prefix='orbit-m27-update-assets-'))
        archives: dict[str, bytes] = {}
        checksums: dict[str, str] = {}
        for version in (
            DOWNGRADE_VERSION,
            CURRENT_VERSION,
            STABLE_VERSION,
            PRERELEASE_VERSION,
            BAD_CHECKSUM_VERSION,
            TRUNCATED_VERSION,
        ):
            archives[version], checksums[version] = build_archive(cls.shared_dir, version)

        # A corrupt digest for 0.2.1 and a truncated archive for 0.2.2 whose
        # checksum matches the truncated bytes, so each failure is distinct.
        assets: dict[str, bytes] = {}
        for version, payload in archives.items():
            archive_name = f'orbit-{version}-{TARGET}.tar.gz'
            checksum_name = f'{archive_name}.sha256'
            if version == BAD_CHECKSUM_VERSION:
                assets[archive_name] = payload
                assets[checksum_name] = (
                    f'{"0" * 64}  {archive_name}\n'.encode('utf-8')
                )
            elif version == TRUNCATED_VERSION:
                truncated = payload[: max(1, len(payload) // 2)]
                assets[archive_name] = truncated
                digest = hashlib.sha256(truncated).hexdigest()
                assets[checksum_name] = f'{digest}  {archive_name}\n'.encode('utf-8')
            else:
                assets[archive_name] = payload
                assets[checksum_name] = checksums[version].encode('utf-8')

        def release(version: str, prerelease: bool) -> dict:
            archive_name = f'orbit-{version}-{TARGET}.tar.gz'
            return {
                'tag_name': f'v{version}',
                'name': f'v{version}',
                'draft': False,
                'prerelease': prerelease,
                'assets': [
                    {'name': archive_name, 'size': len(assets[archive_name])},
                    {'name': f'{archive_name}.sha256', 'size': len(assets[f'{archive_name}.sha256'])},
                ],
            }

        listed = [
            release(DOWNGRADE_VERSION, False),
            release(CURRENT_VERSION, False),
            release(STABLE_VERSION, False),
            release(PRERELEASE_VERSION, True),
        ]
        tags = {item['tag_name']: item for item in listed}
        # Explicit-version fixtures are reachable by tag but deliberately not
        # part of the channel list, so stable defaults still select 0.2.0.
        for version, prerelease in ((BAD_CHECKSUM_VERSION, False), (TRUNCATED_VERSION, False)):
            item = release(version, prerelease)
            tags[item['tag_name']] = item
        cls.server = FixtureServer(listed, tags, assets)
        cls.server.start()

    @classmethod
    def tearDownClass(cls) -> None:
        cls.server.stop()
        shutil.rmtree(cls.shared_dir, ignore_errors=True)

    def setUp(self) -> None:
        self.root = Path(tempfile.mkdtemp(prefix='orbit-m27-update-'))
        self.home = self.root / 'home'
        self.home.mkdir()
        self.tmp_dir = self.root / 'tmp'
        self.tmp_dir.mkdir()
        self.config_path = self.home / '.orbit' / 'config.toml'
        self.prefix = self.root / 'prefix'
        self.database = self.root / 'data' / 'orbit.db'
        self.install_managed_prefix(self.prefix)
        self.call('init', '--database', str(self.database))

    def tearDown(self) -> None:
        shutil.rmtree(self.root, ignore_errors=True)

    # -- helpers ---------------------------------------------------------

    def environment(self, **overrides: object) -> dict[str, str]:
        ignored = {
            'HTTP_PROXY', 'HTTPS_PROXY', 'ALL_PROXY', 'NO_PROXY',
            'http_proxy', 'https_proxy', 'all_proxy', 'no_proxy',
        }
        env = {
            key: value
            for key, value in os.environ.items()
            if not key.startswith('ORBIT_') and key not in ignored
        }
        env.update(
            {
                'HOME': str(self.home),
                'USERPROFILE': str(self.home),
                'ORBIT_CONFIG': str(self.config_path),
                'ORBIT_GITHUB_API_BASE': self.server.base,
                'ORBIT_GITHUB_DOWNLOAD_BASE': f'{self.server.base}/download',
                'ORBIT_TARGET': TARGET,
                'TMPDIR': str(self.tmp_dir),
                'NO_PROXY': '127.0.0.1,localhost',
                'NO_COLOR': '1',
            }
        )
        for key, value in overrides.items():
            if value is None:
                env.pop(key, None)
            else:
                env[key] = str(value)
        return env

    def call(
        self,
        *arguments: str,
        expect: int = 0,
        env: dict[str, str] | None = None,
    ) -> subprocess.CompletedProcess[str]:
        result = subprocess.run(
            [str(self.binary), *arguments],
            text=True,
            capture_output=True,
            timeout=120,
            env=env or self.environment(),
        )
        if result.returncode != expect:
            self.fail(
                f'command {arguments!r} exited {result.returncode}, expected {expect}\n'
                f'stdout={result.stdout}\nstderr={result.stderr}'
            )
        return result

    def json_call(self, *arguments: str, **kwargs: object) -> dict:
        result = self.call('--json', *arguments, **kwargs)
        text = result.stdout.strip() or result.stderr.strip()
        try:
            parsed = json.loads(text)
        except json.JSONDecodeError as error:
            raise AssertionError(
                f'non-JSON response for {arguments!r}: stdout={result.stdout!r} stderr={result.stderr!r}'
            ) from error
        self.assertIsInstance(parsed, dict)
        return parsed

    def write_config(self, text: str) -> None:
        self.config_path.parent.mkdir(parents=True, exist_ok=True)
        self.config_path.write_text(text, encoding='utf-8')

    def install_managed_prefix(self, prefix: Path) -> None:
        (prefix / 'bin').mkdir(parents=True)
        (prefix / 'share' / 'orbit').mkdir(parents=True)
        old = prefix / 'bin' / 'orbit'
        old.write_text(f'#!/bin/sh\necho "orbit {CURRENT_VERSION}"\n', encoding='utf-8')
        old.chmod(0o755)
        (prefix / 'share' / 'orbit' / 'installed-binaries.txt').write_text(
            'orbit\n', encoding='utf-8'
        )

    def installed_binary(self) -> Path:
        return self.prefix / 'bin' / 'orbit'

    def manifest_text(self) -> str:
        return (self.prefix / 'share/orbit/installed-binaries.txt').read_text(encoding='utf-8')

    def digest(self, path: Path) -> str:
        return hashlib.sha256(path.read_bytes()).hexdigest()

    def backup_files(self) -> list[str]:
        directory = self.home / '.orbit' / 'backups'
        if not directory.is_dir():
            return []
        return sorted(path.name for path in directory.iterdir())

    # -- cases -----------------------------------------------------------

    def test_help_documents_contract(self) -> None:
        help_text = self.call('update', '--help').stdout
        for flag in (
            '--check', '--version', '--channel', '--prefix',
            '--allow-downgrade', '--force',
        ):
            self.assertIn(flag, help_text)

    def test_check_reports_available_without_mutation(self) -> None:
        binary_before = self.digest(self.installed_binary())
        database_before = self.digest(self.database)
        backups_before = self.backup_files()

        report = self.json_call('update', '--check', '--prefix', str(self.prefix))
        self.assertEqual(report['type'], 'update')
        self.assertEqual(report['current'], CURRENT_VERSION)
        self.assertEqual(report['available'], STABLE_VERSION)
        self.assertEqual(report['channel'], 'stable')
        self.assertEqual(report['target'], TARGET)
        self.assertEqual(report['prefix'], str(self.prefix))
        self.assertTrue(report['check'])
        self.assertTrue(report['update_available'])
        self.assertEqual(report['action'], 'checked')
        self.assertIsNone(report['backup_path'])
        self.assertIsNone(report['installed_version'])

        self.assertEqual(self.digest(self.installed_binary()), binary_before)
        self.assertEqual(self.digest(self.database), database_before)
        self.assertEqual(self.backup_files(), backups_before)
        self.assertIn(str(self.prefix), report['prefix'])

    def test_successful_update_backs_up_database_and_preserves_manifest(self) -> None:
        database_before = self.digest(self.database)
        report = self.json_call('update', '--prefix', str(self.prefix))
        self.assertEqual(report['action'], 'installed')
        self.assertEqual(report['installed_version'], STABLE_VERSION)
        self.assertEqual(report['available'], STABLE_VERSION)
        self.assertEqual(report['tag'], f'v{STABLE_VERSION}')
        self.assertTrue(report['update_available'])
        self.assertFalse(report['check'])

        self.assertIn(STABLE_VERSION, self.installed_binary().read_text(encoding='utf-8'))
        self.assertEqual(self.manifest_text(), 'orbit\n')
        self.assertEqual(self.digest(self.database), database_before)

        backup_path = Path(report['backup_path'])
        self.assertTrue(backup_path.is_file())
        self.assertEqual(backup_path.parent, self.home / '.orbit' / 'backups')
        self.assertTrue(backup_path.name.startswith(f'orbit-pre-update-{CURRENT_VERSION}-'))
        self.assertTrue(backup_path.name.endswith('.db'))
        connection = sqlite3.connect(f'file:{backup_path}?mode=ro', uri=True)
        try:
            self.assertEqual(
                connection.execute('PRAGMA integrity_check').fetchone()[0], 'ok'
            )
            self.assertIsNotNone(
                connection.execute('SELECT version FROM orbit_schema WHERE id = 1').fetchone()
            )
        finally:
            connection.close()

    def test_global_db_override_is_the_database_that_gets_backed_up(self) -> None:
        override = self.root / 'override.db'
        self.call('--db', str(override), 'init')
        connection = sqlite3.connect(override)
        try:
            connection.execute('CREATE TABLE update_probe(value TEXT)')
            connection.execute("INSERT INTO update_probe VALUES ('from-override')")
            connection.commit()
        finally:
            connection.close()

        report = self.json_call(
            'update', '--db', str(override), '--version', STABLE_VERSION,
            '--prefix', str(self.prefix),
        )
        backup_path = Path(report['backup_path'])
        connection = sqlite3.connect(f'file:{backup_path}?mode=ro', uri=True)
        try:
            value = connection.execute('SELECT value FROM update_probe').fetchone()[0]
        finally:
            connection.close()
        self.assertEqual(value, 'from-override')

    def test_checksum_mismatch_preserves_old_binary(self) -> None:
        before = self.installed_binary().read_bytes()
        backups_before = self.backup_files()
        result = self.call(
            'update', '--version', BAD_CHECKSUM_VERSION,
            '--prefix', str(self.prefix), expect=2,
        )
        self.assertIn('checksum mismatch', result.stderr.lower())
        self.assertEqual(self.installed_binary().read_bytes(), before)
        self.assertEqual(self.backup_files(), backups_before)

    def test_truncated_archive_preserves_old_binary(self) -> None:
        before = self.installed_binary().read_bytes()
        result = self.call(
            'update', '--version', TRUNCATED_VERSION,
            '--prefix', str(self.prefix), expect=2,
        )
        self.assertIn('truncated', result.stderr.lower())
        self.assertEqual(self.installed_binary().read_bytes(), before)

    def test_unmanaged_prefix_is_refused(self) -> None:
        unmanaged = self.root / 'unmanaged'
        (unmanaged / 'bin').mkdir(parents=True)
        executable = unmanaged / 'bin' / 'orbit'
        executable.write_text('#!/bin/sh\necho "orbit 0.1.0"\n', encoding='utf-8')
        executable.chmod(0o755)
        before = executable.read_bytes()
        result = self.call('update', '--prefix', str(unmanaged), expect=2)
        self.assertIn('not a GitHub-managed installation', result.stderr)
        self.assertEqual(executable.read_bytes(), before)

    def test_downgrade_requires_allow_downgrade(self) -> None:
        before = self.installed_binary().read_bytes()
        refused = self.call(
            'update', '--version', f'v{DOWNGRADE_VERSION}',
            '--prefix', str(self.prefix), expect=2,
        )
        self.assertIn('--allow-downgrade', refused.stderr)
        self.assertEqual(self.installed_binary().read_bytes(), before)

        allowed = self.json_call(
            'update', '--version', DOWNGRADE_VERSION,
            '--prefix', str(self.prefix), '--allow-downgrade',
        )
        self.assertEqual(allowed['action'], 'installed')
        self.assertEqual(allowed['installed_version'], DOWNGRADE_VERSION)
        self.assertIn(
            DOWNGRADE_VERSION,
            self.installed_binary().read_text(encoding='utf-8'),
        )

    def test_channel_and_explicit_version_selection(self) -> None:
        stable = self.json_call('update', '--check', '--prefix', str(self.prefix))
        self.assertEqual(stable['channel'], 'stable')
        self.assertEqual(stable['available'], STABLE_VERSION)

        prerelease = self.json_call(
            'update', '--check', '--channel', 'prerelease',
            '--prefix', str(self.prefix),
        )
        self.assertEqual(prerelease['channel'], 'prerelease')
        self.assertEqual(prerelease['available'], PRERELEASE_VERSION)
        self.assertTrue(prerelease['update_available'])

        explicit = self.json_call(
            'update', '--check', '--version', f'v{PRERELEASE_VERSION}',
            '--prefix', str(self.prefix),
        )
        self.assertEqual(explicit['available'], PRERELEASE_VERSION)
        self.assertEqual(explicit['tag'], f'v{PRERELEASE_VERSION}')

        explicit_stable = self.json_call(
            'update', '--check', '--version', STABLE_VERSION,
            '--prefix', str(self.prefix),
        )
        self.assertEqual(explicit_stable['available'], STABLE_VERSION)

    def test_force_reinstalls_the_same_version(self) -> None:
        first = self.json_call(
            'update', '--version', STABLE_VERSION, '--prefix', str(self.prefix)
        )
        self.assertEqual(first['action'], 'installed')

        up_to_date = self.json_call(
            'update', '--version', STABLE_VERSION, '--prefix', str(self.prefix)
        )
        self.assertEqual(up_to_date['action'], 'up_to_date')
        self.assertFalse(up_to_date['update_available'])
        self.assertIsNone(up_to_date['installed_version'])

        forced = self.json_call(
            'update', '--version', STABLE_VERSION,
            '--prefix', str(self.prefix), '--force',
        )
        self.assertEqual(forced['action'], 'installed')
        self.assertEqual(forced['installed_version'], STABLE_VERSION)

    def test_invalid_configuration_fails_before_mutation(self) -> None:
        before = self.installed_binary().read_bytes()
        backups_before = self.backup_files()
        self.write_config('version = 1\nunknown = true\n')
        result = self.call(
            'update', '--version', STABLE_VERSION,
            '--prefix', str(self.prefix), expect=2,
        )
        self.assertIn('configuration', result.stderr.lower())
        self.assertEqual(self.installed_binary().read_bytes(), before)
        self.assertEqual(self.backup_files(), backups_before)

    def test_output_never_contains_credentials(self) -> None:
        secret = 'super-secret-update-token-value'
        env = self.environment(
            ORBIT_SYNC_HTTP_TOKEN=secret,
            ORBIT_GITHUB_TOKEN=secret,
        )
        result = self.call(
            '--json', 'update', '--check', '--prefix', str(self.prefix), env=env
        )
        self.assertNotIn(secret, result.stdout)
        self.assertNotIn(secret, result.stderr)
        self.assertNotIn('Bearer', result.stdout)
        report = json.loads(result.stdout)
        self.assertEqual(report['type'], 'update')


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument('--binary', default='target/debug/orbit')
    arguments, remaining = parser.parse_known_args()
    UpdateAcceptance.binary = Path(arguments.binary).resolve()
    program = unittest.main(
        argv=[__file__, *remaining],
        exit=False,
        verbosity=2,
    )
    return 0 if program.result.wasSuccessful() else 1


if __name__ == '__main__':
    raise SystemExit(main())
