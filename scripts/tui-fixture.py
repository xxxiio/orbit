#!/usr/bin/env python3
"""Build a deterministic, realistic Orbit workspace for TUI interaction probes.

The fixture uses the real ``orbit`` CLI for everything the CLI can express:

* projects: ``orbit project add``
* tasks: ``orbit task add --id ...``
* estimates/deadlines: ``orbit task planning``
* statuses: ``orbit task status`` through an allowed transition sequence
* blueprints: ``orbit blueprint install`` / ``orbit blueprint apply``
* attachments: ``orbit attachment add``

Two pieces have no cheap CLI path and are deliberately written through the
storage schema as *fixture aids*:

* task priority (the CLI has no priority setter; the TUI quick-edit form does,
  but driving that 200 times would make the fixture interaction-bound), and
* agent-job history rows (``orbit agent start`` launches an external runtime).

Activity rows come from the CLI mutations themselves.  A storage-only top-up
is used only when the CLI-generated count falls below ``--activity-target``;
the summary records exactly how many rows were synthetic.

The script never uses fixed paths.  Run it with ``--out`` or let it create a
fresh directory under ``/tmp/opencode``.

Examples
--------
    python3 scripts/tui-fixture.py --out /tmp/opencode/m29-run --json

The module is also importable so the acceptance smoke test and the performance
probe can share the same seeding code.
"""
from __future__ import annotations

import argparse
import json
import os
import random
import sqlite3
import subprocess
import sys
import tempfile
import time
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]
TUI_DIR = REPO_ROOT / "tests" / "acceptance" / "tui"

SEED = 20261024
FULL_PROJECT_COUNT = 20
FULL_TASK_COUNT = 200
ACTIVITY_TARGET = 1000


def _load_pty_harness():
    """Import the shared PTY harness by path (tests/ is not a package)."""
    if str(TUI_DIR) not in sys.path:
        sys.path.insert(0, str(TUI_DIR))
    import pty_harness  # type: ignore

    return pty_harness


_PTY = _load_pty_harness()
Workspace = _PTY.Workspace
DEFAULT_CLI_BINARY = _PTY.DEFAULT_CLI_BINARY
_isolated_environment = _PTY._isolated_environment


# ---------------------------------------------------------------------------
# Deterministic data catalogue
# ---------------------------------------------------------------------------
# (name, workflow id).  ``Performance Lab`` stays on Basic because the Basic
# blueprint owns the dense Tasks table used by the scroll probe.
FULL_PROJECTS: list[tuple[str, str]] = [
    ("Platform Core", "basic"),
    ("Release Readiness", "basic"),
    ("Client Foundation", "basic"),
    ("Orbit Web", "basic"),
    ("Desktop Shell", "basic"),
    ("Sync Reliability", "development"),
    ("Runtime Hardening", "development"),
    ("Search Index", "development"),
    ("Observability", "development"),
    ("Research: Retrieval Quality", "research"),
    ("Research: Local Models", "research"),
    ("Learning: Systems Design", "learning"),
    ("Learning: Rust Deep Dive", "learning"),
    ("Habits: Deep Work", "habits"),
    ("Habits: Daily Review", "habits"),
    ("Docs and Onboarding", "basic"),
    ("Security Review", "basic"),
    ("Performance Lab", "basic"),
    ("Data Migration", "basic"),
    ("Community Feedback", "basic"),
]

# Smaller runs still span the bundled workflows and always include the project
# the probes use for the dense task table.
SMALL_PREFERENCE = [
    "Performance Lab",
    "Platform Core",
    "Sync Reliability",
    "Research: Retrieval Quality",
    "Learning: Systems Design",
    "Habits: Deep Work",
    "Orbit Web",
    "Release Readiness",
    "Runtime Hardening",
    "Search Index",
    "Observability",
    "Research: Local Models",
    "Learning: Rust Deep Dive",
    "Habits: Daily Review",
    "Docs and Onboarding",
    "Security Review",
    "Data Migration",
    "Community Feedback",
    "Client Foundation",
    "Desktop Shell",
]

