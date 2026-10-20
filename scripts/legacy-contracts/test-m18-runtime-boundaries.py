#!/usr/bin/env python3
"""Fail-closed source checks for the M18 single-authority runtime contract.

These checks complement, and never replace, executable Rust acceptance. They
focus on architectural regressions introduced during repository consolidation:
transport adapters opening private SQLite stores, duplicate authoritative
business tables, raw mutation handles, and legacy relay command ingestion.
"""
from __future__ import annotations

from pathlib import Path
import re
import unittest

ROOT = Path(__file__).resolve().parents[2]
CRATES = ROOT / 'crates'


def source(crate: str, filename: str = 'lib.rs') -> str:
    return (CRATES / crate / 'src' / filename).read_text(encoding='utf-8')


def without_comments(text: str) -> str:
    """Discard line comments for broad structural assertions."""
    return '\n'.join(line.split('//', 1)[0] for line in text.splitlines())


class OwnershipContracts(unittest.TestCase):
    def test_application_service_is_host_owned(self):
        host = source('orbit-runtime')
        self.assertIn('pub(crate) fn application_services(&mut self)', host)
        self.assertIn('ApplicationServices::new(&mut self.authority.conn)', host)

    def test_application_queries_are_host_owned(self):
        host = source('orbit-runtime')
        self.assertIn('pub fn application_queries(&self)', host)
        self.assertIn('ApplicationQueries::new(&self.authority.conn)', host)

    def test_legacy_mutable_store_escape_hatch_is_absent(self):
        host = source('orbit-runtime')
        self.assertNotIn('pub fn application_store_mut(', host)
        self.assertNotIn('pub fn into_legacy_store(', host)

    def test_shared_store_is_opened_once_in_host(self):
        host = source('orbit-runtime')
        self.assertIn('store::AuthorityStore::open(path)', host)
        self.assertNotIn('SqliteStore::open(path)', host)

    def test_host_declares_one_application_database_owner(self):
        host = source('orbit-runtime')
        self.assertIn('authority: store::AuthorityStore', host)
        self.assertNotIn('application_store: SqliteStore', host)
        self.assertNotIn('local_store: SqliteStore', host)

    def test_services_do_not_open_a_new_database(self):
        services = source('orbit-runtime', 'application_services.rs')
        for forbidden in ('SqliteStore::open(', 'Connection::open(', 'AuthorityStore::open('):
            self.assertNotIn(forbidden, services)

    def test_queries_do_not_expose_sqlite_handle(self):
        services = source('orbit-runtime', 'application_services.rs')
        self.assertIn('pub(crate) fn new(store:', services)
        self.assertNotRegex(services, r'pub fn (?:get_)?store\(&self\)\s*->\s*&SqliteStore')

    def test_diagnostics_are_host_queries(self):
        services = source('orbit-runtime', 'application_services.rs')
        cli = source('orbit-cli')
        self.assertIn('pub struct ApplicationDiagnostics', services)
        self.assertIn('pub fn diagnostics(&self)', services)
        self.assertIn('workspace_status_host(&host, &db_path)', cli)
        self.assertIn('host.application_queries().diagnostics()', cli)

    def test_cli_status_uses_markdown_count(self):
        cli = source('orbit-cli')
        self.assertIn('host.markdown_knowledge_index()', cli)
        status_section = cli.split('fn workspace_status_host(')[1].split('\nfn ')[0]
        self.assertNotIn('list_knowledge_items(', status_section)

    def test_cli_search_reads_through_host_and_markdown_vault(self):
        cli = source('orbit-cli')
        self.assertIn('search_host(&host, args)', cli)
        section = cli.split('fn search_host(')[1].split('\nfn ')[0]
        self.assertIn('host.application_queries()', section)
        self.assertIn('host.markdown_search(', section)
        self.assertNotIn('search_knowledge(', section)
        self.assertNotIn('SqliteStore', section)

    def test_cli_core_project_task_mutations_are_host_owned(self):
        cli = source('orbit-cli')
        task_section = cli.split('fn run_task(')[1].split('\nfn ')[0]
        project_section = cli.split('fn run_project_host(')[1].split('\nfn ')[0]
        self.assertIn('host.set_task_planning_accepted(', task_section)
        self.assertIn('host.configure_project_strategy_accepted(', project_section)
        self.assertNotIn('SqliteStore::open(', task_section)
        self.assertNotIn('SqliteStore::open(', project_section)

    def test_cli_routine_and_scheduling_are_host_owned(self):
        cli = source('orbit-cli')
        self.assertIn('run_routine_host(&mut host', cli)
        self.assertIn('run_schedule_host(&mut host', cli)
        self.assertNotIn('run_schedule(store,', cli)

    def test_mcp_uses_host_application_services(self):
        mcp = '\n'.join(p.read_text() for p in (CRATES / 'orbit-mcp' / 'src').rglob('*.rs'))
        self.assertIn('create_task_accepted(', mcp)
        self.assertNotIn('SqliteStore::open(', mcp)

    def test_embedded_client_uses_host_service(self):
        client = source('orbit-client')
        self.assertIn('application_queries()', client)
        self.assertIn('apply_task_plan_accepted(', client)
        self.assertNotIn('application_services().apply_task_plan(', client)
        self.assertNotIn('SqliteStore::open(', client)

    def test_remote_server_does_not_open_a_second_application_store(self):
        server = source('orbit-server', 'main.rs')
        self.assertNotIn('SqliteStore::open(', server)
        self.assertIn('AuthoritativeHost', server)

    def test_old_duplicate_application_tables_not_in_runtime_schema(self):
        runtime = source('orbit-runtime', 'store.rs').split('#[cfg(test)]\nmod ')[0]
        for table in ('authority_projects', 'authority_tasks', 'authority_habits',
                      'authority_habit_occurrences', 'authority_metrics',
                      'authority_metric_observations', 'authority_knowledge_items'):
            self.assertNotRegex(runtime, rf'CREATE TABLE(?: IF NOT EXISTS)?\s+{table}\b')

    def test_markdown_content_not_authoritative_sql_table(self):
        runtime = source('orbit-runtime', 'store.rs')
        self.assertNotIn('CREATE TABLE IF NOT EXISTS authority_knowledge_items', runtime)

    def test_host_opens_knowledge_recovery_before_serving(self):
        host = source('orbit-runtime')
        open_part = host.split('pub fn open(path:')[1].split('\n    ///')[0]
        for method in ('recover_folder_creates()', 'recover_markdown_commands()',
                       'recover_folder_commands()'):
            self.assertIn(method, open_part)

    def test_single_replica_outbox_is_explicit(self):
        unified = source('orbit-client', 'unified.rs')
        self.assertIn('outbox', unified.lower())
        self.assertIn('Pending', unified)

    def test_old_host_mutable_store_api_not_used_by_clients(self):
        for crate in ('orbit-cli', 'orbit-client', 'orbit-mcp', 'orbit-tui'):
            for f in (CRATES / crate / 'src').rglob('*.rs'):
                self.assertNotIn('application_store_mut()', f.read_text(), str(f))

    def test_executable_acceptance_is_fail_closed(self):
        script = (ROOT / 'scripts/legacy-contracts/m18-acceptance.sh').read_text()
        self.assertIn('cargo check --workspace', script)
        self.assertIn('cargo test --workspace', script)
        self.assertIn('exit 2', script)
        self.assertIn('test-m18-e2e-workflows.py', script)

    def test_no_fake_binary_acceptance(self):
        script = (ROOT / 'scripts/legacy-contracts/test-m18-e2e-workflows.py').read_text()
        self.assertIn('subprocess.run(args', script)
        self.assertIn('sqlite3.connect(self.db)', script)
        self.assertIn('if not binary.is_file()', script)
        self.assertNotIn('MagicMock', script)


if __name__ == '__main__':
    unittest.main(verbosity=2)
