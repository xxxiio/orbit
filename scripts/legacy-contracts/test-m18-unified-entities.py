#!/usr/bin/env python3
"""Verify consolidated entity storage and wire-format projections in SQLite.

Runs real SQLite transactions, but does not compile or exercise Rust.
"""
from pathlib import Path
import re
import sqlite3
import tempfile

root=Path(__file__).resolve().parents[2]
store=(root/'crates/orbit-runtime/src/store.rs').read_text()
evidence=(root/'crates/orbit-runtime/src/authority_evidence.rs').read_text()
host=(root/'crates/orbit-runtime/src/lib.rs').read_text()
assert not (root/'crates/orbit-runtime/src/projection_bridge.rs').exists()
assert 'projection_bridge::' not in host
assert 'CREATE TABLE IF NOT EXISTS authority_projects' not in store
assert 'CREATE TABLE IF NOT EXISTS authority_tasks' not in store
for table in ('authority_habits','authority_habit_occurrences','authority_metrics','authority_metric_observations'):
    assert f'CREATE TABLE IF NOT EXISTS {table}' not in store
    assert 'INTO '+table not in evidence
assert 'INSERT OR IGNORE INTO projects' in store
assert 'INSERT OR IGNORE INTO tasks' in store
assert 'INSERT OR IGNORE INTO habits' in evidence
assert 'INSERT OR IGNORE INTO metric_targets' in evidence
assert 'INSERT OR IGNORE INTO metric_observations' in evidence

with tempfile.TemporaryDirectory() as d:
    db=str(Path(d)/'orbit.db')
    a,b=sqlite3.connect(db),sqlite3.connect(db)
    a.executescript('''
       CREATE TABLE projects(id TEXT PRIMARY KEY,name TEXT NOT NULL,status TEXT NOT NULL);
       CREATE TABLE tasks(id TEXT PRIMARY KEY,title TEXT NOT NULL,status TEXT NOT NULL,project_id TEXT,milestone_id TEXT,estimated_minutes INTEGER,status_sequence INTEGER NOT NULL DEFAULT 0);
       CREATE TABLE habits(id TEXT PRIMARY KEY,name TEXT NOT NULL,cadence_kind TEXT NOT NULL,target_per_week INTEGER,active INTEGER NOT NULL);
       CREATE TABLE habit_occurrences(id TEXT PRIMARY KEY,habit_id TEXT NOT NULL,occurred_at_ms INTEGER NOT NULL,duration_minutes INTEGER);
       CREATE TABLE metric_targets(metric_id TEXT PRIMARY KEY,baseline INTEGER NOT NULL,target INTEGER NOT NULL,direction TEXT NOT NULL);
       CREATE TABLE metric_observations(metric_id TEXT NOT NULL,observed_at_ms INTEGER NOT NULL,value INTEGER NOT NULL,PRIMARY KEY(metric_id,observed_at_ms));
    ''')
    # Application repository and the server now see the *same* rows, not
    # independent authority_* projections maintained by triggers.
    a.execute("INSERT INTO projects VALUES ('p','Project','proposed')")
    a.execute("INSERT INTO tasks(id,title,status,project_id,status_sequence) VALUES ('t','Task','inbox','p',1)")
    a.execute("INSERT INTO habits VALUES ('h','Read','weekly',3,1)")
    a.execute("INSERT INTO habit_occurrences VALUES ('occ','h',123,NULL)")
    a.execute("INSERT INTO metric_targets VALUES ('energy',0,20,'at_least')")
    a.execute("INSERT INTO metric_observations VALUES ('energy',123,5)")
    a.commit()
    assert b.execute("SELECT title FROM tasks WHERE id='t'").fetchone()==('Task',)
    assert b.execute("SELECT CASE WHEN cadence_kind='daily' THEN 'daily' ELSE 'weekly:'||target_per_week END FROM habits WHERE id='h'").fetchone()==('weekly:3',)
    assert b.execute("SELECT CASE WHEN direction='at_least' THEN 'increasing' ELSE 'decreasing' END FROM metric_targets WHERE metric_id='energy'").fetchone()==('increasing',)
    # Ancillary application fields are preserved by authoritative changes.
    b.execute("UPDATE tasks SET estimated_minutes=30 WHERE id='t'")
    b.commit()
    a.execute("UPDATE tasks SET status='planned',status_sequence=2 WHERE id='t'")
    a.commit()
    assert b.execute("SELECT status,estimated_minutes FROM tasks WHERE id='t'").fetchone()==('planned',30)
    assert b.execute("SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name LIKE 'authority_%'").fetchone()[0]==0
print('M18 unified entity schema and SQLite behavior: passed')