TASK_VERBS = [
    "Implement",
    "Refine",
    "Audit",
    "Document",
    "Benchmark",
    "Simplify",
    "Automate",
    "Validate",
    "Instrument",
    "Migrate",
    "Harden",
    "Review",
    "Design",
    "Profile",
    "Stabilize",
    "Outline",
    "Test",
    "Optimize",
    "Clarify",
    "Prepare",
]

TASK_OBJECTS = [
    "dependency graph",
    "offline sync queue",
    "command palette",
    "task detail layout",
    "search ranking",
    "blueprint binding",
    "attachment pipeline",
    "schedule planner",
    "agent job recovery",
    "workspace migration",
    "activity feed",
    "keyboard navigation",
    "project sidebar",
    "form validation",
    "error reporting",
    "storage diagnostics",
    "release checklist",
    "onboarding guide",
    "retrieval index",
    "notification badges",
]

TASK_CONTEXTS = [
    "for the desktop shell",
    "for the CLI",
    "across the sync layer",
    "in the TUI",
    "without regressions",
    "under load",
    "for new contributors",
    "before the release",
]

STATUS_WEIGHTS: list[tuple[str, int]] = [
    ("done", 20),
    ("review", 13),
    ("waiting", 13),
    ("in-progress", 15),
    ("blocked", 10),
    ("ready", 10),
    ("planned", 5),
    ("deferred", 5),
    ("cancelled", 5),
    ("inbox", 4),
]

# Allowed transition sequences from the inbox state.  Every step is accepted by
# the domain; no invalid shortcut is attempted.
STATUS_SEQUENCES: dict[str, list[str]] = {
    "inbox": [],
    "planned": ["planned"],
    "ready": ["planned", "ready"],
    "in-progress": ["planned", "ready", "in-progress"],
    "waiting": ["planned", "ready", "in-progress", "waiting"],
    "blocked": ["planned", "ready", "blocked"],
    "review": ["planned", "ready", "in-progress", "review"],
    "done": ["planned", "ready", "in-progress", "done"],
    "deferred": ["deferred"],
    "cancelled": ["cancelled"],
}

PRIORITY_WEIGHTS: list[tuple[str, int]] = [
    ("low", 20),
    ("medium", 45),
    ("high", 27),
    ("urgent", 8),
]

ESTIMATES = [15, 30, 45, 60, 90, 120, 180, 240]

ATTACHMENT_FILES: list[tuple[str, str, str]] = [
    ("acceptance-notes.txt", "Text", "Acceptance notes and reproduction steps."),
    ("latency-trace.txt", "Text", "Trimmed latency trace captured under load."),
    ("schema-sketch.txt", "Text", "Draft schema for the migration review."),
    ("release-checklist.txt", "Text", "Pre-release verification checklist."),
    ("retrieval-eval.txt", "Text", "Retrieval evaluation notes for the cohort."),
    ("handoff-summary.txt", "Text", "Short handoff for the next agent session."),
]

AGENT_STATES = [
    "completed",
    "completed",
    "completed",
    "completed",
    "failed",
    "failed",
    "interrupted",
    "completed",
]


# ---------------------------------------------------------------------------
# Workspace helpers
# ---------------------------------------------------------------------------
def create_workspace(path: str | os.PathLike[str]) -> Workspace:
    """Create exactly ``path`` as an isolated workspace (no fixed paths)."""
    root = Path(path).expanduser().resolve()
    root.mkdir(parents=True, exist_ok=True)
    home = root / "home"
    home.mkdir(parents=True, exist_ok=True)
    db = root / "orbit.db"
    config = root / "orbit.config.toml"
    return Workspace(
        root=root,
        db=db,
        config=config,
        home=home,
        env=_isolated_environment(db, config, home),
    )


