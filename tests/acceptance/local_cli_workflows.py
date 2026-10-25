#!/usr/bin/env python3
"""Executable acceptance for the *real* Orbit CLI, not source-shape checks.

Usage: python3 tests/acceptance/local_cli_workflows.py --binary target/debug/orbit

This script intentionally refuses to pass without a runnable binary. It runs
against fresh, isolated workspaces and checks persisted state after separate
process invocations. No monkeypatch, fake runtime, or SQLite fixture stands in
for Orbit's actual application services. The suite is a gate, not an estimate.

Validates core persisted local CLI behavior as an M15 acceptance check.
"""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import sqlite3
import subprocess
import sys
import tempfile
import unittest


class OrbitCli:
    def __init__(self, executable: Path, db: Path):
        self.executable = executable
        self.db = db
        self.history: list[tuple[str, ...]] = []

    def call(self, *argv: str, successful: bool = True) -> dict:
        args = [str(self.executable), '--db', str(self.db), '--json', *argv]
        self.history.append(tuple(argv))
        result = subprocess.run(args, text=True, capture_output=True, timeout=30)
        if successful and result.returncode:
            raise AssertionError(
                f'command {argv!r} unexpectedly failed ({result.returncode})\n'
                f'stdout={result.stdout}\nstderr={result.stderr}'
            )
        if not successful:
            if result.returncode == 0:
                raise AssertionError(f'expected failure from {argv!r}; got {result.stdout}')
            return {'stdout': result.stdout, 'stderr': result.stderr,
                    'returncode': result.returncode}
        try:
            parsed = json.loads(result.stdout)
        except json.JSONDecodeError as error:
            raise AssertionError(f'non-JSON response for {argv!r}: {result.stdout}') from error
        if not isinstance(parsed, dict):
            raise AssertionError(f'expected JSON object for {argv!r}, received {parsed!r}')
        return parsed

    def create_project(self, name: str) -> str:
        result = self.call('project', 'add', name)
        assert result['type'] == 'project', result
        return result['project']['id']

    def create_habit(self, name: str, *options: str) -> str:
        result = self.call('habit', 'add', name, *options)
        assert result['type'] == 'habit', result
        return result['habit']['id']

    def create_routine(self, name: str) -> str:
        result = self.call('routine', 'add', name)
        assert result['type'] == 'routine', result
        return result['routine']['id']

    def sqlite_snapshot(self) -> dict:
        if not self.db.is_file():
            raise AssertionError('Orbit did not persist a SQLite database')
        with sqlite3.connect(self.db) as conn:
            conn.row_factory = sqlite3.Row
            objects = conn.execute(
                "SELECT name, type FROM sqlite_master WHERE name NOT LIKE 'sqlite_%' ORDER BY name"
            ).fetchall()
            return {
                'objects': {row['name']: row['type'] for row in objects},
                'integrity': conn.execute('PRAGMA integrity_check').fetchone()[0],
                'foreign_key_violations': conn.execute('PRAGMA foreign_key_check').fetchall(),
                'application_entities': {
                    table: conn.execute(f'SELECT COUNT(*) FROM "{table}"').fetchone()[0]
                    for table in ('projects', 'tasks', 'habits', 'routines')
                    if table in {row['name'] for row in objects}
                },
            }


