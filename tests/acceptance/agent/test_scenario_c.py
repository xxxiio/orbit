"""Scenario C evidence: Orbit-side lifecycle with a scripted Herdr CLI; not a live model run.

This test drives the real ``orbit`` binary through the whole local agent
lifecycle (start -> status/inspect -> handoff -> review -> complete/interrupt
-> resume -> explicit task completion) against a disposable git repository.

The Herdr executable is a small Python shim placed first on ``PATH``. It only
answers the argv shapes ``HerdrRuntime`` issues (workspace/worktree create,
agent start/prompt, agent get) and records every argv for assertions. No real
Herdr daemon, no network, and no model provider is contacted, so the evidence
proves Orbit-side state transitions and command surfaces only. It does NOT
prove live Herdr integration or model behaviour; that remains the opt-in
``deploy/server/smoke-herdr-live.py`` path, which this suite never runs.

Run from the repository root after ``cargo build -p orbit-cli``::

    python3 -m pytest -q tests/acceptance/agent -vv

Set ``ORBIT_REQUIRE_AGENT_EVIDENCE=1`` (as the release-evidence runner does) to
fail, rather than skip, when the compiled binary is missing. ``ORBIT_CLI_BIN``
overrides the binary path.
"""
from __future__ import annotations

import json
import os
import sqlite3
import subprocess
import time
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parents[3]
BINARY = Path(os.environ.get("ORBIT_CLI_BIN", ROOT / "target" / "debug" / "orbit"))
REQUIRE_EVIDENCE = os.environ.get("ORBIT_REQUIRE_AGENT_EVIDENCE") == "1"

NOTE = "scripted Herdr CLI; Orbit-side lifecycle only"

HERDR_SHIM = '''#!/usr/bin/env python3
"""Scripted Herdr CLI double: records argv, answers fixed JSON, never runs an agent."""
import json
import os
import sys

args = sys.argv[1:]
log = os.environ.get("HERDR_SHIM_LOG")
if log:
    with open(log, "a", encoding="utf-8") as handle:
        handle.write(json.dumps(args) + "\\n")

if args[:1] == ["--machine"]:
    args = args[2:]
head = args[:2]
if head in (["workspace", "create"], ["worktree", "create"]):
    print(json.dumps({"result": {"root_pane": {"pane_id": "pane-1"}}}))
elif head in (["agent", "start"], ["agent", "prompt"]):
    print(json.dumps({"result": {}}))
elif head == ["agent", "get"]:
    print(json.dumps({"result": {"agent": {"status": "idle"}}}))
else:
    print(json.dumps({"result": {}}))
'''


def now_ms() -> int:
    return int(time.time() * 1000)


def evidence(message: str) -> str:
    """Label every failure with the scope of the evidence."""
    return f"[{NOTE}] {message}"