def workspace_from_path(path: str | os.PathLike[str]) -> Workspace:
    """Reconstruct the harness ``Workspace`` for an existing fixture directory."""
    root = Path(path).expanduser().resolve()
    db = root / "orbit.db"
    config = root / "orbit.config.toml"
    home = root / "home"
    if not db.is_file():
        raise FileNotFoundError(f"{db} does not exist; build the fixture first")
    if not home.is_dir():
        home.mkdir(parents=True, exist_ok=True)
    return Workspace(
        root=root,
        db=db,
        config=config,
        home=home,
        env=_isolated_environment(db, config, home),
    )


def _cli_binary() -> Path:
    candidate = Path(
        os.environ.get("ORBIT_CLI_BIN", str(DEFAULT_CLI_BINARY))
    ).expanduser()
    if not candidate.is_file():
        raise FileNotFoundError(
            f"orbit CLI not found at {candidate}; build it with "
            "`cargo build -p orbit-cli` or set ORBIT_CLI_BIN"
        )
    return candidate


class _Seeder:
    def __init__(self, workspace: Workspace, *, verbose: bool) -> None:
        self.workspace = workspace
        self.binary = _cli_binary()
        self.verbose = verbose
        self.commands = 0
        self.started = time.monotonic()

    def cli(self, *args: str, timeout: float = 90.0) -> dict:
        command = [
            str(self.binary),
            "--db",
            str(self.workspace.db),
            "--json",
            *args,
        ]
        completed = subprocess.run(
            command,
            env=dict(self.workspace.env),
            cwd=str(REPO_ROOT),
            text=True,
            capture_output=True,
            timeout=timeout,
        )
        self.commands += 1
        if completed.returncode != 0:
            raise RuntimeError(
                "fixture CLI command failed\n"
                f"command: {' '.join(command)}\n"
                f"stdout: {completed.stdout[-4000:]}\n"
                f"stderr: {completed.stderr[-4000:]}"
            )
        output = completed.stdout.strip()
        if not output:
            return {}
        try:
            return json.loads(output)
        except json.JSONDecodeError:
            return {"raw": output}

    def log(self, message: str) -> None:
        if self.verbose:
            elapsed = time.monotonic() - self.started
            print(f"[tui-fixture {elapsed:6.1f}s] {message}", file=sys.stderr)

    def sqlite(self) -> sqlite3.Connection:
        connection = sqlite3.connect(self.workspace.db)
        connection.execute("PRAGMA foreign_keys = ON")
        return connection

    def init(self) -> None:
        self.cli("init", "--non-interactive")

    def install_blueprints(self) -> dict[str, int]:
        workflows = sorted({workflow for _, workflow in FULL_PROJECTS})
        for workflow in workflows:
            self.cli("blueprint", "install", workflow)
        payload = self.cli("blueprint", "list")
        result = payload.get("result", [])
        versions: dict[str, int] = {}
        for row in result:
            if isinstance(row, (list, tuple)) and len(row) >= 2:
                blueprint_id, version = row[0], row[1]
            elif isinstance(row, dict):
                blueprint_id = row.get("id")
                version = row.get("version")
            else:
                continue
            if isinstance(blueprint_id, str) and isinstance(version, int):
                versions[blueprint_id] = max(versions.get(blueprint_id, 0), version)
        if not versions:
            raise RuntimeError(f"blueprint list returned no installed versions: {payload!r}")
        return versions

    def add_projects(
        self, definitions: list[tuple[str, str]], versions: dict[str, int]
    ) -> list[dict[str, str]]:
        projects: list[dict[str, str]] = []
        for name, workflow in definitions:
            payload = self.cli("project", "add", name)
            project = payload.get("project") or {}
            project_id = project.get("id")
            if not project_id:
                raise RuntimeError(f"project add returned no id: {payload!r}")
            if workflow != "basic":
                version = versions.get(workflow)
                if version is None:
                    raise RuntimeError(f"workflow {workflow!r} was not installed")
                self.cli(
                    "blueprint",
                    "apply",
                    project_id,
                    workflow,
                    "--version",
                    str(version),
                )
            projects.append(
                {"id": project_id, "name": name, "workflow": workflow}
            )
            self.log(f"project {len(projects):02d}/{len(definitions)} {name} [{workflow}]")
        return projects

    def _task_counts(self, project_count: int, task_count: int) -> list[int]:
        base = max(1, task_count // project_count - 3)
        counts = [base] * project_count
        hot = next(
            (
                index
                for index, (name, _) in enumerate(
                    _project_definitions(project_count)
                )
                if name == "Performance Lab"
            ),
            0,
        )
        remaining = task_count - base * project_count
        if remaining < 0:
            raise ValueError("task_count is too small for the requested projects")
        counts[hot] += remaining
        return counts

    def add_tasks(
        self,
        projects: list[dict[str, str]],
        task_count: int,
        rng: random.Random,
    ) -> tuple[list[dict[str, object]], int]:
        counts = self._task_counts(len(projects), task_count)
        combinations = [
            f"{verb} {obj} {context}"
            for verb in TASK_VERBS
            for obj in TASK_OBJECTS
            for context in TASK_CONTEXTS
        ]
        if task_count > len(combinations):
            raise ValueError("task_count exceeds the unique title catalogue")
        titles = rng.sample(combinations, task_count)
        now_ms = int(time.time() * 1000)
        task_index = 0
        tasks: list[dict[str, object]] = []
        transition_activity = 0

        for project, count in zip(projects, counts):
            for _ in range(count):
                title = titles[task_index]
                task_id = f"task-fixture-{task_index + 1:04d}"
                payload = self.cli(
                    "task", "add", title, "--project", project["id"], "--id", task_id
                )
                if not (payload.get("task") or {}).get("id"):
                    raise RuntimeError(f"task add returned no id: {payload!r}")

                estimate = rng.choice(ESTIMATES)
                status = rng.choices(
                    [value for value, _ in STATUS_WEIGHTS],
                    weights=[weight for _, weight in STATUS_WEIGHTS],
                    k=1,
                )[0]
                if status in ("done", "cancelled"):
                    deadline_days = rng.randint(-21, 4)
                elif status == "deferred":
                    deadline_days = rng.randint(20, 75)
                else:
                    deadline_days = rng.randint(1, 45)
                deadline_ms = now_ms + deadline_days * 86_400_000
                earliest_ms = None
                if status in ("ready", "in-progress", "waiting", "blocked", "review"):
                    earliest_ms = now_ms + rng.randint(0, 7) * 86_400_000
                    if earliest_ms >= deadline_ms:
                        earliest_ms = now_ms
                planning_args = [
                    "task",
                    "planning",
                    task_id,
                    "--estimate-minutes",
                    str(estimate),
                    "--deadline-ms",
                    str(deadline_ms),
                ]
                if earliest_ms is not None:
                    planning_args.extend(["--earliest-start-ms", str(earliest_ms)])
                self.cli(*planning_args)

                sequence = STATUS_SEQUENCES[status]
                for next_status in sequence:
                    self.cli("task", "status", task_id, next_status)
                    transition_activity += 1

                tasks.append(
                    {
                        "id": task_id,
                        "title": title,
                        "project_id": project["id"],
                        "status": status,
                        "priority": rng.choices(
                            [value for value, _ in PRIORITY_WEIGHTS],
                            weights=[weight for _, weight in PRIORITY_WEIGHTS],
                            k=1,
                        )[0],
                        "estimated_minutes": estimate,
                        "deadline_ms": deadline_ms,
                        "activity_transitions": len(sequence),
                    }
                )
                task_index += 1
                if task_index % 20 == 0:
                    self.log(
                        f"tasks {task_index}/{task_count} "
                        f"({transition_activity} status transitions)"
                    )
        return tasks, transition_activity

    def write_priorities(self, tasks: list[dict[str, object]]) -> None:
        """Fixture aid: priority has no CLI setter (the TUI quick edit does)."""
        connection = self.sqlite()
        try:
            connection.executemany(
                "UPDATE tasks SET priority=?1 WHERE id=?2",
                [(task["priority"], task["id"]) for task in tasks],
            )
            connection.commit()
        finally:
            connection.close()

    def add_attachments(self, tasks: list[dict[str, object]], count: int) -> int:
        artifacts = self.workspace.root / "artifacts"
        artifacts.mkdir(parents=True, exist_ok=True)
        attached = 0
        for index, (filename, _kind, description) in enumerate(
            ATTACHMENT_FILES[:count]
        ):
            task = tasks[index * max(1, len(tasks) // max(1, count)) % len(tasks)]
            path = artifacts / filename
            path.write_text(
                f"{description}\n\nFixture task: {task['title']}\n"
                f"Task id: {task['id']}\n",
                encoding="utf-8",
            )
            self.cli(
                "attachment",
                "add",
                "task",
                str(task["id"]),
                str(path),
                "--description",
                description,
            )
            attached += 1
        return attached

    def add_agent_jobs(
        self, tasks: list[dict[str, object]], count: int
    ) -> int:
        """Fixture aid: ``agent start`` would launch an external runtime."""
        if count <= 0 or not tasks:
            return 0
        now_ms = int(time.time() * 1000)
        rows = []
        for index in range(min(count, len(tasks))):
            task = tasks[(index * 7) % len(tasks)]
            state = AGENT_STATES[index % len(AGENT_STATES)]
            created = now_ms - (index + 1) * 3_600_000
            rows.append(
                (
                    f"agent-job-fixture-{index + 1:04d}",
                    f"request-fixture-{index + 1:04d}",
                    task["project_id"],
                    task["id"],
                    1,
                    ["codex", "claude", "opencode"][index % 3],
                    "local",
                    "herdr",
                    "file:///workspace/orbit",
                    state,
                    2,
                    created,
                    created + 1_800_000,
                    None,
                    f"session-fixture-{index + 1:04d}",
                )
            )
        connection = self.sqlite()
        try:
            connection.executemany(
                """
                INSERT INTO agent_jobs (
                    id, request_id, project_id, task_id, launch_revision,
                    agent_kind, machine_ref, runtime_kind, repository_ref,
                    state, revision, created_at_ms, updated_at_ms,
                    last_seen_at_ms, runtime_session_ref
                ) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)
                """,
                rows,
            )
            connection.commit()
        finally:
            connection.close()
        return len(rows)

    def top_up_activities(self, target: int, tasks: list[dict[str, object]]) -> int:
        """Fixture aid: append synthetic history when CLI activity is < target."""
        connection = self.sqlite()
        try:
            current = connection.execute("SELECT COUNT(*) FROM activities").fetchone()[0]
            missing = max(0, target - int(current))
            if missing == 0:
                return 0
            now_ms = int(time.time() * 1000)
            kinds = [
                "updated",
                "progress_updated",
                "scheduled",
                "rescheduled",
                "custom",
            ]
            rows = []
            for index in range(missing):
                task = tasks[index % len(tasks)]
                kind = kinds[index % len(kinds)]
                rows.append(
                    (
                        f"activity-fixture-{index + 1:05d}",
                        "task",
                        task["id"],
                        kind,
                        "fixture" if kind == "custom" else None,
                        "system",
                        "fixture-backfill",
                        now_ms - (missing - index) * 60_000,
                        f"Fixture history note {index + 1:04d} for {task['title']}",
                    )
                )
            connection.executemany(
                """
                INSERT INTO activities (
                    id, subject_kind, subject_id, activity_kind,
                    activity_kind_value, origin_kind, origin_value,
                    occurred_at_ms, summary
                ) VALUES (?,?,?,?,?,?,?,?,?)
                """,
                rows,
            )
            connection.commit()
            return len(rows)
        finally:
            connection.close()


def _project_definitions(count: int) -> list[tuple[str, str]]:
    if count <= 0:
        raise ValueError("project_count must be positive")
    if count > len(FULL_PROJECTS):
        raise ValueError(f"at most {len(FULL_PROJECTS)} projects are supported")
    if count == len(FULL_PROJECTS):
        return list(FULL_PROJECTS)
    by_name = dict(FULL_PROJECTS)
    return [(name, by_name[name]) for name in SMALL_PREFERENCE[:count]]


def _read_counts(workspace: Workspace) -> dict[str, object]:
    connection = sqlite3.connect(workspace.db)
    try:
        scalar = lambda sql: int(connection.execute(sql).fetchone()[0])
        pinned = [
            {"id": row[0], "version": row[1]}
            for row in connection.execute(
                "SELECT blueprint_id, blueprint_version FROM orbit_project_blueprints"
            )
        ]
        distinct_pinned = sorted(
            {(row["id"], row["version"]) for row in pinned}
        )
        statuses = {
            row[0]: int(row[1])
            for row in connection.execute(
                "SELECT status, COUNT(*) FROM tasks GROUP BY status ORDER BY status"
            )
        }
        return {
            "projects": scalar("SELECT COUNT(*) FROM projects"),
            "tasks": scalar("SELECT COUNT(*) FROM tasks"),
            "activities": scalar("SELECT COUNT(*) FROM activities"),
            "attachments": scalar("SELECT COUNT(*) FROM attachments"),
            "agent_jobs": scalar("SELECT COUNT(*) FROM agent_jobs"),
            "tasks_with_priority": scalar(
                "SELECT COUNT(*) FROM tasks WHERE priority IS NOT NULL"
            ),
            "tasks_with_estimate": scalar(
                "SELECT COUNT(*) FROM tasks WHERE estimated_minutes IS NOT NULL"
            ),
            "tasks_with_deadline": scalar(
                "SELECT COUNT(*) FROM tasks WHERE deadline_at_ms IS NOT NULL"
            ),
            "pinned_blueprints": pinned,
            "distinct_pinned_versions": len(distinct_pinned),
            "tasks_by_status": statuses,
        }
    finally:
        connection.close()


def _git_revision() -> str:
    try:
        completed = subprocess.run(
            ["git", "rev-parse", "HEAD"],
            cwd=str(REPO_ROOT),
            text=True,
            capture_output=True,
            check=False,
        )
        return completed.stdout.strip()
    except OSError:
        return ""


def build_fixture(
    out: str | os.PathLike[str],
    *,
    project_count: int = FULL_PROJECT_COUNT,
    task_count: int = FULL_TASK_COUNT,
    seed: int = SEED,
    activity_target: int = ACTIVITY_TARGET,
    attachment_count: int = 6,
    agent_job_count: int = 8,
    verbose: bool = False,
) -> dict[str, object]:
    """Build the workspace and return the machine-readable summary."""
    if task_count < project_count:
        raise ValueError("task_count must be at least project_count")
    rng = random.Random(seed)
    workspace = create_workspace(out)
    seeder = _Seeder(workspace, verbose=verbose)

    seeder.log(f"workspace {workspace.root}")
    seeder.init()
    versions = seeder.install_blueprints()
    definitions = _project_definitions(project_count)
    projects = seeder.add_projects(definitions, versions)
    tasks, transitions = seeder.add_tasks(projects, task_count, rng)
    seeder.write_priorities(tasks)
    attachments = seeder.add_attachments(tasks, attachment_count)
    agent_jobs = seeder.add_agent_jobs(tasks, agent_job_count)
    synthetic = seeder.top_up_activities(activity_target, tasks)
    counts = _read_counts(workspace)

    summary: dict[str, object] = {
        "workspace": str(workspace.root),
        "database": str(workspace.db),
        "seed": seed,
        "git_revision": _git_revision(),
        "duration_seconds": round(time.monotonic() - seeder.started, 3),
        "commands": seeder.commands,
        "counts": counts,
        "targets": {
            "projects": project_count,
            "tasks": task_count,
            "activity_target": activity_target,
            "attachments": attachment_count,
            "agent_jobs": agent_job_count,
        },
        "seeding": {
            "projects": "orbit project add",
            "tasks": "orbit task add --id (deterministic task ids)",
            "estimates_deadlines": "orbit task planning",
            "statuses": "orbit task status with allowed transition sequences",
            "status_transitions": transitions,
            "priorities": (
                "direct SQL UPDATE tasks.priority: the CLI has no priority "
                "setter; the TUI quick-edit form is the interactive path"
            ),
            "blueprints": "orbit blueprint install + orbit blueprint apply",
            "attachments": "orbit attachment add against generated text files",
            "activities_cli": "activity rows emitted by project/task/planning/status/attachment commands",
            "activities_synthetic": synthetic,
            "activities_synthetic_reason": (
                ""
                if synthetic == 0
                else "storage backfill used because CLI mutations did not reach the target"
            ),
            "agent_jobs": (
                "direct storage rows: `orbit agent start` would launch an "
                "external runtime; history rows are read-only fixture data"
            ),
        },
    }
    seeder.log(f"done: {json.dumps(counts, sort_keys=True)}")
    return summary


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--out",
        help="workspace directory (default: fresh temp dir under /tmp/opencode)",
    )
    parser.add_argument("--seed", type=int, default=SEED)
    parser.add_argument("--projects", type=int, default=None)
    parser.add_argument("--tasks", type=int, default=None)
    parser.add_argument("--activity-target", type=int, default=None)
    parser.add_argument("--attachments", type=int, default=6)
    parser.add_argument("--agent-jobs", type=int, default=8)
    parser.add_argument(
        "--full",
        action="store_true",
        help="use 20 projects / 200 tasks / 1000 activities (the default)",
    )
    parser.add_argument(
        "--small",
        action="store_true",
        help="use 5 projects / 40 tasks for a quick smoke fixture",
    )
    parser.add_argument("--json", action="store_true", help="print only the summary JSON")
    parser.add_argument("--verbose", action="store_true", help="progress on stderr")
    args = parser.parse_args(argv)

    # Full is the default for manual runs and for the probe.  The small shape
    # is opt-in so CI never pays for 20 projects / 200 tasks by accident.
    small = (
        args.small
        and not args.full
        and os.environ.get("ORBIT_TUI_FULL_FIXTURE") != "1"
    )
    if args.projects is not None:
        project_count = args.projects
    elif small:
        project_count = 5
    else:
        project_count = FULL_PROJECT_COUNT
    if args.tasks is not None:
        task_count = args.tasks
    elif small:
        task_count = 40
    else:
        task_count = FULL_TASK_COUNT
    if args.activity_target is not None:
        activity_target = args.activity_target
    elif small:
        activity_target = 180
    else:
        activity_target = ACTIVITY_TARGET

    out = args.out
    if not out:
        base = Path("/tmp/opencode")
        base.mkdir(parents=True, exist_ok=True)
        out = tempfile.mkdtemp(prefix="m29-fixture-", dir=str(base))

    try:
        summary = build_fixture(
            out,
            project_count=project_count,
            task_count=task_count,
            seed=args.seed,
            activity_target=activity_target,
            attachment_count=args.attachments,
            agent_job_count=args.agent_jobs,
            verbose=args.verbose,
        )
    except Exception as error:  # keep failures readable for shell callers
        print(f"tui-fixture: {error}", file=sys.stderr)
        return 1

    if args.json:
        print(json.dumps(summary, indent=2, sort_keys=True))
    else:
        counts = summary["counts"]
        print(f"workspace: {summary['workspace']}")
        for key in (
            "projects",
            "tasks",
            "activities",
            "attachments",
            "agent_jobs",
            "distinct_pinned_versions",
        ):
            print(f"{key}: {counts[key]}")
        print(f"duration: {summary['duration_seconds']}s")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