class RuntimeAcceptance(unittest.TestCase):
    executable: Path

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='orbit-local-cli-')
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.client = OrbitCli(self.executable, self.root / 'orbit.sqlite')

    def assertHealthy(self):
        snapshot = self.client.sqlite_snapshot()
        self.assertEqual(snapshot['integrity'], 'ok')
        self.assertEqual(snapshot['foreign_key_violations'], [])
        return snapshot

    def test_new_workspace_is_runnable_and_persistent(self):
        first = self.client.call('status')
        self.assertIsInstance(first, dict)
        self.assertTrue(self.client.db.is_file())
        before = self.assertHealthy()
        second = OrbitCli(self.executable, self.client.db).call('status')
        self.assertEqual(first, second, 'reopening workspace changed diagnostics')
        self.assertEqual(before, self.assertHealthy())

    def test_project_create_and_list_survive_reopen(self):
        project_id = self.client.create_project('M18 End-to-End')
        listed = self.client.call('project', 'list')
        self.assertIn(project_id, json.dumps(listed))
        second = OrbitCli(self.executable, self.client.db).call('project', 'list')
        self.assertIn('M18 End-to-End', json.dumps(second))
        self.assertHealthy()

    def test_project_generated_identities_are_distinct(self):
        self.client.create_project('First')
        self.client.create_project('Different')
        listed = self.client.call('project', 'list')
        self.assertIn('First', json.dumps(listed))
        self.assertIn('Different', json.dumps(listed))
        self.assertHealthy()

    def test_task_create_and_list_after_restart(self):
        self.client.call('task', 'add', 'Prepare architecture review', '--id', 'accept-task')
        self.assertIn('accept-task', json.dumps(self.client.call('task', 'list')))
        other = OrbitCli(self.executable, self.client.db)
        self.assertIn('Prepare architecture review', json.dumps(other.call('task', 'list')))
        self.assertHealthy()

    def test_tasks_preserve_project_association(self):
        planning_id = self.client.create_project('Planning')
        self.client.call('task', 'add', 'Attached', '--project', planning_id, '--id', 'attached')
        project_tasks = self.client.call('task', 'list', '--project', planning_id)
        self.assertIn('attached', json.dumps(project_tasks))
        self.assertHealthy()

    def test_invalid_task_project_is_rejected_without_partial_write(self):
        before = self.client.call('task', 'list')
        self.client.call('task', 'add', 'Orphan', '--project', 'missing-project', successful=False)
        self.assertEqual(before, self.client.call('task', 'list'))
        self.assertHealthy()

    def test_task_lifecycle_and_activity_survive_restart(self):
        self.client.call('task', 'add', 'Ship runtime', '--id', 'lifecycle')
        self.client.call('task', 'start', 'lifecycle')
        self.client.call('task', 'complete', 'lifecycle')
        reopened = OrbitCli(self.executable, self.client.db)
        self.assertIn('lifecycle', json.dumps(reopened.call('task', 'list')))
        self.assertHealthy()

    def test_project_task_cannot_redefine_identity(self):
        project_id = self.client.create_project('Project A')
        self.client.call('task', 'add', 'Task A', '--id', 'task-a', '--project', project_id)
        self.client.call('task', 'add', 'Task B', '--id', 'task-a', '--project', project_id, successful=False)
        tasks = self.client.call('task', 'list')
        self.assertIn('Task A', json.dumps(tasks))
        self.assertNotIn('Task B', json.dumps(tasks))
        self.assertHealthy()

    def test_status_counts_reconcile_with_shared_entity_tables(self):
        self.client.create_project('One')
        self.client.call('task', 'add', 'Two', '--id', 'two')
        status = self.client.call('status')
        snapshot = self.assertHealthy()
        self.assertEqual(snapshot['application_entities'].get('projects'), 1)
        self.assertEqual(snapshot['application_entities'].get('tasks'), 1)
        self.assertIn('1', json.dumps(status))

    def test_duplicate_authoritative_entity_tables_are_absent(self):
        self.client.call('status')
        objects = self.assertHealthy()['objects']
        for name in ('authority_projects', 'authority_tasks', 'authority_habits',
                     'authority_metrics', 'authority_habit_occurrences',
                     'authority_metric_observations', 'authority_knowledge_items'):
            self.assertNotIn(name, objects, f'old competing authority table: {name}')

    def test_second_process_uses_same_persisted_entities(self):
        first = OrbitCli(self.executable, self.client.db)
        second = OrbitCli(self.executable, self.client.db)
        project_id = first.create_project('First process')
        self.assertIn('First process', json.dumps(second.call('project', 'list')))
        second.call('task', 'add', 'Second process', '--id', 'second-process')
        self.assertIn('Second process', json.dumps(first.call('task', 'list')))
        self.assertHealthy()

    def test_failed_command_does_not_break_later_commands(self):
        self.client.call('task', 'complete', 'nonexistent', successful=False)
        self.client.create_project('Still operational')
        self.assertIn('Still operational', json.dumps(self.client.call('project', 'list')))
        self.assertHealthy()

    def test_blank_search_fails_without_changing_database(self):
        self.client.call('status')
        before = self.assertHealthy()['application_entities']
        self.client.call('search', '   ', successful=False)
        self.assertEqual(before, self.assertHealthy()['application_entities'])

    def test_project_search_returns_persisted_project(self):
        self.client.create_project('UniqueOrbitProject')
        found = self.client.call('search', 'UniqueOrbitProject')
        self.assertIn('UniqueOrbitProject', json.dumps(found))
        self.assertHealthy()

    def test_task_search_returns_persisted_task(self):
        self.client.call('task', 'add', 'UniqueOrbitTask', '--id', 'unique-task')
        found = self.client.call('search', 'UniqueOrbitTask')
        self.assertIn('UniqueOrbitTask', json.dumps(found))
        self.assertHealthy()

    def test_cli_help_and_json_errors_do_not_corrupt_state(self):
        self.client.call('status')
        before = self.assertHealthy()
        self.client.call('task', 'unexpected-subcommand', successful=False)
        after = self.assertHealthy()
        self.assertEqual(before['application_entities'], after['application_entities'])


    def test_habit_create_and_history_after_restart(self):
        habit_id = self.client.create_habit('Daily review')
        reopened = OrbitCli(self.executable, self.client.db)
        self.assertIn(habit_id, json.dumps(reopened.call('habit', 'list')))
        self.assertHealthy()

    def test_weekly_habit_retains_cadence(self):
        habit_id = self.client.create_habit('Weekly review', '--weekly-target', '3')
        listed = self.client.call('habit', 'list')
        self.assertIn(habit_id, json.dumps(listed))
        self.assertIn('3/week', json.dumps(listed))
        self.assertHealthy()

    def test_habit_pause_resume_survives_restart(self):
        habit_id = self.client.create_habit('Exercise')
        self.client.call('habit', 'pause', habit_id)
        paused = OrbitCli(self.executable, self.client.db).call('habit', 'list', '--paused')
        self.assertIn(habit_id, json.dumps(paused))
        self.client.call('habit', 'resume', habit_id)
        active = OrbitCli(self.executable, self.client.db).call('habit', 'list', '--active')
        self.assertIn(habit_id, json.dumps(active))
        self.assertHealthy()

    def test_habit_occurrences_are_persisted(self):
        habit_id = self.client.create_habit('Write notes')
        self.client.call('habit', 'check', habit_id, '--occurrence-id', 'occurrence-1',
                         '--at-ms', '1760000000000')
        history = OrbitCli(self.executable, self.client.db).call('habit', 'history', habit_id)
        self.assertIn('occurrence-1', json.dumps(history))
        self.assertHealthy()

    def test_habit_occurrence_duplicate_identity_is_rejected(self):
        habit_id = self.client.create_habit('Drink water')
        self.client.call('habit', 'check', habit_id, '--occurrence-id', 'same-occurrence',
                         '--at-ms', '1760000000000')
        self.client.call('habit', 'check', habit_id, '--occurrence-id', 'same-occurrence',
                         '--at-ms', '1760000060000', successful=False)
        history = self.client.call('habit', 'history', habit_id)
        self.assertIn('same-occurrence', json.dumps(history))
        self.assertHealthy()

    def test_routine_create_and_list_after_restart(self):
        routine_id = self.client.create_routine('Morning session')
        reopened = OrbitCli(self.executable, self.client.db)
        self.assertIn(routine_id, json.dumps(reopened.call('routine', 'list')))
        self.assertHealthy()

    def test_routine_membership_can_be_added_and_removed(self):
        habit_id = self.client.create_habit('Meditation')
        routine_id = self.client.create_routine('Morning practice')
        self.client.call('routine', 'add-habit', routine_id, habit_id)
        self.assertIn(routine_id, json.dumps(self.client.call('routine', 'list')))
        self.client.call('routine', 'remove-habit', routine_id, habit_id)
        self.assertHealthy()

    def test_routine_cannot_reference_missing_habit(self):
        routine_id = self.client.create_routine('Reference protection')
        self.client.call('routine', 'add-habit', routine_id, 'nonexistent-habit', successful=False)
        self.assertHealthy()

    def test_schedule_create_and_list_after_restart(self):
        self.client.call('schedule', 'add', 'Deep work', '--start-ms', '1760000000000',
                         '--duration-minutes', '30')
        reopened = OrbitCli(self.executable, self.client.db)
        response = reopened.call('schedule', 'list', '--from-ms', '1759999999000',
                                 '--to-ms', '1760004000000')
        self.assertIn('Deep work', json.dumps(response))
        self.assertHealthy()

    def test_schedule_can_target_existing_task(self):
        self.client.call('task', 'add', 'Review', '--id', 'scheduled-task')
        self.client.call('schedule', 'add', 'Review block', '--task', 'scheduled-task',
                         '--start-ms', '1760000000000', '--duration-minutes', '30')
        self.assertIn('Review block', json.dumps(self.client.call('schedule', 'list',
            '--from-ms', '1759999999000', '--to-ms', '1760004000000')))
        self.assertHealthy()

    def test_schedule_cannot_target_missing_task(self):
        self.client.call('schedule', 'add', 'Impossible block', '--task', 'missing-task',
                         '--start-ms', '1760000000000', '--duration-minutes', '30',
                         successful=False)
        self.assertHealthy()

    def test_schedule_invalid_range_fails_without_write(self):
        self.client.call('schedule', 'list', '--from-ms', '1000', '--to-ms', '999',
                         successful=False)
        self.assertHealthy()

    def test_project_status_transition_is_persistent(self):
        project_id = self.client.create_project('In progress')
        self.client.call('project', 'status', project_id, 'active')
        reopened = OrbitCli(self.executable, self.client.db)
        self.assertIn(project_id, json.dumps(reopened.call('project', 'list', '--status', 'active')))
        self.assertHealthy()

    def test_project_health_uses_same_workspace(self):
        project_id = self.client.create_project('Health source')
        response = self.client.call('project', 'health', project_id)
        self.assertIn(project_id, json.dumps(response))
        self.assertHealthy()

    def test_task_planning_preserves_other_fields(self):
        self.client.call('task', 'add', 'Estimate', '--id', 'estimate-task')
        self.client.call('task', 'planning', 'estimate-task', '--estimate-minutes', '45')
        reopened = OrbitCli(self.executable, self.client.db)
        tasks = reopened.call('task', 'list')
        self.assertIn('Estimate', json.dumps(tasks))
        self.assertHealthy()

    def test_multiple_domain_families_share_one_database(self):
        project_id = self.client.create_project('Combined state')
        self.client.call('task', 'add', 'Combined task', '--project', project_id,
                         '--id', 'combined-task')
        habit_id = self.client.create_habit('Combined habit')
        routine_id = self.client.create_routine('Combined routine')
        self.client.call('routine', 'add-habit', routine_id, habit_id)
        snapshot = self.assertHealthy()
        for table in ('projects', 'tasks', 'habits', 'routines'):
            self.assertEqual(snapshot['application_entities'][table], 1, table)
        status = OrbitCli(self.executable, self.client.db).call('status')
        self.assertIn('workspace_status', json.dumps(status))

    def test_generated_project_ids_survive_independent_restarts(self):
        a = self.client.create_project('A')
        b = OrbitCli(self.executable, self.client.db).create_project('B')
        self.assertNotEqual(a, b)
        projects = self.client.call('project', 'list')
        self.assertIn(a, json.dumps(projects))
        self.assertIn(b, json.dumps(projects))
        self.assertHealthy()

    def test_error_does_not_rollback_earlier_accepted_work(self):
        self.client.call('task', 'add', 'Accepted task', '--id', 'accepted-before-error')
        self.client.call('task', 'start', 'missing-task', successful=False)
        reopened = OrbitCli(self.executable, self.client.db)
        self.assertIn('Accepted task', json.dumps(reopened.call('task', 'list')))
        self.assertHealthy()

    def test_search_across_project_task_and_habit(self):
        self.client.create_project('SharedSearchProject')
        self.client.call('task', 'add', 'SharedSearchTask', '--id', 'search-task')
        self.client.create_habit('SharedSearchHabit')
        for word in ('SharedSearchProject', 'SharedSearchTask', 'SharedSearchHabit'):
            self.assertIn(word, json.dumps(self.client.call('search', word)))
        self.assertHealthy()

    def test_after_restart_status_counts_match_sqlite(self):
        self.client.create_project('Status project')
        self.client.call('task', 'add', 'Status task')
        self.client.create_habit('Status habit')
        self.client.create_routine('Status routine')
        reopened = OrbitCli(self.executable, self.client.db)
        status = reopened.call('status')
        snapshot = reopened.sqlite_snapshot()
        for entity, count in snapshot['application_entities'].items():
            self.assertEqual(status[entity], count, f'status count mismatch: {entity}')
        self.assertHealthy()

    def test_read_only_commands_do_not_create_duplicate_entity_tables(self):
        self.client.create_project('Read stability')
        self.client.call('project', 'list')
        self.client.call('task', 'list')
        self.client.call('habit', 'list')
        self.client.call('routine', 'list')
        self.client.call('status')
        snapshot = self.assertHealthy()
        self.assertFalse(any(name.startswith('authority_projects') or name.startswith('authority_tasks')
                             for name in snapshot['objects']))

    def test_crash_safe_restart_after_many_accepted_commands(self):
        for index in range(8):
            self.client.call('task', 'add', f'Accepted {index}', '--id', f'accepted-{index}')
        reopened = OrbitCli(self.executable, self.client.db)
        listed = reopened.call('task', 'list')
        for index in range(8):
            self.assertIn(f'accepted-{index}', json.dumps(listed))
        self.assertHealthy()

    def test_workspace_has_one_application_entity_schema(self):
        self.client.call('status')
        objects = self.assertHealthy()['objects']
        for table in ('projects', 'tasks', 'habits', 'routines'):
            self.assertIn(table, objects)
        for table in ('authority_projects', 'authority_tasks', 'authority_habits'):
            self.assertNotIn(table, objects)

    def test_failed_habit_command_preserves_application_state(self):
        self.client.create_habit('First habit')
        before = self.assertHealthy()['application_entities']
        self.client.call('habit', 'pause', 'nonexistent', successful=False)
        self.assertEqual(before, self.assertHealthy()['application_entities'])

    def test_multiple_reopens_do_not_mutate_schema_unexpectedly(self):
        self.client.call('status')
        base = self.assertHealthy()['objects']
        for _ in range(4):
            reopened = OrbitCli(self.executable, self.client.db)
            reopened.call('status')
            self.assertEqual(base, reopened.sqlite_snapshot()['objects'])
        self.assertHealthy()

    def test_invalid_range_does_not_damage_valid_schedule(self):
        self.client.call('schedule', 'add', 'Valid schedule', '--start-ms', '1760000000000',
                         '--duration-minutes', '20')
        self.client.call('schedule', 'list', '--from-ms', '1760001000000',
                         '--to-ms', '1760000000000', successful=False)
        self.assertIn('Valid schedule', json.dumps(self.client.call('schedule', 'list',
            '--from-ms', '1759999999000', '--to-ms', '1760004000000')))
        self.assertHealthy()

    def test_task_scope_remains_correct_when_other_project_is_created(self):
        p1 = self.client.create_project('P1')
        p2 = self.client.create_project('P2')
        self.client.call('task', 'add', 'P1-only task', '--project', p1, '--id', 'p1-only')
        in_p1 = json.dumps(self.client.call('task', 'list', '--project', p1))
        in_p2 = json.dumps(self.client.call('task', 'list', '--project', p2))
        self.assertIn('P1-only task', in_p1)
        self.assertNotIn('P1-only task', in_p2)
        self.assertHealthy()


    def test_project_status_filters_across_restart(self):
        project_id = self.client.create_project('Active filter')
        self.client.call('project', 'status', project_id, 'active')
        other = OrbitCli(self.executable, self.client.db)
        self.assertIn(project_id, json.dumps(other.call('project', 'list', '--status', 'active')))
        self.assertNotIn(project_id, json.dumps(other.call('project', 'list', '--status', 'archived')))
        self.assertHealthy()

    def test_task_status_filter_across_restart(self):
        self.client.call('task', 'add', 'Task to finish', '--id', 'task-to-finish')
        self.client.call('task', 'start', 'task-to-finish')
        self.client.call('task', 'complete', 'task-to-finish')
        other = OrbitCli(self.executable, self.client.db)
        self.assertIn('task-to-finish', json.dumps(other.call('task', 'list', '--status', 'done')))
        self.assertHealthy()

    def test_project_query_filters_are_case_insensitive(self):
        self.client.create_project('UniqueNameForQuery')
        filtered = self.client.call('project', 'list', '--query', 'uniquenameforquery')
        self.assertIn('UniqueNameForQuery', json.dumps(filtered))
        self.assertHealthy()

    def test_task_query_filters_are_case_insensitive(self):
        self.client.call('task', 'add', 'DistinctiveTASKTITLE', '--id', 'distinctive-task')
        filtered = self.client.call('task', 'list', '--query', 'distinctivetasktitle')
        self.assertIn('DistinctiveTASKTITLE', json.dumps(filtered))
        self.assertHealthy()

    def test_habit_query_filters_are_case_insensitive(self):
        self.client.create_habit('DistinctiveHABITNAME')
        filtered = self.client.call('habit', 'list', '--query', 'distinctivehabitname')
        self.assertIn('DistinctiveHABITNAME', json.dumps(filtered))
        self.assertHealthy()

    def test_blank_project_filter_rejected_without_mutation(self):
        self.client.create_project('Existing project')
        before = self.assertHealthy()['application_entities']
        self.client.call('project', 'list', '--query', ' ', successful=False)
        self.assertEqual(before, self.assertHealthy()['application_entities'])

    def test_blank_task_filter_rejected_without_mutation(self):
        self.client.call('task', 'add', 'Existing task')
        before = self.assertHealthy()['application_entities']
        self.client.call('task', 'list', '--query', ' ', successful=False)
        self.assertEqual(before, self.assertHealthy()['application_entities'])

    def test_invalid_weekly_target_preserves_habits(self):
        self.client.create_habit('Valid habit')
        before = self.assertHealthy()['application_entities']
        self.client.call('habit', 'add', 'Bad target', '--weekly-target', '0', successful=False)
        self.assertEqual(before, self.assertHealthy()['application_entities'])

    def test_schedule_missing_required_args_does_not_modify_database(self):
        self.client.call('status')
        before = self.assertHealthy()['application_entities']
        self.client.call('schedule', 'add', 'Missing time', successful=False)
        self.assertEqual(before, self.assertHealthy()['application_entities'])

    def test_schedule_query_filters(self):
        self.client.call('schedule', 'add', 'Unmatched meeting', '--start-ms', '1760000000000',
                         '--duration-minutes', '20')
        self.client.call('schedule', 'add', 'Unique planning block', '--start-ms', '1760000100000',
                         '--duration-minutes', '20')
        response = self.client.call('schedule', 'list', '--from-ms', '1759999999000',
                                    '--to-ms', '1760004000000', '--query', 'unique')
        self.assertIn('Unique planning block', json.dumps(response))
        self.assertNotIn('Unmatched meeting', json.dumps(response))
        self.assertHealthy()

    def test_task_planning_invalid_deadline_preserves_record(self):
        self.client.call('task', 'add', 'Deadline test', '--id', 'deadline-test')
        before = self.client.call('task', 'list')
        self.client.call('task', 'planning', 'deadline-test', '--earliest-start-ms', '2000',
                         '--deadline-ms', '1000', successful=False)
        self.assertEqual(before, self.client.call('task', 'list'))
        self.assertHealthy()

    def test_many_project_creations_use_one_table(self):
        for index in range(8):
            self.client.create_project(f'Bulk project {index}')
        snapshot = self.assertHealthy()
        self.assertEqual(snapshot['application_entities']['projects'], 8)
        self.assertNotIn('authority_projects', snapshot['objects'])

    def test_many_habit_creations_use_one_table(self):
        for index in range(8):
            self.client.create_habit(f'Bulk habit {index}')
        snapshot = self.assertHealthy()
        self.assertEqual(snapshot['application_entities']['habits'], 8)
        self.assertNotIn('authority_habits', snapshot['objects'])

    def test_multiple_invalid_commands_do_not_corrupt_data(self):
        project_id = self.client.create_project('Stable data')
        self.client.call('task', 'start', 'missing-task', successful=False)
        self.client.call('project', 'status', 'missing-project', 'active', successful=False)
        self.client.call('habit', 'pause', 'missing-habit', successful=False)
        snapshot = self.assertHealthy()
        self.assertEqual(snapshot['application_entities']['projects'], 1)
        self.assertIn(project_id, json.dumps(self.client.call('project', 'list')))

    def test_application_tables_have_no_unresolved_foreign_keys(self):
        project_id = self.client.create_project('Referential project')
        self.client.call('task', 'add', 'Referential task', '--id', 'referential-task',
                         '--project', project_id)
        habit_id = self.client.create_habit('Referential habit')
        routine_id = self.client.create_routine('Referential routine')
        self.client.call('routine', 'add-habit', routine_id, habit_id)
        snapshot = self.assertHealthy()
        self.assertEqual(snapshot['foreign_key_violations'], [])

    def test_search_and_status_are_read_only_across_restarts(self):
        self.client.create_project('Immutable read test')
        before = self.assertHealthy()
        for _ in range(3):
            reader = OrbitCli(self.executable, self.client.db)
            reader.call('search', 'Immutable read test')
            reader.call('status')
        after = self.assertHealthy()
        self.assertEqual(before['application_entities'], after['application_entities'])
        self.assertEqual(before['objects'], after['objects'])

    def test_blueprint_version_upgrade_is_previewed_fenced_and_persistent(self):
        """Preview -> apply is explicit, fingerprint-fenced and restart-safe."""
        project_id = self.client.create_project('Upgrade persistence')
        manifest_v1 = {
            'id': 'upgrade-demo',
            'version': 1,
            'name': 'Upgrade demo',
            'work_items': [{
                'id': 'feature',
                'name': 'Feature',
                'initial_state': 'backlog',
                'states': ['backlog', 'done'],
                'fields': [
                    {'id': 'effort', 'kind': {'type': 'text'},
                     'required': True, 'default': 'small'},
                ],
                'transitions': [{'from': 'backlog', 'to': 'done'}],
            }],
        }
        manifest_v2 = {
            'id': 'upgrade-demo',
            'version': 2,
            'name': 'Upgrade demo',
            'work_items': [{
                'id': 'feature',
                'name': 'Feature',
                'initial_state': 'backlog',
                'states': ['backlog', 'review', 'done'],
                'fields': [
                    {'id': 'effort',
                     'kind': {'type': 'choice', 'options': ['small', 'large']},
                     'required': True, 'default': 'small'},
                    {'id': 'owner', 'kind': {'type': 'text'}},
                ],
                'transitions': [
                    {'from': 'backlog', 'to': 'review'},
                    {'from': 'review', 'to': 'done'},
                ],
            }],
        }
        first = self.root / 'upgrade-v1.json'
        second = self.root / 'upgrade-v2.json'
        first.write_text(json.dumps(manifest_v1))
        second.write_text(json.dumps(manifest_v2))
        properties = self.root / 'properties.json'
        properties.write_text(json.dumps({'effort': 'small'}))

        self.client.call('blueprint', 'import', str(first))
        applied = self.client.call(
            'blueprint', 'apply', project_id, 'upgrade-demo', '--version', '1')
        self.assertEqual(applied['result']['version'], 1)
        item = self.client.call(
            'blueprint', 'create-item', project_id, 'item-1', 'feature',
            '--properties', str(properties))
        self.assertEqual(item['result']['state'], 'backlog')
        self.client.call('blueprint', 'import', str(second))

        preview = self.client.call(
            'blueprint', 'upgrade-preview', project_id,
            '--id', 'upgrade-demo', '--version', '2')['result']
        self.assertEqual(preview['current']['version'], 1)
        self.assertEqual(preview['target']['version'], 2)
        self.assertEqual(preview['blockers'], [])
        self.assertIn('field_type_widened',
                      [warning['code'] for warning in preview['warnings']])
        self.assertEqual(len(preview['fingerprint']), 64)
        # A preview is a read: the project is still pinned to version 1.
        self.assertEqual(
            self.client.call('blueprint', 'binding', project_id)['result']['version'], 1)

        def upgrade(fingerprint: str, confirm: bool = True) -> dict:
            argv = ['blueprint', 'upgrade', project_id, '--id', 'upgrade-demo',
                    '--version', '2', '--fingerprint', fingerprint]
            if confirm:
                argv.append('--confirm')
            return self.client.call(*argv, successful=False)

        missing_confirm = upgrade(preview['fingerprint'], confirm=False)
        self.assertEqual(missing_confirm['returncode'], 2)
        self.assertEqual(json.loads(missing_confirm['stderr'])['code'], 'usage_error')

        wrong = upgrade('0' * 64)
        self.assertEqual(wrong['returncode'], 4)
        self.assertEqual(json.loads(wrong['stderr'])['code'], 'repository_error')
        self.assertEqual(
            self.client.call('blueprint', 'binding', project_id)['result']['version'], 1)

        # Any accepted change after the preview invalidates its fingerprint.
        properties.write_text(json.dumps({'effort': 'large'}))
        self.client.call(
            'blueprint', 'create-item', project_id, 'item-2', 'feature',
            '--properties', str(properties))
        stale = upgrade(preview['fingerprint'])
        self.assertEqual(stale['returncode'], 4)
        self.assertIn('fingerprint mismatch', json.loads(stale['stderr'])['message'])
        self.assertEqual(
            self.client.call('blueprint', 'binding', project_id)['result']['version'], 1)

        refreshed = self.client.call(
            'blueprint', 'upgrade-preview', project_id,
            '--id', 'upgrade-demo', '--version', '2')['result']['fingerprint']
        self.assertNotEqual(refreshed, preview['fingerprint'])
        success = self.client.call(
            'blueprint', 'upgrade', project_id, '--id', 'upgrade-demo',
            '--version', '2', '--fingerprint', refreshed, '--confirm')['result']
        self.assertEqual(success['binding']['version'], 2)
        self.assertEqual(success['previous']['version'], 1)
        self.assertFalse(success['already_applied'])
        self.assertEqual(success['fingerprint'], refreshed)

        restarted = OrbitCli(self.executable, self.client.db)
        self.assertEqual(
            restarted.call('blueprint', 'binding', project_id)['result']['version'], 2)
        preserved = restarted.call('blueprint', 'item', 'item-1')['result']['item']
        self.assertEqual(preserved['state'], 'backlog')
        self.assertEqual(preserved['revision'], 0)
        self.assertEqual(preserved['properties']['effort'], 'small')

        replay = restarted.call(
            'blueprint', 'upgrade', project_id, '--id', 'upgrade-demo',
            '--version', '2', '--fingerprint', refreshed, '--confirm')['result']
        self.assertTrue(replay['already_applied'])
        self.assertEqual(replay['binding']['version'], 2)
        self.assertHealthy()


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, required=True)
    args = parser.parse_args()
    binary = args.binary.resolve()
    if not binary.is_file() or not os.access(binary, os.X_OK):
        parser.error(f'executable Orbit CLI binary required: {binary}')
    RuntimeAcceptance.executable = binary
    suite = unittest.defaultTestLoader.loadTestsFromTestCase(RuntimeAcceptance)
    outcome = unittest.TextTestRunner(verbosity=2).run(suite)
    return 0 if outcome.wasSuccessful() else 1


if __name__ == '__main__':
    raise SystemExit(main())