class Workspace:
    """One isolated local Orbit deployment with a PATH-shimmed Herdr."""

    def __init__(self, root: Path) -> None:
        self.root = root
        self.home = root / "home"
        self.home.mkdir(parents=True, exist_ok=True)
        self.db = root / "orbit.db"
        self.config = root / "orbit.config.toml"
        self.repo = root / "repo"
        self.shim_log = root / "herdr-argv.jsonl"
        self.shim_dir = root / "bin"
        self.shim_dir.mkdir()
        shim = self.shim_dir / "herdr"
        shim.write_text(HERDR_SHIM, encoding="utf-8")
        shim.chmod(0o755)
        env = {
            key: value
            for key, value in os.environ.items()
            if not key.startswith("ORBIT_") and key != "HERDR_SHIM_LOG"
        }
        env.update(
            {
                "HOME": str(self.home),
                "USERPROFILE": str(self.home),
                "ORBIT_CONFIG": str(self.config),
                "ORBIT_DB": str(self.db),
                "HERDR_SHIM_LOG": str(self.shim_log),
                "PATH": str(self.shim_dir) + os.pathsep + env.get("PATH", ""),
                "LANG": env.get("LANG") or "C.UTF-8",
            }
        )
        self.env = env

    # -- process helpers -------------------------------------------------
    def run(self, *arguments: str, expect: int = 0) -> subprocess.CompletedProcess[str]:
        result = subprocess.run(
            [str(BINARY), "--db", str(self.db), *arguments],
            cwd=str(ROOT),
            env=dict(self.env),
            text=True,
            capture_output=True,
            timeout=180,
        )
        if result.returncode != expect:
            pytest.fail(
                evidence(
                    f"orbit {' '.join(arguments)} exited {result.returncode}, "
                    f"expected {expect}\nstdout={result.stdout}\nstderr={result.stderr}"
                )
            )
        return result

    def json_call(self, *arguments: str, expect: int = 0) -> dict:
        result = self.run(*arguments, expect=expect)
        try:
            return json.loads(result.stdout)
        except json.JSONDecodeError as error:
            pytest.fail(
                evidence(
                    f"orbit {' '.join(arguments)} did not emit JSON: {error}\n"
                    f"stdout={result.stdout}\nstderr={result.stderr}"
                )
            )

    def authority(self, *arguments: str, expect: int = 0):
        """Agent commands answer with the shared authority envelope."""
        envelope = self.json_call(*arguments, expect=expect)
        assert envelope.get("type") == "authority", evidence(
            f"orbit {' '.join(arguments)} did not answer with an authority envelope: {envelope}"
        )
        return envelope["result"]

    # -- fixtures --------------------------------------------------------
    def init_repository(self) -> None:
        self.repo.mkdir()
        git = ["git", "-C", str(self.repo)]
        subprocess.run([*git, "init", "-q"], check=True, env=dict(self.env))
        subprocess.run([*git, "config", "user.email", "scenario-c@example.test"], check=True)
        subprocess.run([*git, "config", "user.name", "Scenario C"], check=True)
        (self.repo / "README.md").write_text("scenario c\n", encoding="utf-8")
        subprocess.run([*git, "add", "README.md"], check=True)
        subprocess.run([*git, "commit", "-qm", "initial"], check=True)

    # -- observation -----------------------------------------------------
    def shim_calls(self) -> list[list[str]]:
        if not self.shim_log.is_file():
            return []
        return [
            json.loads(line)
            for line in self.shim_log.read_text(encoding="utf-8").splitlines()
            if line.strip()
        ]

    def query(self, sql: str, parameters: tuple = ()) -> list[sqlite3.Row]:
        with sqlite3.connect(self.db) as connection:
            connection.row_factory = sqlite3.Row
            return list(connection.execute(sql, parameters))

    def job_row(self, job_id: str) -> sqlite3.Row:
        rows = self.query("SELECT * FROM agent_jobs WHERE id=?", (job_id,))
        assert len(rows) == 1, evidence(f"agent job {job_id} is missing from the workspace")
        return rows[0]

    def job_events(self, job_id: str) -> list[sqlite3.Row]:
        return self.query(
            "SELECT revision,state,reason FROM agent_job_events WHERE job_id=? ORDER BY revision",
            (job_id,),
        )

    def at_ms_after_job(self, job_id: str) -> int:
        """A timestamp that never regresses the observed job revision."""
        row = self.job_row(job_id)
        observed = [int(row["updated_at_ms"])]
        if row["last_seen_at_ms"] is not None:
            observed.append(int(row["last_seen_at_ms"]))
        return max(now_ms(), max(observed))

    def task_status(self, task_id: str) -> str:
        rows = self.query("SELECT status FROM tasks WHERE id=?", (task_id,))
        assert len(rows) == 1, evidence(f"task {task_id} is missing from the workspace")
        return str(rows[0]["status"])


