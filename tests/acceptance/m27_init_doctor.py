#!/usr/bin/env python3
"""M27 acceptance: `orbit init` and read-only `orbit doctor`.

Usage: python3 tests/acceptance/m27_init_doctor.py --binary target/debug/orbit

The suite runs the compiled CLI against isolated temporary homes. Every
invocation sets HOME and ORBIT_CONFIG so no real user state is read or written.
"""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import shutil
import stat
import subprocess
import tempfile
import unittest


DEFAULT_CONFIG = 'version = 1\n\n[mode]\nauthority = "local"\n'


class InitDoctorAcceptance(unittest.TestCase):
    binary = Path('target/debug/orbit').resolve()

    @classmethod
    def setUpClass(cls) -> None:
        if not cls.binary.is_file():
            raise unittest.SkipTest(f'compiled binary not found: {cls.binary}')

    def setUp(self) -> None:
        self.root = Path(tempfile.mkdtemp(prefix='orbit-m27-'))
        self.home = self.root / 'home'
        self.home.mkdir()
        self.config_path = self.home / '.orbit' / 'config.toml'

    def tearDown(self) -> None:
        shutil.rmtree(self.root, ignore_errors=True)

    def environment(self, **overrides: object) -> dict[str, str]:
        env = {
            key: value
            for key, value in os.environ.items()
            if not key.startswith('ORBIT_')
        }
        env['HOME'] = str(self.home)
        env['USERPROFILE'] = str(self.home)
        env['ORBIT_CONFIG'] = str(self.config_path)
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

    def test_fresh_init_creates_documented_layout(self) -> None:
        report = self.json_call('init')
        self.assertEqual(report['type'], 'init')
        self.assertEqual(report['config_path'], str(self.config_path))
        self.assertEqual(report['config_action'], 'created')
        self.assertEqual(report['authority'], 'local')
        self.assertEqual(report['database'], str(self.home / '.orbit' / 'orbit.db'))
        self.assertEqual(report['database_action'], 'created')
        self.assertEqual(report['logs'], str(self.home / '.orbit' / 'logs'))
        self.assertEqual(report['state'], str(self.home / '.orbit' / 'state'))
        self.assertEqual(report['backups'], str(self.home / '.orbit' / 'backups'))
        self.assertEqual(self.config_path.read_text(encoding='utf-8'), DEFAULT_CONFIG)
        self.assertTrue((self.home / '.orbit' / 'orbit.db').is_file())
        for name in ('logs', 'state', 'backups'):
            self.assertTrue((self.home / '.orbit' / name).is_dir(), name)
        if os.name == 'posix':
            self.assertEqual(stat.S_IMODE(self.config_path.stat().st_mode), 0o600)
            for name in ('.orbit', 'logs', 'state', 'backups'):
                path = self.home / name if name == '.orbit' else self.home / '.orbit' / name
                self.assertEqual(stat.S_IMODE(path.stat().st_mode), 0o700, name)
        status = self.json_call('status')
        self.assertEqual(status['type'], 'workspace_status')
        self.assertTrue(status['healthy'])
        self.assertEqual(status['database'], str(self.home / '.orbit' / 'orbit.db'))

    def test_second_init_retains_configuration_and_database(self) -> None:
        first = self.json_call('init')
        self.assertEqual(first['config_action'], 'created')
        before = self.config_path.read_text(encoding='utf-8')
        second = self.json_call('init')
        self.assertEqual(second['config_action'], 'retained')
        self.assertEqual(second['database_action'], 'existing')
        self.assertEqual(self.config_path.read_text(encoding='utf-8'), before)

    def test_custom_database_is_recorded_and_used(self) -> None:
        custom = self.root / 'data' / 'custom.db'
        report = self.json_call('init', '--database', str(custom))
        self.assertEqual(report['database'], str(custom))
        self.assertEqual(report['database_action'], 'created')
        self.assertTrue(custom.is_file())
        config = self.config_path.read_text(encoding='utf-8')
        self.assertIn('[storage]', config)
        self.assertIn(f'database = "{custom}"', config)
        status = self.json_call('status')
        self.assertEqual(status['database'], str(custom))

    def test_custom_config_location_does_not_relocate_runtime_directories(self) -> None:
        custom_config = self.root / 'elsewhere' / 'config.toml'
        report = self.json_call('init', '--config', str(custom_config))
        self.assertEqual(report['config_path'], str(custom_config))
        self.assertEqual(report['database'], str(self.home / '.orbit' / 'orbit.db'))
        self.assertEqual(report['logs'], str(self.home / '.orbit' / 'logs'))
        self.assertTrue(custom_config.is_file())
        self.assertTrue((self.home / '.orbit' / 'orbit.db').is_file())

    def test_orbit_db_environment_and_cli_override_precedence(self) -> None:
        self.json_call('init')
        env_database = self.root / 'env.db'
        env = self.environment(ORBIT_DB=env_database)
        created = self.json_call('task', 'add', 'Environment task', env=env)
        self.assertEqual(created['type'], 'task')
        listed = self.json_call('task', 'list', env=env)
        self.assertEqual(len(listed['tasks']), 1)
        default_list = self.json_call('task', 'list')
        self.assertEqual(default_list['tasks'], [])
        status = self.json_call('status', env=env)
        self.assertEqual(status['database'], str(env_database))

        cli_database = self.root / 'cli.db'
        self.json_call(
            '--db',
            str(cli_database),
            'task',
            'add',
            'CLI task',
            env=env,
        )
        cli_list = self.json_call('--db', str(cli_database), 'task', 'list', env=env)
        self.assertEqual(len(cli_list['tasks']), 1)
        cli_status = self.json_call('--db', str(cli_database), 'status', env=env)
        self.assertEqual(cli_status['database'], str(cli_database))

    def test_existing_configuration_is_preserved_and_conflicting_flags_fail(self) -> None:
        original = (
            'version = 1\n[storage]\ndatabase = "kept.db"\n'
            '[mode]\nauthority = "local"\n'
        )
        self.write_config(original)
        report = self.json_call('init')
        self.assertEqual(report['config_action'], 'retained')
        self.assertEqual(
            report['database'], str(self.config_path.parent / 'kept.db')
        )
        self.assertEqual(self.config_path.read_text(encoding='utf-8'), original)

        rejected = self.json_call(
            'init',
            '--database',
            str(self.root / 'other.db'),
            expect=2,
        )
        self.assertEqual(rejected['type'], 'error')
        self.assertIn('--force', rejected['message'])
        self.assertEqual(self.config_path.read_text(encoding='utf-8'), original)

    def test_invalid_existing_configuration_is_rejected(self) -> None:
        original = 'version = 1\nunknown = true\n'
        self.write_config(original)
        rejected = self.json_call('init', expect=2)
        self.assertEqual(rejected['type'], 'error')
        self.assertEqual(self.config_path.read_text(encoding='utf-8'), original)
        command = self.json_call('status', expect=2)
        self.assertEqual(command['type'], 'error')
        self.assertIn('invalid configuration', command['message'])

    def test_connected_init_and_doctor_report_authority(self) -> None:
        replica = self.root / 'replica.db'
        report = self.json_call(
            'init',
            '--authority',
            'connected',
            '--server',
            'https://orbit.example',
            '--replica-database',
            str(replica),
            '--agent-provider',
            'codex',
        )
        self.assertEqual(report['authority'], 'connected')
        self.assertEqual(report['database'], str(replica))
        self.assertTrue(replica.is_file())
        config = self.config_path.read_text(encoding='utf-8')
        self.assertIn('[connected]', config)
        self.assertIn('server = "https://orbit.example"', config)
        self.assertIn('[agent]', config)
        self.assertIn('default_provider = "codex"', config)
        doctor = self.json_call('doctor')
        self.assertEqual(doctor['authority']['mode'], 'connected')
        self.assertEqual(doctor['authority']['server'], 'https://orbit.example')
        self.assertEqual(doctor['authority']['replica'], str(replica))
        self.assertTrue(doctor['authority']['replica_exists'])

    def test_doctor_json_shape_and_areas(self) -> None:
        self.json_call('init')
        doctor = self.json_call('doctor')
        self.assertEqual(doctor['type'], 'doctor')
        self.assertIn(doctor['status'], ('ok', 'warnings', 'errors'))
        for section in ('executable', 'config', 'database', 'authority', 'optional'):
            self.assertIn(section, doctor)
        areas = {finding['area'] for finding in doctor['findings']}
        for area in ('executable', 'configuration', 'database', 'authority'):
            self.assertIn(area, areas)
        for finding in doctor['findings']:
            self.assertIn(finding['severity'], ('ok', 'info', 'advice', 'error'))
            self.assertIn('message', finding)
        self.assertEqual(doctor['config']['path'], str(self.config_path))
        self.assertTrue(doctor['config']['valid'])
        self.assertEqual(
            doctor['database']['schema_version'],
            doctor['database']['supported_schema_version'],
        )
        self.assertTrue(doctor['database']['integrity_ok'])
        human = self.call('doctor').stdout
        self.assertIn('Status:', human)
        self.assertTrue(any(prefix in human for prefix in ('OK:', 'INFO:', 'ADVICE:')))

    def test_doctor_missing_database_is_read_only(self) -> None:
        missing = self.root / 'absent' / 'orbit.db'
        self.write_config(
            'version = 1\n[storage]\n'
            f'database = "{missing}"\n'
            '[mode]\nauthority = "local"\n'
        )
        doctor = self.json_call('doctor')
        self.assertEqual(doctor['database']['path'], str(missing))
        self.assertFalse(doctor['database']['exists'])
        self.assertFalse(missing.exists())
        self.assertFalse(missing.parent.exists())

    def test_doctor_never_prints_credentials(self) -> None:
        self.json_call('init')
        secret = 'super-secret-token-value'
        env = self.environment(
            ORBIT_SYNC_HTTP_TOKEN=secret,
            ORBIT_AUTHORITY_DEVICE='device-identifier',
        )
        result = self.call('--json', 'doctor', env=env)
        self.assertNotIn(secret, result.stdout)
        self.assertNotIn(secret, result.stderr)
        self.assertNotIn('device-identifier', result.stdout)
        report = json.loads(result.stdout)
        self.assertEqual(report['authority']['credentials'], 'configured (value hidden)')

    def test_doctor_strict_exits_one_without_orbit_prefix(self) -> None:
        self.write_config('version = 1\nunknown = true\n')
        result = self.call('--json', 'doctor', '--strict', expect=1)
        self.assertNotIn('orbit:', result.stderr)
        report = json.loads(result.stdout)
        self.assertEqual(report['type'], 'doctor')
        self.assertEqual(report['status'], 'errors')
        self.assertTrue(any(
            finding['severity'] == 'error'
            for finding in report['findings']
        ))

    def test_init_failure_on_invalid_database_preserves_data_and_config(self) -> None:
        invalid = self.root / 'invalid.db'
        payload = b'this is not a sqlite database'
        invalid.write_bytes(payload)
        result = self.json_call('init', '--database', str(invalid), expect=4)
        self.assertEqual(result['type'], 'error')
        self.assertEqual(invalid.read_bytes(), payload)
        self.assertFalse(self.config_path.exists())

    def test_init_and_doctor_help(self) -> None:
        init_help = self.call('init', '--help')
        self.assertIn('--authority', init_help.stdout)
        self.assertIn('--non-interactive', init_help.stdout)
        doctor_help = self.call('doctor', '--help')
        self.assertIn('--strict', doctor_help.stdout)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument('--binary', default='target/debug/orbit')
    arguments, remaining = parser.parse_known_args()
    InitDoctorAcceptance.binary = Path(arguments.binary).resolve()
    program = unittest.main(
        argv=[__file__, *remaining],
        exit=False,
        verbosity=2,
    )
    return 0 if program.result.wasSuccessful() else 1


if __name__ == '__main__':
    raise SystemExit(main())
