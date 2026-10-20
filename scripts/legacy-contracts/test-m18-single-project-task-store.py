#!/usr/bin/env python3
"""Executable SQLite acceptance for the greenfield project/task schema cutover.

This runs SQL and source contracts; it does not replace Rust compilation.
"""
from pathlib import Path
import sqlite3
import tempfile

root = Path(__file__).resolve().parents[2]
source = (root/'crates/orbit-runtime/src/store.rs').read_text()
assert 'CREATE TABLE IF NOT EXISTS authority_projects' not in source
assert 'CREATE TABLE IF NOT EXISTS authority_tasks' not in source
assert 'FROM projects ORDER BY id' in source
assert 'FROM tasks ORDER BY id' in source
assert 'INSERT OR IGNORE INTO projects' in source
assert 'INSERT OR IGNORE INTO tasks' in source
assert 'UPDATE tasks SET status=' in source

with tempfile.TemporaryDirectory() as d:
    path = str(Path(d)/'orbit.sqlite')
    app = sqlite3.connect(path)
    api = sqlite3.connect(path)
    app.executescript('''
       CREATE TABLE projects (id TEXT PRIMARY KEY, name TEXT NOT NULL, status TEXT NOT NULL);
       CREATE TABLE tasks (id TEXT PRIMARY KEY, title TEXT NOT NULL, status TEXT NOT NULL,
          project_id TEXT, milestone_id TEXT, estimated_minutes INTEGER);
    ''')
    # Mimic the runtime's schema addition to the *existing* application table.
    api.execute('ALTER TABLE tasks ADD COLUMN status_sequence INTEGER NOT NULL DEFAULT 0')
    api.execute("INSERT INTO projects(id,name,status) VALUES('p','Project','proposed')")
    api.execute("INSERT INTO tasks(id,title,status,status_sequence,project_id) VALUES('t','Task','inbox',1,'p')")
    api.commit()
    assert app.execute("SELECT title,status FROM tasks WHERE id='t'").fetchone() == ('Task','inbox')
    # Rich application fields survive canonical updates and no projection is needed.
    app.execute("UPDATE tasks SET milestone_id='m',estimated_minutes=30 WHERE id='t'")
    app.commit()
    api.execute("UPDATE tasks SET status='planned',status_sequence=2 WHERE id='t'")
    api.commit()
    assert app.execute("SELECT status,milestone_id,estimated_minutes,status_sequence FROM tasks WHERE id='t'").fetchone() == ('planned','m',30,2)
    assert api.execute("SELECT COUNT(*) FROM sqlite_master WHERE name IN ('authority_projects','authority_tasks')").fetchone()[0] == 0
print('M18 shared project/task schema source and SQL contract: passed')