@pytest.fixture()
def workspace(tmp_path: Path) -> Workspace:
    if not BINARY.is_file():
        message = evidence(
            f"compiled orbit binary missing at {BINARY}; "
            "run `cargo build -p orbit-cli` to exercise this scenario"
        )
        if REQUIRE_EVIDENCE:
            pytest.fail(
                f"{message} (ORBIT_REQUIRE_AGENT_EVIDENCE=1 forbids a skipped run)"
            )
        pytest.skip(message)
    return Workspace(tmp_path)


def test_scenario_c_local_lifecycle_with_scripted_herdr(workspace: Workspace) -> None:
    workspace.init_repository()

    project = workspace.json_call("--json", "project", "add", "Scenario C")
    project_id = project["project"]["id"]
    task = workspace.json_call(
        "--json", "task", "add", "Exercise the local agent lifecycle", "--project", project_id
    )
    task_id = task["task"]["id"]
    ready = workspace.json_call("--json", "task", "status", task_id, "ready")
    assert ready["task"]["status"] == "ready"
    assert workspace.task_status(task_id) == "ready"

    started = workspace.authority(
        "--json",
        "agent",
        "start",
        task_id,
        "--repository",
        str(workspace.repo),
        "--agent",
        "codex",
    )
    assert started["outcome"] == "confirmed", evidence(
        f"scripted Herdr did not confirm the launch: {started}; "
        f"argv={workspace.shim_calls()}"
    )
    job_id = started["job_id"]

    status = workspace.authority("--json", "agent", "status", job_id)
    assert status["state"] == "launching"
    assert status["task_id"] == task_id
    first_revision = status["revision"]

    inspected = workspace.authority(
        "--json",
        "agent",
        "inspect",
        job_id,
        "--revision",
        str(first_revision),
        "--at-ms",
        str(workspace.at_ms_after_job(job_id)),
    )
    # The shim reports an idle agent; idleness stays waiting-input, never completion.
    assert inspected["state"] == "waiting_input"
    assert inspected["revision"] > first_revision
    assert workspace.task_status(task_id) == "ready"

    handoff_file = workspace.root / "handoff.json"
    handoff_file.write_text(
        json.dumps(
            {
                "summary": "Implemented the lifecycle exercise without claiming acceptance",
                "validation": {"status": "not_run", "reason": "scripted runtime, no model run"},
                "next_steps": ["Run the real validation suite before accepting the task"],
            }
        ),
        encoding="utf-8",
    )
    handoff = workspace.authority(
        "--json", "agent", "handoff", job_id, "--file", str(handoff_file)
    )
    handoff_id = handoff["handoff_id"]
    assert handoff["saved"] is True

    review = workspace.authority(
        "--json",
        "agent",
        "review",
        job_id,
        "--handoff-id",
        handoff_id,
        "--reviewer",
        "scenario-c",
        "--decision",
        "changes_requested",
        "--reason",
        "Acceptance evidence is incomplete for a real change",
    )
    assert review["task_completed"] is False
    recorded_review = workspace.authority("--json", "agent", "review-status", job_id)
    assert recorded_review["decision"] == "changes_requested"
    assert workspace.task_status(task_id) == "ready"

    completed = workspace.authority(
        "--json",
        "agent",
        "complete",
        job_id,
        "--summary",
        "Scripted agent work captured for review",
    )
    assert completed["job_id"] == job_id
    assert completed["state"] == "completed"
    assert completed["previous_state"] == "waiting_input"
    assert completed["task_completed"] is False
    assert completed["runtime_stopped"] is False
    assert "may keep running" in completed["note"], evidence(
        f"completion result did not disclose the still-running session: {completed}"
    )
    assert completed["revision"] == inspected["revision"] + 1, evidence(
        f"completion revision {completed['revision']} did not follow "
        f"inspection revision {inspected['revision']}"
    )
    assert workspace.task_status(task_id) == "ready"

    # Handoffs and the review survive explicit job completion.
    assert completed["handoffs_preserved"] >= 1
    handoffs = workspace.authority("--json", "agent", "handoffs", job_id)
    assert handoff_id in {entry["handoff_id"] for entry in handoffs}
    assert workspace.authority("--json", "agent", "review-status", job_id)["decision"] == (
        "changes_requested"
    )

    # Terminal jobs are refused instead of being silently re-completed, and a
    # rejected command changes neither the revision nor the event list.
    events_before_rejection = [
        (row["revision"], row["state"], row["reason"])
        for row in workspace.job_events(job_id)
    ]
    revision_before_rejection = workspace.job_row(job_id)["revision"]
    second_complete = workspace.run(
        "--json", "agent", "complete", job_id, "--summary", "again", expect=2
    )
    assert "already terminal" in second_complete.stderr, evidence(second_complete.stderr)
    second_interrupt = workspace.run(
        "--json", "agent", "interrupt", job_id, "--reason", "again", expect=2
    )
    assert "already terminal" in second_interrupt.stderr, evidence(second_interrupt.stderr)
    assert workspace.job_row(job_id)["state"] == "completed"
    assert workspace.job_row(job_id)["revision"] == revision_before_rejection
    assert [
        (row["revision"], row["state"], row["reason"])
        for row in workspace.job_events(job_id)
    ] == events_before_rejection

    # Resume needs a terminal predecessor plus a durable handoff; the same
    # repository is reused and the task is untouched.
    resumed = workspace.authority("--json", "agent", "resume", job_id)
    resumed_id = resumed["job_id"]
    assert resumed_id != job_id
    assert resumed["resumed_from"] == job_id
    assert resumed["outcome"] == "confirmed", evidence(
        f"scripted Herdr did not confirm the resume: {resumed}; "
        f"argv={workspace.shim_calls()}"
    )
    resumed_status = workspace.authority("--json", "agent", "status", resumed_id)
    assert resumed_status["task_id"] == task_id
    assert resumed_status["state"] == "launching"
    assert workspace.job_row(resumed_id)["repository_ref"] == str(workspace.repo)
    assert workspace.task_status(task_id) == "ready"

    resumed_inspect = workspace.authority(
        "--json",
        "agent",
        "inspect",
        resumed_id,
        "--revision",
        str(resumed_status["revision"]),
        "--at-ms",
        str(workspace.at_ms_after_job(resumed_id)),
    )
    assert resumed_inspect["state"] == "waiting_input"

    interrupt_reason = (
        "Operator interrupted the Orbit job record; the external session may keep running"
    )
    interrupted = workspace.authority(
        "--json",
        "agent",
        "interrupt",
        resumed_id,
        "--reason",
        interrupt_reason,
    )
    assert interrupted["job_id"] == resumed_id
    assert interrupted["state"] == "interrupted"
    assert interrupted["previous_state"] == "waiting_input"
    assert interrupted["task_completed"] is False
    assert interrupted["runtime_stopped"] is False
    assert "may keep running" in interrupted["note"], evidence(
        f"interrupt result did not disclose the still-running session: {interrupted}"
    )
    assert interrupted["reason"] == interrupt_reason
    assert interrupted["revision"] == resumed_inspect["revision"] + 1, evidence(
        f"interrupt revision {interrupted['revision']} did not follow "
        f"inspection revision {resumed_inspect['revision']}"
    )
    assert workspace.task_status(task_id) == "ready"

    # The agent lifecycle never moves the task: it is still ready here.
    assert workspace.task_status(task_id) == "ready"

    # Only explicit operator transitions complete the task. The domain requires
    # ready -> in-progress -> done, so acceptance takes both explicit steps and
    # happens after, never during, the agent lifecycle.
    started_task = workspace.json_call("--json", "task", "start", task_id)
    assert started_task["task"]["status"] == "in_progress"
    finished = workspace.json_call("--json", "task", "complete", task_id)
    assert finished["task"]["status"] == "done"
    assert workspace.task_status(task_id) == "done"
    assert workspace.job_row(job_id)["state"] == "completed"
    assert workspace.job_row(resumed_id)["state"] == "interrupted"

    # Every lifecycle transition is durable, exact and unique per revision.
    def event_timeline(identifier: str) -> tuple[list[str], list[int]]:
        rows = workspace.job_events(identifier)
        return (
            [row["state"] for row in rows],
            [row["revision"] for row in rows],
        )

    states, revisions = event_timeline(job_id)
    assert states == [
        "queued",
        "preparing",
        "launching",
        "waiting_input",
        "completed",
    ], evidence(f"unexpected transition timeline for {job_id}: {states}")
    assert revisions == [0, 1, 2, 3, 4], evidence(f"unexpected revisions for {job_id}: {revisions}")
    assert len(revisions) == len(set(revisions)), evidence(
        f"duplicate revisions for {job_id}: {revisions}"
    )

    resumed_states, resumed_revisions = event_timeline(resumed_id)
    assert resumed_states == [
        "queued",
        "preparing",
        "launching",
        "waiting_input",
        "interrupted",
    ], evidence(f"unexpected transition timeline for {resumed_id}: {resumed_states}")
    assert resumed_revisions == [0, 1, 2, 3, 4], evidence(
        f"unexpected revisions for {resumed_id}: {resumed_revisions}"
    )
    assert len(resumed_revisions) == len(set(resumed_revisions)), evidence(
        f"duplicate revisions for {resumed_id}: {resumed_revisions}"
    )

    for identifier in (job_id, resumed_id):
        commands = workspace.query(
            "SELECT command_kind,resulting_state FROM agent_job_commands WHERE job_id=? "
            "ORDER BY resulting_revision",
            (identifier,),
        )
        assert [(row["command_kind"], row["resulting_state"]) for row in commands] == [
            ("prepare", "preparing")
        ], evidence(
            f"unexpected command receipts for {identifier}: {[dict(row) for row in commands]}"
        )

    # The resume brief froze the predecessor handoff and the same repository.
    brief_rows = workspace.query(
        "SELECT brief_json FROM agent_job_briefs WHERE job_id=?", (resumed_id,)
    )
    assert len(brief_rows) == 1, evidence(f"resume brief missing for {resumed_id}")
    brief = json.loads(brief_rows[0]["brief_json"])
    assert brief["resume"]["prior_job_id"] == job_id
    assert brief["resume"]["handoff"]["summary"].startswith("Implemented the lifecycle")
    assert brief["launch_revision"] == 1

    # Only the two jobs created by this scenario exist.
    jobs = workspace.query("SELECT id FROM agent_jobs ORDER BY created_at_ms,id")
    assert [row["id"] for row in jobs] == [job_id, resumed_id], evidence(
        f"unexpected agent jobs in the workspace: {[row['id'] for row in jobs]}"
    )

    # Two launches mean exactly two workspace creates, starts and prompts, plus
    # one inspection for each job.
    calls = workspace.shim_calls()
    creates = [call for call in calls if call[:2] == ["workspace", "create"]]
    worktrees = [call for call in calls if call[:2] == ["worktree", "create"]]
    starts = [call for call in calls if call[:2] == ["agent", "start"]]
    prompts = [call for call in calls if call[:2] == ["agent", "prompt"]]
    inspections = [call for call in calls if call[:2] == ["agent", "get"]]
    assert len(creates) == 2, evidence(f"expected two workspace creates: {calls}")
    assert not worktrees, evidence(f"worktree create was not requested: {worktrees}")
    assert len(starts) == 2, evidence(f"expected two agent starts: {calls}")
    assert len(prompts) == 2, evidence(f"expected two agent prompts: {calls}")
    assert len(inspections) == 2, evidence(f"expected one inspect per job: {calls}")

    prompt_text = " \n".join(" ".join(call[3:]) for call in prompts)
    assert job_id in prompt_text, evidence(f"initial prompt missing: {prompts}")
    assert resumed_id in prompt_text, evidence(f"resume prompt missing: {prompts}")
    assert "Implemented the lifecycle exercise without claiming acceptance" in prompt_text, (
        evidence(f"resume prompt did not freeze the predecessor handoff: {prompts}")
    )
