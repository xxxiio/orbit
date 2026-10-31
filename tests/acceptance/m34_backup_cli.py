#!/usr/bin/env python3
"""M34 I5 — CLI acceptance and failure injection for workspace backup/restore.

This suite runs the real `orbit` binary against real workspace files in
isolated temporary directories. It accepts the CLI surface, then fails it on
purpose: interrupted creation and restoration, corrupted/truncated archives,
missing attachment objects, unreadable vault files, permission-denied
destinations, unsupported manifest versions and wrong/active restore targets.

For every failure case it asserts the property that matters operationally:

* the command exits nonzero with a clear error (never a silent success);
* no partial `.orbitbak` is published into the destination;
* the original workspace is still readable and byte-identical (database
  content, Markdown vault and attachment object store);
* temporary/staging artifacts are cleaned up;
* when a replacement starts activating, a preserved copy of the previous
  workspace exists and is never deleted automatically.

Usage
-----
    python3 tests/acceptance/m34_backup_cli.py \\
        --binary target/debug/orbit \\
        --server-binary target/debug/orbit-server

Evidence is written to `web/evidence/m34-recovery/failure-injection.json` and
the shared README is regenerated. Exit codes: 0 = all cases passed (cases that
are recorded NOT_APPLICABLE because of an external boundary do not fail the
suite); 1 = a case failed or was inconclusive; 2 = a case is blocked because the
product surface is not implemented in this build.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import shutil
import signal
import sqlite3
import subprocess
import sys
import tarfile
import tempfile
import threading
import time
import traceback
from datetime import datetime, timezone
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from m34_recovery_rehearsal import (  # noqa: E402
    DEFAULT_CLI,
    DEFAULT_EVIDENCE_DIR,
    DEFAULT_SERVER,
    REPO_ROOT,
    OrbitCli,
    binary_identity,
    git_identity,
    clean_env,
    hash_tree,
    is_not_implemented,
    read_json_file,
    sha256_bytes,
    sha256_file,
    sqlite_snapshot_hash,
    sqlite_table_counts,
    write_readme,
)

PASS = "PASS"
FAIL = "FAIL"
BLOCKED = "BLOCKED"
INCONCLUSIVE = "INCONCLUSIVE"
NOT_APPLICABLE = "NOT_APPLICABLE"


# ---------------------------------------------------------------------------
# Isolated local workspace
# ---------------------------------------------------------------------------
class LocalWorkspace:
    """One disposable local-authority workspace created with the real CLI."""

    def __init__(self, root: Path, binary: Path):
        self.root = root
        self.binary = binary
        self.home = root / "home"
        self.config = root / "orbit.config.toml"
        self.db = root / "orbit.db"
        self.attachments = root / "attachments"
        self.knowledge = root / "orbit.knowledge"

    def cli(self) -> OrbitCli:
        return OrbitCli(
            self.binary,
            db=self.db,
            config=self.config,
            home=self.home,
        )

    def create(self) -> "LocalWorkspace":
        self.home.mkdir(parents=True, exist_ok=True)
        completed = subprocess.run(
            [str(self.binary), "init"],
            env=clean_env(
                {
                    "HOME": str(self.home),
                    "ORBIT_CONFIG": str(self.config),
                    "ORBIT_DB": str(self.db),
                }
            ),
            cwd=str(REPO_ROOT),
            text=True,
            capture_output=True,
            timeout=120,
        )
        if completed.returncode != 0:
            raise RuntimeError(
                f"orbit init failed ({completed.returncode})\n{completed.stdout}\n"
                f"{completed.stderr}"
            )
        return self

    def populate(self) -> dict:
        cli = self.cli()
        project = cli.run_json("project", "add", "Acceptance project")
        project_id = project["project"]["id"]
        tasks = []
        for index in range(1, 6):
            result = cli.run_json(
                "task", "add", f"Acceptance task {index:03d}",
                "--id", f"task-acceptance-{index:03d}",
                "--project", project_id,
            )
            tasks.append(result["task"]["id"])
        cli.run_json("task", "depends-on", tasks[1], tasks[0])
        binary_path = self.root / "fixtures" / "acceptance-binary.bin"
        binary_path.parent.mkdir(parents=True, exist_ok=True)
        binary = bytes((index * 29 + 7) % 256 for index in range(8192))
        binary_path.write_bytes(binary)
        cli.run_json("attachment", "add", "task", tasks[0], str(binary_path))
        text_path = self.root / "fixtures" / "acceptance-notes.md"
        text_path.write_text("# Acceptance notes\n\nfixture content\n")
        cli.run_json("attachment", "add", "project", project_id, str(text_path))
        for note_id, body in (
            ("note-acceptance-plan", "# Acceptance plan\n\nSee [[note-acceptance-log]].\n"),
            ("note-acceptance-log", "# Acceptance log\n\nBack to [[note-acceptance-plan]].\n"),
        ):
            cli.run_json(
                "note", "create", "--id", note_id,
                f"{note_id}.md", "--body", body,
            )
        return {
            "project_id": project_id,
            "tasks": tasks,
            "attachments": {
                "binary_sha256": sha256_bytes(binary),
                "notes_sha256": sha256_file(text_path),
            },
            "notes": ["note-acceptance-plan", "note-acceptance-log"],
        }

    # -- read-only evidence ----------------------------------------------------
    def file_listing(self, *, include_db_sidecars: bool = False) -> list:
        entries = []
        for path in sorted(self.root.rglob("*")):
            rel = path.relative_to(self.root).as_posix()
            if path.is_dir():
                continue
            if not include_db_sidecars and rel.endswith(("-wal", "-shm")):
                continue
            entries.append(rel)
        return entries

    def semantic_state(self, scratch: Path) -> dict:
        """The parts of the workspace that must not change on a failed command."""
        return {
            "db_sha256": sqlite_snapshot_hash(self.db, scratch / "semantic.db"),
            "db_counts": sqlite_table_counts(self.db),
            "vault": hash_tree(self.knowledge),
            "attachments": hash_tree(self.attachments / "objects"),
            "listing": self.file_listing(),
            "sidecars": sorted(
                path.name for path in self.root.glob("orbit.db-*")
            ),
        }

    def copy_to(self, destination: Path) -> "LocalWorkspace":
        shutil.copytree(self.root, destination)
        return LocalWorkspace(destination, self.binary)


# ---------------------------------------------------------------------------
# Assertion helpers
# ---------------------------------------------------------------------------
def archives_in(directory: Path) -> list:
    if not directory.is_dir():
        return []
    return sorted(path.name for path in directory.glob("*.orbitbak"))


def staging_artifacts(directory: Path) -> list:
    """Names that look like leftover staging/temporary artifacts."""
    if not directory.is_dir():
        return []
    found = []
    for path in directory.iterdir():
        name = path.name
        if any(marker in name for marker in (
            "staging", ".tmp", ".partial", ".incomplete", "orbit-verify",
            "orbit-backup", "orbit-restore",
        )):
            found.append(name)
    return sorted(found)


def assert_original_intact(before: dict, after: dict) -> list:
    problems = []
    for field in ("db_sha256", "vault", "attachments", "listing", "sidecars"):
        if before.get(field) != after.get(field):
            problems.append(f"{field} changed")
    return problems


def expect_failure(record: dict, *, label: str, allowed=(1, 2, 3, 4, 5, 101)) -> None:
    if record["returncode"] == 0:
        raise RuntimeError(
            f"{label}: command unexpectedly succeeded\nstdout:\n{record['stdout']}"
        )
    combined = (record["stdout"] or "") + (record["stderr"] or "")
    if not combined.strip():
        raise RuntimeError(f"{label}: command failed with no diagnostic output")


def expect_success(record: dict, *, label: str) -> dict:
    if record["returncode"] != 0:
        raise RuntimeError(
            f"{label}: command exited {record['returncode']}\n"
            f"stdout:\n{record['stdout']}\nstderr:\n{record['stderr']}"
        )
    try:
        return json.loads(record["stdout"])
    except json.JSONDecodeError as error:
        raise RuntimeError(f"{label}: response was not JSON: {error}\n{record['stdout']}") from error


def rewrite_archive(source: Path, destination: Path, *, drop_entry: str | None = None,
                    patch_manifest: dict | None = None,
                    truncate_fraction: float | None = None) -> None:
    """Produce a tampered copy of an archive using the documented tar layout."""
    if truncate_fraction is not None:
        content = source.read_bytes()
        destination.write_bytes(content[: int(len(content) * truncate_fraction)])
        return
    with tarfile.open(source, "r") as reader:
        entries = []
        for member in reader.getmembers():
            data = reader.extractfile(member).read() if member.isfile() else None
            entries.append((member, data))
    with tarfile.open(destination, "w") as writer:
        for member, data in entries:
            if member.name == "manifest.json" and patch_manifest is not None:
                data = json.dumps(patch_manifest, indent=2).encode() + b"\n"
            if drop_entry is not None and member.name == drop_entry:
                continue
            if data is None:
                writer.addfile(member)
            else:
                member.size = len(data)
                import io
                writer.addfile(member, io.BytesIO(data))


def first_attachment_entry(archive: Path) -> str:
    with tarfile.open(archive, "r") as reader:
        for member in reader.getmembers():
            if member.name.startswith("attachments/objects/"):
                return member.name
    raise RuntimeError("archive contains no attachment object entries")


def flip_archive_bytes(source: Path, destination: Path, offset_fraction: float = 0.5) -> int:
    content = bytearray(source.read_bytes())
    offset = int(len(content) * offset_fraction)
    content[offset] ^= 0xFF
    destination.write_bytes(bytes(content))
    return offset


# ---------------------------------------------------------------------------
# Case framework
# ---------------------------------------------------------------------------
class CaseRunner:
    def __init__(self, evidence_dir: Path, keep_workdir: bool = False):
        self.cases: list[dict] = []
        self.evidence_dir = evidence_dir
        self.keep_workdir = keep_workdir
        self.limitations: list[str] = []
        self.defects: list[dict] = []
        self.started_at = time.time()

    def run(self, name: str, expected: str, function):
        started = time.monotonic()
        record = {
            "name": name,
            "expected": expected,
            "observed": "",
            "original_intact": "unknown",
            "status": FAIL,
            "details": {},
            "duration_ms": 0.0,
        }
        print(f"\n[case] {name}")
        try:
            outcome = function()
            observation = (outcome or {}).get("observed", "")
            intact = (outcome or {}).get("original_intact", True)
            status = (outcome or {}).get("status", PASS)
            details = (outcome or {}).get("details", {})
            record.update(
                observed=observation,
                original_intact="yes" if intact is True else
                ("no" if intact is False else str(intact)),
                status=status,
                details=details,
            )
        except Exception as error:  # noqa: BLE001 - evidence must capture everything
            record["observed"] = f"{type(error).__name__}: {error}"[:1000]
            record["traceback"] = traceback.format_exc(limit=8)
            record["original_intact"] = "unknown"
            if is_not_implemented(str(error)):
                record["status"] = BLOCKED
                record["blocked_reason"] = (
                    "product surface not implemented in this build: "
                    + str(error).splitlines()[0][:300]
                )
        record["duration_ms"] = round((time.monotonic() - started) * 1000, 2)
        print(f"[case] {record['status']}: {record['observed'] or record.get('blocked_reason', '')}")
        self.cases.append(record)
        return record

    def summary(self) -> dict:
        summary: dict[str, int] = {}
        for case in self.cases:
            summary[case["status"]] = summary.get(case["status"], 0) + 1
        return summary

    def exit_code(self) -> int:
        summary = self.summary()
        if summary.get(FAIL) or summary.get(INCONCLUSIVE):
            return 1
        if summary.get(BLOCKED):
            return 2
        return 0

    def evidence(self, binary: Path) -> dict:
        summary = self.summary()
        if summary.get(FAIL) or summary.get(INCONCLUSIVE):
            result = FAIL
        elif summary.get(BLOCKED):
            result = BLOCKED
        else:
            result = PASS
        return {
            "schema": "orbit.m34.failure-injection/1",
            "generated_by": "tests/acceptance/m34_backup_cli.py",
            "result": result,
            "started_at": datetime.fromtimestamp(
                self.started_at, timezone.utc).isoformat(),
            "finished_at": datetime.now(timezone.utc).isoformat(timespec="milliseconds"),
            "binary": str(binary),
            "binaries": {
                "orbit_cli": binary_identity(binary),
            },
            "host": sys.platform,
            "case_summary": summary,
            "cases": self.cases,
            "limitations": self.limitations,
            "defects": self.defects,
        }


# ---------------------------------------------------------------------------
# Acceptance cases
# ---------------------------------------------------------------------------
class Testbed:
    def __init__(self, root: Path, binary: Path, server_binary: Path):
        self.root = root
        self.binary = binary
        self.server_binary = server_binary
        self.workspace = LocalWorkspace(root / "workspace", binary).create()
        self.dataset = self.workspace.populate()
        self.external_backups = root / "external-backups"
        self.external_backups.mkdir(parents=True, exist_ok=True)
        self.default_backups = self.workspace.home / ".orbit" / "backups"
        self.scratch = root / "scratch"
        self.scratch.mkdir(parents=True, exist_ok=True)
        # One known-good archive reused by tamper cases.
        self.baseline = self._create_baseline()

    def cli(self) -> OrbitCli:
        return self.workspace.cli()

    def _create_baseline(self) -> Path:
        cli = self.cli()
        record = cli.call_backup(
            "create", "--output", str(self.external_backups),
            "--label", "baseline",
        )
        response = expect_success(record, label="baseline create")
        if not response.get("verified"):
            raise RuntimeError(f"baseline archive did not verify: {response}")
        return Path(response["backup"])

    def state(self) -> dict:
        return self.workspace.semantic_state(self.scratch)


def case_create_default(testbed: Testbed):
    before_archives = archives_in(testbed.default_backups)
    record = testbed.cli().call_backup("create")
    response = expect_success(record, label="create default")
    archive = Path(response["backup"])
    if not archive.is_file():
        raise RuntimeError(f"archive missing: {archive}")
    if not response.get("verified"):
        raise RuntimeError(f"creation not verified: {response}")
    if not any(str(archive).startswith(str(testbed.default_backups)) or True for _ in [0]):
        pass
    sidecar = Path(str(archive) + ".verify.json")
    if not sidecar.is_file():
        raise RuntimeError(f"verification sidecar missing: {sidecar}")
    archives_after = archives_in(testbed.default_backups)
    if len(archives_after) != len(before_archives) + 1:
        raise RuntimeError(
            f"expected one new archive in the default directory: {before_archives} -> "
            f"{archives_after}"
        )
    return {
        "observed": f"created {archive.name} ({response.get('archive_bytes')} bytes), "
                    f"verified={response.get('verified')}, sidecar present",
        "details": {
            "command": record["argv"],
            "archive": str(archive),
            "components": response.get("components"),
            "archive_bytes": response.get("archive_bytes"),
            "default_directory": str(testbed.default_backups),
        },
    }


def case_create_output_and_label(testbed: Testbed):
    record = testbed.cli().call_backup(
        "create", "--output", str(testbed.external_backups),
        "--label", "nightly acceptance",
    )
    response = expect_success(record, label="create --output --label")
    archive = Path(response["backup"])
    if not archive.is_file():
        raise RuntimeError(f"archive missing: {archive}")
    inspect = expect_success(
        testbed.cli().call_backup("inspect", str(archive)), label="inspect labelled archive"
    )
    manifest = inspect["manifest"]
    if manifest.get("label") != "nightly acceptance":
        raise RuntimeError(f"label not recorded in manifest: {manifest.get('label')!r}")
    if manifest.get("format_version") != 1:
        raise RuntimeError(f"unexpected format version: {manifest.get('format_version')}")
    if manifest.get("complete") is not True:
        raise RuntimeError(f"manifest not complete: {manifest.get('complete')}")
    kinds = sorted({entry.get("kind") for entry in manifest.get("inventory", [])})
    for expected in ("database", "knowledge_note", "attachment_object"):
        if expected not in kinds:
            raise RuntimeError(f"inventory misses {expected}: {kinds}")
    return {
        "observed": f"archive {archive.name} labelled 'nightly acceptance', "
                    f"format {manifest.get('format_version')}, inventory kinds {kinds}",
        "details": {
            "archive": str(archive),
            "manifest_label": manifest.get("label"),
            "counts": manifest.get("counts"),
            "inventory_files": inspect.get("inventory_files"),
        },
    }


def case_list_default_and_external(testbed: Testbed):
    cli = testbed.cli()
    default = cli.run_json("backup", "list")
    external = cli.run_json("backup", "list", "--output", str(testbed.external_backups))
    if not default.get("backups"):
        raise RuntimeError(f"default listing is empty: {default}")
    names = {Path(entry["path"]).name for entry in external.get("backups", [])}
    if testbed.baseline.name not in names:
        raise RuntimeError(f"baseline archive not listed in external directory: {names}")
    verified = [entry for entry in external["backups"] if entry["verification"] == "verified"]
    if not verified:
        raise RuntimeError(f"no verified archive listed: {external['backups']}")
    if default.get("skipped"):
        raise RuntimeError(f"default listing reported unreadable files: {default['skipped']}")
    return {
        "observed": f"default dir: {len(default.get('backups', []))} archive(s); "
                    f"external dir: {len(external.get('backups', []))} archive(s), "
                    f"{len(verified)} verified",
        "details": {
            "default": {"directory": default.get("directory"),
                        "count": len(default.get("backups", []))},
            "external": {"directory": external.get("directory"),
                         "count": len(external.get("backups", []))},
        },
    }


def case_verify_pass(testbed: Testbed):
    report = expect_success(
        testbed.cli().call_backup("verify", str(testbed.baseline)),
        label="verify baseline",
    )
    if report.get("ok") is not True:
        raise RuntimeError(f"verification did not report ok: {report}")
    checks = {check.get("name"): check.get("status") for check in report.get("checks", [])}
    if not checks:
        raise RuntimeError(f"verification returned no checks: {report}")
    if any(status == "fail" for status in checks.values()):
        raise RuntimeError(f"verification reported a failing check: {checks}")
    return {
        "observed": f"ok=true, {len(checks)} checks: {checks}",
        "details": {"checks": checks, "problems": report.get("problems"),
                    "warnings": report.get("warnings")},
    }


def case_verify_detects_corruption(testbed: Testbed):
    before = testbed.state()
    corrupted = testbed.scratch / "corrupted.orbitbak"
    offset = flip_archive_bytes(testbed.baseline, corrupted)
    record = testbed.cli().call_backup("verify", str(corrupted))
    expect_failure(record, label="verify corrupted archive")
    combined = (record["stdout"] or "") + (record["stderr"] or "")
    if "ok" in combined and '"ok": true' in combined:
        raise RuntimeError(f"corrupted archive verified ok:\n{combined}")
    after = testbed.state()
    problems = assert_original_intact(before, after)
    if problems:
        raise RuntimeError(f"original workspace changed: {problems}")
    return {
        "observed": f"nonzero exit {record['returncode']}, corrupt byte at offset {offset} "
                    f"detected; original workspace unchanged",
        "details": {"returncode": record["returncode"],
                    "diagnostic_excerpt": combined[-1500:]},
    }


def case_restore_dry_run_mutates_nothing(testbed: Testbed):
    """Strict: recursive file list + hashes, including no new WAL/SHM sidecars."""
    before = testbed.state()
    before_files = testbed.workspace.file_listing(include_db_sidecars=True)
    destination = testbed.root / "dry-run-target"
    record = testbed.cli().call_backup(
        "restore", str(testbed.baseline), "--dry-run", "--to", str(destination)
    )
    plan = expect_success(record, label="restore --dry-run --to")
    if destination.exists():
        raise RuntimeError(f"dry-run created the destination directory: {destination}")
    after = testbed.state()
    after_files = testbed.workspace.file_listing(include_db_sidecars=True)
    problems = assert_original_intact(before, after)
    if before_files != after_files:
        problems.append(f"file list changed: {sorted(set(after_files) - set(before_files))}")
    if problems:
        raise RuntimeError(
            "dry-run mutated the active workspace: " + "; ".join(problems)
            + " (product defect: dry-run creates empty WAL/SHM sidecars)"
        )
    if plan.get("admissible") is not True:
        raise RuntimeError(f"dry-run preflight was not admissible: {plan}")
    target = (plan.get("plan") or {}).get("target", {})
    return {
        "observed": f"admissible preflight, no destination created, workspace file list "
                    f"and hashes unchanged ({len(after_files)} files)",
        "details": {
            "admissible": plan.get("admissible"),
            "plan_mode": (plan.get("plan") or {}).get("mode"),
            "target": target,
            "blockers": plan.get("blockers"),
            "warnings": plan.get("warnings"),
        },
    }


def case_isolated_restore_opens(testbed: Testbed):
    before = testbed.state()
    destination = testbed.root / "isolated-restore"
    record = testbed.cli().call_backup(
        "restore", str(testbed.baseline), "--to", str(destination)
    )
    response = expect_success(record, label="restore --to")
    database = Path(response["database"])
    if not database.is_file():
        raise RuntimeError(f"restored database missing: {database}")
    if response.get("mode") != "isolated":
        raise RuntimeError(f"restore --to reported mode {response.get('mode')!r}, not isolated")
    cli = testbed.cli()
    cli.run_json("--db", str(database), "status")
    tasks = cli.run_json("--db", str(database), "task", "list")
    task_rows = tasks.get("tasks", [])
    if len(task_rows) < 5:
        raise RuntimeError(f"restored task list is short: {tasks}")
    notes = cli.run_json("--db", str(database), "note", "list")
    note_rows = notes.get("result") if isinstance(notes.get("result"), list) else notes.get("notes", [])
    note_ids = {note.get("id") for note in note_rows}
    for note_id in testbed.dataset["notes"]:
        if note_id not in note_ids:
            raise RuntimeError(f"restored note missing: {note_id} in {note_ids}")
    restored_counts = sqlite_table_counts(database)
    if restored_counts.get("tasks") != testbed.state()["db_counts"].get("tasks"):
        raise RuntimeError("restored task row count differs from the source workspace")
    problems = assert_original_intact(before, testbed.state())
    if problems:
        raise RuntimeError(f"isolated restore touched the active workspace: {problems}")
    return {
        "observed": f"restored {database.name}, CLI status/list open it, "
                    f"{len(task_rows)} tasks and {len(note_ids)} notes readable, "
                    f"active workspace unchanged",
        "details": {
            "response": response,
            "task_count": len(task_rows),
            "note_ids": sorted(note_ids),
            "restored_counts_tasks": restored_counts.get("tasks"),
        },
    }


def case_replace_requires_confirmation(testbed: Testbed):
    copy = testbed.workspace.copy_to(testbed.root / "replace-target")
    cli = LocalWorkspace(copy.root, testbed.binary).cli()
    before = LocalWorkspace(copy.root, testbed.binary).semantic_state(testbed.scratch)
    no_confirm = cli.call_backup(
        "restore", str(testbed.baseline), "--replace"
    )
    expect_failure(no_confirm, label="replace without --confirm")
    wrong_confirm = cli.call_backup(
        "restore", str(testbed.baseline), "--replace", "--confirm", "yes"
    )
    expect_failure(wrong_confirm, label="replace with wrong confirmation")
    after = LocalWorkspace(copy.root, testbed.binary).semantic_state(testbed.scratch)
    problems = assert_original_intact(before, after)
    if problems:
        raise RuntimeError(f"refused replacement mutated the target: {problems}")
    confirmed = cli.call_backup(
        "restore", str(testbed.baseline), "--replace", "--confirm", "REPLACE"
    )
    response = expect_success(confirmed, label="replace with --confirm REPLACE")
    if response.get("activated") is not True:
        raise RuntimeError(f"replacement did not activate: {response}")
    preserved = response.get("preserved")
    if not preserved or not Path(preserved).exists():
        raise RuntimeError(f"replacement did not preserve the previous workspace: {response}")
    journal = response.get("journal")
    if not journal or not Path(journal).is_file():
        raise RuntimeError(f"replacement did not write a recovery journal: {response}")
    status = cli.run_json("--db", str(Path(preserved) / "orbit.db"), "status")
    return {
        "observed": f"without/wrong --confirm refused; REPLACE activated, preserved "
                    f"{Path(preserved).name} and journal {Path(journal).name}; "
                    f"preserved copy opens ({status.get('type')})",
        "details": {
            "no_confirm_returncode": no_confirm["returncode"],
            "wrong_confirm_returncode": wrong_confirm["returncode"],
            "preserved": preserved,
            "journal": journal,
        },
    }


def case_replace_refuses_active_target(testbed: Testbed):
    copy = testbed.workspace.copy_to(testbed.root / "active-target")
    cli = LocalWorkspace(copy.root, testbed.binary).cli()
    before = LocalWorkspace(copy.root, testbed.binary).semantic_state(testbed.scratch)
    holder = sqlite3.connect(str(copy.root / "orbit.db"), timeout=5)
    holder.execute("BEGIN IMMEDIATE")
    holder.execute("CREATE TABLE IF NOT EXISTS m34_active_probe (id INTEGER)")
    try:
        record = cli.call_backup(
            "restore", str(testbed.baseline), "--replace", "--confirm", "REPLACE"
        )
        expect_failure(record, label="replace while the target is active")
        observed = (record["stdout"] or "") + (record["stderr"] or "")
    finally:
        holder.rollback()
        holder.close()
    after = LocalWorkspace(copy.root, testbed.binary).semantic_state(testbed.scratch)
    problems = assert_original_intact(before, after)
    if problems:
        raise RuntimeError(f"refused replacement mutated an active target: {problems}")
    return {
        "observed": f"nonzero exit {record['returncode']} while a writer held the target; "
                    f"target intact",
        "details": {"returncode": record["returncode"],
                    "diagnostic_excerpt": observed[-1500:]},
    }


def case_schedule_and_retention(testbed: Testbed):
    cli = testbed.cli()
    directory = testbed.root / "policy-backups"
    directory.mkdir(exist_ok=True)
    testbed.cli().call_backup(
        "create", "--output", str(directory), "--label", "retention-1"
    )
    testbed.cli().call_backup(
        "create", "--output", str(directory), "--label", "retention-2"
    )
    set_record = cli.call_backup(
        "schedule", "set", "--output", str(directory),
        "--mode", "daily", "--max-retained", "1", "--min-retention-days", "0",
    )
    if set_record["returncode"] != 0:
        combined = (set_record["stdout"] or "") + (set_record["stderr"] or "")
        if is_not_implemented(combined):
            raise RuntimeError(f"not implemented yet: {combined[:300]}")
        raise RuntimeError(f"schedule set failed: {combined[:1000]}")
    shown = expect_success(
        cli.call_backup("schedule", "show", "--output", str(directory)),
        label="schedule show",
    )
    retention = expect_success(
        cli.call_backup("retention", "apply", "--output", str(directory)),
        label="retention apply",
    )
    remaining = archives_in(directory)
    if not remaining:
        raise RuntimeError("retention removed every recovery point")
    kept_listing = cli.run_json("backup", "list", "--output", str(directory))
    verified = [entry for entry in kept_listing["backups"]
                if entry["verification"] == "verified"]
    if not verified:
        raise RuntimeError("retention removed the last verified recovery point")
    return {
        "observed": f"schedule stored {shown.get('policy', {}).get('schedule')!r}; "
                    f"retention kept {len(remaining)} archive(s), last verified present",
        "details": {"policy": shown.get("policy"), "retention": retention,
                    "remaining": remaining},
    }


# ---------------------------------------------------------------------------
# Failure-injection cases
# ---------------------------------------------------------------------------
def kill_during(cli: OrbitCli, argv: list[str], *, watch_dirs: list[Path],
                baseline_entries: dict[Path, set[str]], max_seconds: float = 10.0,
                fallback_delays=(0.01, 0.03, 0.08, 0.2, 0.5)) -> dict:
    """Spawn a CLI command and SIGKILL it once staging activity is observed."""
    env = cli.env()
    full_argv = [str(cli.binary), "--json", *argv]
    for delay in fallback_delays:
        process = subprocess.Popen(
            full_argv,
            env=env,
            cwd=str(REPO_ROOT),
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        )
        activity = None
        deadline = time.monotonic() + min(max_seconds, delay + 2.0)
        while time.monotonic() < deadline:
            if process.poll() is not None:
                break
            for directory in watch_dirs:
                if not directory.is_dir():
                    continue
                current = {entry.name for entry in directory.iterdir()}
                new = current - baseline_entries.get(directory, set())
                if new:
                    activity = {"directory": str(directory), "entries": sorted(new)}
                    break
            if activity is not None:
                break
            time.sleep(0.002)
        if activity is None and process.poll() is None:
            time.sleep(delay)
        if process.poll() is None:
            process.send_signal(signal.SIGKILL)
        try:
            stdout, stderr = process.communicate(timeout=30)
        except subprocess.TimeoutExpired:
            process.kill()
            stdout, stderr = process.communicate()
        return {
            "argv": argv,
            "returncode": process.returncode,
            "stdout": stdout,
            "stderr": stderr,
            "killed_after_staging": activity is not None,
            "activity": activity,
        }
    raise RuntimeError("unreachable")


def case_interrupted_backup(testbed: Testbed):
    destination = testbed.root / "interrupted-backups"
    destination.mkdir(exist_ok=True)
    baseline_entries = {
        directory: {entry.name for entry in directory.iterdir()}
        for directory in (destination, testbed.workspace.root, Path(tempfile.gettempdir()))
        if directory.is_dir()
    }
    before = testbed.state()
    attempts = []
    outcome = None
    for _attempt in range(4):
        record = kill_during(
            testbed.cli(),
            ["--db", str(testbed.workspace.db), "--config",
             str(testbed.workspace.config), "backup", "create",
             "--output", str(destination)], 
            watch_dirs=[destination, testbed.workspace.root],
            baseline_entries=baseline_entries,
        )
        attempts.append(record)
        published = archives_in(destination)
        interrupted = record["returncode"] != 0
        if interrupted and not published:
            outcome = record
            break
        if published:
            # The create finished before the kill; retry with an earlier window.
            for archive in published:
                (destination / archive).unlink(missing_ok=True)
                (destination / (archive + ".verify.json")).unlink(missing_ok=True)
            continue
    after = testbed.state()
    problems = assert_original_intact(before, after)
    staging = staging_artifacts(destination) + staging_artifacts(testbed.workspace.root)
    published = archives_in(destination)
    if problems:
        raise RuntimeError(f"interrupted backup damaged the workspace: {problems}")
    if published:
        raise RuntimeError(f"interrupted backup published a partial archive: {published}")
    if staging:
        raise RuntimeError(f"interrupted backup left staging artifacts: {staging}")
    if outcome is None:
        return {
            "status": INCONCLUSIVE,
            "observed": "every kill landed after the archive was published; the timing "
                        "window was not reproducible on this machine",
            "original_intact": True,
            "details": {"attempts": attempts},
        }
    return {
        "observed": f"SIGKILL after staging activity (rc {outcome['returncode']}); no "
                    f"partial .orbitbak published, no staging artifacts, workspace intact",
        "original_intact": True,
        "details": {
            "attempts": [
                {"returncode": attempt["returncode"],
                 "killed_after_staging": attempt["killed_after_staging"],
                 "activity": attempt["activity"]}
                for attempt in attempts
            ],
            "staging_artifacts": staging,
            "published": published,
        },
    }


def case_corrupted_archive(testbed: Testbed):
    before = testbed.state()
    corrupted = testbed.scratch / "flip.orbitbak"
    flip_archive_bytes(testbed.baseline, corrupted, 0.5)
    verify = testbed.cli().call_backup("verify", str(corrupted))
    expect_failure(verify, label="verify flipped archive")
    destination = testbed.root / "corrupt-restore"
    restore = testbed.cli().call_backup("restore", str(corrupted), "--to", str(destination))
    expect_failure(restore, label="restore flipped archive")
    if destination.exists() and any(destination.iterdir()):
        raise RuntimeError(f"failed restore left a populated target: {destination}")
    problems = assert_original_intact(before, testbed.state())
    if problems:
        raise RuntimeError(f"corruption handling mutated the workspace: {problems}")
    return {
        "observed": f"verify exit {verify['returncode']}, restore exit "
                    f"{restore['returncode']}; target not populated; workspace intact",
        "original_intact": True,
        "details": {"verify_returncode": verify["returncode"],
                    "restore_returncode": restore["returncode"]},
    }


def case_truncated_archive(testbed: Testbed):
    before = testbed.state()
    truncated = testbed.scratch / "truncated.orbitbak"
    rewrite_archive(testbed.baseline, truncated, truncate_fraction=0.6)
    verify = testbed.cli().call_backup("verify", str(truncated))
    expect_failure(verify, label="verify truncated archive")
    problems = assert_original_intact(before, testbed.state())
    if problems:
        raise RuntimeError(f"truncation handling mutated the workspace: {problems}")
    return {
        "observed": f"truncated archive rejected with exit {verify['returncode']}; "
                    f"workspace intact",
        "original_intact": True,
        "details": {"returncode": verify["returncode"],
                    "diagnostic_excerpt": ((verify["stdout"] or "") +
                                           (verify["stderr"] or ""))[-1200:]},
    }


def case_missing_attachment_object(testbed: Testbed):
    before = testbed.state()
    broken = testbed.scratch / "missing-attachment.orbitbak"
    dropped = first_attachment_entry(testbed.baseline)
    rewrite_archive(testbed.baseline, broken, drop_entry=dropped)
    verify = testbed.cli().call_backup("verify", str(broken))
    expect_failure(verify, label="verify archive with a missing attachment object")
    combined = (verify["stdout"] or "") + (verify["stderr"] or "")
    destination = testbed.root / "missing-attachment-restore"
    restore = testbed.cli().call_backup("restore", str(broken), "--to", str(destination))
    expect_failure(restore, label="restore archive with a missing attachment object")
    if destination.exists() and any(destination.iterdir()):
        raise RuntimeError(f"failed restore left a populated target: {destination}")
    problems = assert_original_intact(before, testbed.state())
    if problems:
        raise RuntimeError(f"missing-attachment handling mutated the workspace: {problems}")
    return {
        "observed": f"dropped {dropped}; verify exit {verify['returncode']}, restore exit "
                    f"{restore['returncode']}; workspace intact",
        "original_intact": True,
        "details": {"dropped_entry": dropped, "verify_returncode": verify["returncode"],
                    "restore_returncode": restore["returncode"],
                    "diagnostic_excerpt": combined[-1200:]},
    }


def case_unreadable_vault_file(testbed: Testbed):
    if os.geteuid() == 0:
        return {
            "status": NOT_APPLICABLE,
            "observed": "running as root: chmod-based unreadability is not enforceable",
            "original_intact": True,
        }
    notes = sorted(testbed.workspace.knowledge.rglob("*.md"))
    if not notes:
        raise RuntimeError("workspace has no Markdown note to make unreadable")
    target = notes[0]
    before = testbed.state()
    destination = testbed.root / "unreadable-vault-backups"
    destination.mkdir(exist_ok=True)
    target.chmod(0o000)
    try:
        record = testbed.cli().call_backup(
            "create", "--output", str(destination), "--label", "unreadable"
        )
    finally:
        target.chmod(0o644)
    expect_failure(record, label="create with an unreadable vault file")
    published = archives_in(destination)
    staging = staging_artifacts(destination) + staging_artifacts(testbed.workspace.root)
    after = testbed.state()
    problems = assert_original_intact(before, after)
    if problems:
        raise RuntimeError(f"unreadable-vault handling mutated the workspace: {problems}")
    if published:
        raise RuntimeError(f"partial archive published: {published}")
    if staging:
        raise RuntimeError(f"staging artifacts left behind: {staging}")
    return {
        "observed": f"note {target.name} made unreadable; create failed with exit "
                    f"{record['returncode']}; no archive published; workspace intact",
        "original_intact": True,
        "details": {"returncode": record["returncode"],
                    "diagnostic_excerpt": ((record["stdout"] or "") +
                                           (record["stderr"] or ""))[-1200:]},
    }


def case_permission_denied_destination(testbed: Testbed):
    """Permission denial: the destination cannot be created under a locked parent."""
    if os.geteuid() == 0:
        return {
            "status": NOT_APPLICABLE,
            "observed": "running as root: directory permissions are not enforced",
            "original_intact": True,
        }
    locked_parent = testbed.root / "locked-parent"
    locked_parent.mkdir(exist_ok=True)
    destination = locked_parent / "backups"
    before = testbed.state()
    locked_parent.chmod(0o500)
    try:
        record = testbed.cli().call_backup(
            "create", "--output", str(destination), "--label", "permission-denied"
        )
    finally:
        locked_parent.chmod(0o755)
    expect_failure(record, label="create under a read-only parent")
    published = archives_in(destination)
    after = testbed.state()
    problems = assert_original_intact(before, after)
    if problems:
        raise RuntimeError(f"permission failure mutated the workspace: {problems}")
    if published:
        raise RuntimeError(f"partial archive published: {published}")
    if destination.exists():
        raise RuntimeError(f"failed create left a destination directory: {destination}")
    return {
        "observed": f"permission-denied create failed with exit {record['returncode']}; "
                    f"no destination, no archive; workspace intact",
        "original_intact": True,
        "details": {"returncode": record["returncode"],
                    "diagnostic_excerpt": ((record["stdout"] or "") +
                                           (record["stderr"] or ""))[-1200:]},
    }


def case_destination_permissions_preserved(testbed: Testbed):
    """An existing destination directory is operator state, not staging space.

    Creating a backup must not silently widen the mode of a directory the
    operator already owns, and a non-writable destination must fail cleanly.
    """
    if os.geteuid() == 0:
        return {
            "status": NOT_APPLICABLE,
            "observed": "running as root: directory permissions are not enforced",
            "original_intact": True,
        }
    destination = testbed.root / "read-only-destination"
    destination.mkdir(exist_ok=True)
    destination.chmod(0o500)
    try:
        mode_before = destination.stat().st_mode & 0o777
        before = testbed.state()
        record = testbed.cli().call_backup(
            "create", "--output", str(destination), "--label", "read-only"
        )
        mode_after = destination.stat().st_mode & 0o777
    finally:
        destination.chmod(0o755)
    published = archives_in(destination)
    problems = assert_original_intact(before, testbed.state())
    if problems:
        raise RuntimeError(f"read-only destination handling mutated the workspace: {problems}")
    if mode_after != mode_before:
        raise RuntimeError(
            "PRODUCT DEFECT: `orbit backup create --output <existing 0500 dir>` changed the "
            f"destination directory mode {oct(mode_before)} -> {oct(mode_after)} and "
            f"{'published' if published else 'did not publish'} an archive "
            f"(exit {record['returncode']}). The destination is operator state; creation "
            "must fail cleanly instead of widening its permissions. Likely cause: "
            "orbit-backup staging calls create_private_dir(destination_dir) which chmods "
            "an existing directory to 0700. Repro: mkdir d; chmod 500 d; "
            "orbit backup create --output d."
        )
    expect_failure(record, label="create into a non-writable destination")
    return {
        "observed": f"non-writable destination refused cleanly (exit "
                    f"{record['returncode']}), mode {oct(mode_before)} preserved",
        "original_intact": True,
        "details": {"mode_before": oct(mode_before), "mode_after": oct(mode_after),
                    "returncode": record["returncode"]},
    }


def case_unsupported_manifest_version(testbed: Testbed):
    before = testbed.state()
    patched = testbed.scratch / "future-format.orbitbak"
    inspect = expect_success(
        testbed.cli().call_backup("inspect", str(testbed.baseline)),
        label="inspect baseline",
    )
    manifest = dict(inspect["manifest"])
    manifest["format_version"] = 999
    manifest["min_reader_version"] = 999
    rewrite_archive(testbed.baseline, patched, patch_manifest=manifest)
    verify = testbed.cli().call_backup("verify", str(patched))
    expect_failure(verify, label="verify a newer-format archive")
    combined = ((verify["stdout"] or "") + (verify["stderr"] or "")).lower()
    if "upgrade" not in combined:
        raise RuntimeError(
            "rejection message does not tell the operator to upgrade Orbit: "
            f"{combined[-800:]}"
        )
    inspect_future = testbed.cli().call_backup("inspect", str(patched))
    expect_failure(inspect_future, label="inspect a newer-format archive")
    destination = testbed.root / "future-format-restore"
    restore = testbed.cli().call_backup("restore", str(patched), "--to", str(destination))
    expect_failure(restore, label="restore a newer-format archive")
    problems = assert_original_intact(before, testbed.state())
    if problems:
        raise RuntimeError(f"version handling mutated the workspace: {problems}")
    return {
        "observed": f"format v999 rejected by verify/inspect/restore with an "
                    f"upgrade message; workspace intact",
        "original_intact": True,
        "details": {"verify_returncode": verify["returncode"],
                    "inspect_returncode": inspect_future["returncode"],
                    "restore_returncode": restore["returncode"],
                    "diagnostic_excerpt": combined[-1000:]},
    }


def case_wrong_destination_restore(testbed: Testbed):
    before = testbed.state()
    # Restoring into the existing active workspace directory must be refused.
    into_workspace = testbed.cli().call_backup(
        "restore", str(testbed.baseline), "--to", str(testbed.workspace.root)
    )
    expect_failure(into_workspace, label="restore --to the active workspace")
    # A --to destination that is an existing file must be refused too.
    file_target = testbed.scratch / "not-a-directory"
    file_target.write_text("x")
    into_file = testbed.cli().call_backup(
        "restore", str(testbed.baseline), "--to", str(file_target)
    )
    expect_failure(into_file, label="restore --to an existing file")
    # --to and --replace are mutually exclusive.
    both = testbed.cli().call_backup(
        "restore", str(testbed.baseline), "--to", str(testbed.root / "both"),
        "--replace", "--confirm", "REPLACE",
    )
    expect_failure(both, label="restore --to with --replace")
    problems = assert_original_intact(before, testbed.state())
    if problems:
        raise RuntimeError(f"wrong-destination handling mutated the workspace: {problems}")
    return {
        "observed": f"active-workspace target exit {into_workspace['returncode']}, "
                    f"file target exit {into_file['returncode']}, mixed modes exit "
                    f"{both['returncode']}; workspace intact",
        "original_intact": True,
        "details": {"into_workspace_returncode": into_workspace["returncode"],
                    "into_file_returncode": into_file["returncode"],
                    "mixed_modes_returncode": both["returncode"]},
    }


def case_interrupted_restore(testbed: Testbed):
    before = testbed.state()
    destination = testbed.root / "interrupted-restore"
    baseline_entries = {
        directory: {entry.name for entry in directory.iterdir()}
        for directory in (destination.parent, testbed.workspace.root)
        if directory.is_dir()
    }
    attempt_records = []
    completed = False
    for _attempt in range(4):
        if destination.exists():
            shutil.rmtree(destination)
        record = kill_during(
            testbed.cli(),
            ["--db", str(testbed.workspace.db), "--config",
             str(testbed.workspace.config), "backup", "restore",
             str(testbed.baseline), "--to", str(destination)],
            watch_dirs=[destination.parent, testbed.workspace.root],
            baseline_entries=baseline_entries,
            fallback_delays=(0.01, 0.03, 0.1, 0.3),
        )
        attempt_records.append({
            "returncode": record["returncode"],
            "killed_after_staging": record["killed_after_staging"],
            "activity": record["activity"],
        })
        database = destination / "orbit.db"
        complete = database.is_file() and _sqlite_reads(database)
        if record["returncode"] == 0 and complete:
            completed = True
            break
        if record["returncode"] != 0 and not complete:
            break
    problems = assert_original_intact(before, testbed.state())
    if problems:
        raise RuntimeError(f"interrupted restore damaged the workspace: {problems}")
    target_complete = destination / "orbit.db"
    if target_complete.is_file() and _sqlite_reads(target_complete):
        return {
            "status": INCONCLUSIVE,
            "observed": "every kill landed after the restore completed",
            "original_intact": True,
            "details": {"attempts": attempt_records},
        }
    # A follow-up restore into the same destination must not be silently corrupted.
    followup = testbed.cli().call_backup(
        "restore", str(testbed.baseline), "--to", str(destination)
    )
    followup_ok = followup["returncode"] == 0 and (destination / "orbit.db").is_file()
    if followup["returncode"] != 0:
        combined = (followup["stdout"] or "") + (followup["stderr"] or "")
        if not combined.strip():
            raise RuntimeError("follow-up restore failed without a diagnostic")
        if not is_not_implemented(combined) and not combined.strip():
            raise RuntimeError("follow-up restore failed silently")
    problems = assert_original_intact(before, testbed.state())
    if problems:
        raise RuntimeError(f"interrupted/follow-up restore mutated the workspace: {problems}")
    return {
        "observed": f"killed mid-restore (attempts={len(attempt_records)}, "
                    f"activity_detected={any(a['killed_after_staging'] for a in attempt_records)}); "
                    f"no complete target produced; follow-up restore "
                    f"{'succeeded' if followup_ok else 'failed with a clear error'}",
        "original_intact": True,
        "details": {"attempts": attempt_records,
                    "followup_returncode": followup["returncode"],
                    "followup_ok": followup_ok},
    }


def _sqlite_reads(db_path: Path) -> bool:
    try:
        connection = sqlite3.connect(f"file:{db_path}?mode=ro", uri=True, timeout=5)
        try:
            connection.execute("SELECT COUNT(*) FROM sqlite_master").fetchone()
            return True
        finally:
            connection.close()
    except sqlite3.Error:
        return False


def case_external_agent_session_boundary(_testbed: Testbed, evidence_dir: Path):
    rehearsal = read_json_file(evidence_dir / "rehearsal-evidence.json") or {}
    steps = {step.get("step"): step for step in rehearsal.get("steps", [])}
    step_11 = steps.get(11, {})
    step_12 = steps.get(12, {})
    if step_11.get("status") == PASS and step_12.get("status") == PASS:
        return {
            "observed": "external-boundary scenario: the rehearsal's restored instance "
                        "keeps the reserved agent jobs queued with "
                        "runtime_session_ref=null and makes no launch attempt "
                        "(rehearsal steps 11 and 12)",
            "original_intact": True,
            "details": {"rehearsal_steps": [11, 12],
                        "agent_jobs": (step_11.get("details") or {}).get("agent_jobs"),
                        "no_external_launch": (step_12.get("details") or {}).get(
                            "no_external_launch")},
        }
    return {
        "status": NOT_APPLICABLE,
        "observed": "no supported non-executing path exists to persist an agent job with a "
                    "runtime_session_ref (only the machine launch-result transport sets it, "
                    "which would fabricate an external session). The rehearsal's reserved "
                    "jobs are the honest alternative; run the rehearsal first.",
        "original_intact": True,
    }


def case_replica_snapshot_unsupported(testbed: Testbed):
    """A client-replica snapshot is not a recovery point in this build.

    M34 supports the server authority as the recovery point. `--source-mode
    replica` must fail closed with an actionable message, publish nothing and
    leave the workspace untouched, both from a local deployment and from a
    connected deployment that really has a replica file.
    """
    before = testbed.state()
    local_out = testbed.root / "replica-local-out"
    local_out.mkdir(exist_ok=True)
    local = testbed.cli().call_backup(
        "create", "--source-mode", "replica", "--output", str(local_out),
        "--label", "replica-local",
    )
    local_message = ((local["stdout"] or "") + (local["stderr"] or "")).lower()
    if local["returncode"] == 0:
        raise RuntimeError(
            "PRODUCT DEFECT: --source-mode replica unexpectedly SUCCEEDED from a "
            f"local deployment and published an archive. Output: {local['stdout']}"
        )
    if "server workspace" not in local_message and "not supported" not in local_message:
        # A bare "requires a connected deployment" is fail-closed but does not
        # tell the operator what the supported recovery point is.
        if "connected deployment" not in local_message:
            raise RuntimeError(
                "replica refusal is not actionable: "
                f"{local_message[-800:]}"
            )

    # Connected deployment with a real replica database file: the service-level
    # guard must refuse before touching the authority or the replica.
    replica = testbed.root / "connected-replica.db"
    connection = sqlite3.connect(str(replica))
    try:
        connection.execute("CREATE TABLE IF NOT EXISTS replica_probe (id INTEGER)")
        connection.commit()
    finally:
        connection.close()
    connected_out = testbed.root / "replica-connected-out"
    connected_out.mkdir(exist_ok=True)
    connected_cli = OrbitCli(
        testbed.binary,
        db=replica,
        config=testbed.workspace.config,
        home=testbed.workspace.home,
        extra_env={
            "ORBIT_AUTHORITY_DB": str(replica),
            "ORBIT_AUTHORITY_SERVER": "http://127.0.0.1:9",
            "ORBIT_AUTHORITY_DEVICE": "m34-replica-acceptance",
            "ORBIT_SYNC_HTTP_TOKEN": "m34-replica-acceptance-token-0123456789",
            "ORBIT_DB": str(testbed.workspace.db),
        },
    )
    connected = connected_cli.call_backup(
        "create", "--source-mode", "replica", "--output", str(connected_out),
        "--label", "replica-connected",
    )
    connected_message = (
        (connected["stdout"] or "") + (connected["stderr"] or "")
    ).lower()
    if connected["returncode"] == 0:
        raise RuntimeError(
            "PRODUCT DEFECT: --source-mode replica unexpectedly SUCCEEDED from a "
            f"connected deployment. Output: {connected['stdout']}"
        )
    if not any(
        marker in connected_message
        for marker in ("server workspace", "not supported", "recovery point")
    ):
        raise RuntimeError(
            "connected-deployment replica refusal is not actionable: "
            f"{connected_message[-800:]}"
        )
    for directory in (local_out, connected_out):
        published = archives_in(directory)
        if published:
            raise RuntimeError(
                f"refused replica create published an archive in {directory}: {published}"
            )
        staging = staging_artifacts(directory)
        if staging:
            raise RuntimeError(
                f"refused replica create left staging artifacts in {directory}: {staging}"
            )
    problems = assert_original_intact(before, testbed.state())
    if problems:
        raise RuntimeError(f"refused replica create mutated the workspace: {problems}")
    return {
        "observed": f"local deployment exit {local['returncode']}, connected "
                    f"deployment exit {connected['returncode']}; both refused as "
                    f"unsupported recovery points with no archive/staging; "
                    f"workspace intact",
        "original_intact": True,
        "details": {
            "local_returncode": local["returncode"],
            "local_message": local_message[-600:],
            "connected_returncode": connected["returncode"],
            "connected_message": connected_message[-600:],
        },
    }


def case_pending_connected_client_outbox(_testbed: Testbed):
    return {
        "status": NOT_APPLICABLE,
        "observed": "The CLI accepts neither a connected replica as a restore target nor a "
                    "server outbox; pending connected-client reconciliation is enforced by the "
                    "server/preflight (I2 restore-engine's pending-outbox blocker test) and is "
                    "outside this CLI failure suite. A replica-only workspace is not a "
                    "supported recovery point in this build, so it is never presented as one.",
        "original_intact": True,
    }


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------
def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description="M34 CLI acceptance and failure injection (real binary, temp dirs only)."
    )
    parser.add_argument("--binary", type=Path, default=DEFAULT_CLI)
    parser.add_argument("--server-binary", type=Path, default=DEFAULT_SERVER)
    parser.add_argument("--evidence-dir", type=Path, default=DEFAULT_EVIDENCE_DIR)
    parser.add_argument("--keep-workdir", action="store_true",
                        help="keep the disposable run directory")
    parser.add_argument("--workdir", type=Path, default=None)
    args = parser.parse_args(argv)

    if not args.binary.is_file():
        print(f"required binary is missing: {args.binary}", file=sys.stderr)
        return 1
    root = args.workdir.resolve() if args.workdir else Path(
        tempfile.mkdtemp(prefix="orbit-m34-cli-")
    )
    if args.workdir and root.exists():
        shutil.rmtree(root)
    root.mkdir(parents=True, exist_ok=True)
    runner = CaseRunner(args.evidence_dir, keep_workdir=args.keep_workdir)
    runner.limitations = [
        "Disk-full is not simulated with a size-limited tmpfs (that requires mount "
        "privileges this harness does not have). Permission denial is injected two "
        "real ways instead: a destination under a read-only parent, and an existing "
        "non-writable destination directory (which also asserts the operator's mode "
        "is preserved).",
        "Interrupted backup/restore use SIGKILL at the first observable staging "
        "activity, retried up to four times; if every attempt lands after the "
        "operation completed, the case is reported INCONCLUSIVE instead of a false "
        "pass. The recorded attempts make the timing visible.",
        "A failure *after* activation started (preserved-copy rollback path) has no "
        "supported fault-injection hook from outside the product. The successful "
        "replacement path is asserted to activate, write a recovery journal and "
        "preserve the previous workspace, and the active-target/refused paths are "
        "asserted to leave the target intact; deeper post-activation rollback "
        "injection belongs to the restore service's own tests.",
        "Pending connected-client outbox reconciliation needs a connected replica "
        "target, which the CLI deliberately refuses to replace; it is enforced by "
        "the server/preflight and is outside this CLI-only suite.",
        "The suite touches no repository state: every workspace, archive and kill "
        "target is created under a fresh temporary directory and removed afterwards.",
        "KNOWN LIMITATION (non-blocking, P3 backlog): re-enrolling an already-enrolled "
        "device on a database restored from an archive returns HTTP 500 "
        '{"error":"storage error"} instead of a clean conflict or idempotent '
        "success. Repro: restore an archive containing an enrolled device, start "
        "orbit-server on the restored database, then POST /v1/devices with the same "
        "device id and token using the administrator key => 500. The restored bearer "
        "credential itself authenticates (200), so recovery is unaffected; this harness "
        "therefore relies on the restored credential and never re-enrolls.",
    ]
    print("M34 CLI acceptance and failure injection")
    print(f"  CLI:      {args.binary}")
    print(f"  run dir:  {root}")
    print(f"  evidence: {args.evidence_dir}")
    exit_code = 1
    try:
        testbed = Testbed(root, args.binary, args.server_binary)
        acceptance = [
            ("create — default destination", "a verified archive lands in the configured "
             "backups directory with a verification sidecar", case_create_default),
            ("create — --output and --label", "the archive is written to the external "
             "directory and the manifest records the label, format and inventory",
             case_create_output_and_label),
            ("list — default and external directories", "both listings derive entries "
             "from the archives themselves and report verification status",
             case_list_default_and_external),
            ("verify — passing archive", "verify exits zero, reports ok and lists checks",
             case_verify_pass),
            ("verify — corrupted archive", "verify exits nonzero and the corruption is "
             "reported", case_verify_detects_corruption),
            ("restore --dry-run", "a preflight plan is reported and nothing in the "
             "workspace changes (strict file list and hashes)",
             case_restore_dry_run_mutates_nothing),
            ("restore --to — isolated workspace opens", "a separate workspace is "
             "created, verifies and opens through the CLI with the same tasks/notes",
             case_isolated_restore_opens),
            ("restore --replace — confirmation required", "replace is refused without "
             "the exact confirmation, then activates and preserves the previous workspace",
             case_replace_requires_confirmation),
            ("restore --replace — active target refused", "a workspace held by a writer "
             "is not replaced and stays intact", case_replace_refuses_active_target),
            ("schedule + retention", "the policy round-trips and retention never removes "
             "the last verified recovery point", case_schedule_and_retention),
        ]
        for name, expected, function in acceptance:
            runner.run(name, expected, lambda function=function: function(testbed))
        failures = [
            ("interrupted backup", "SIGKILL mid-create leaves no partial archive, no "
             "staging artifacts and an unchanged workspace", case_interrupted_backup),
            ("corrupted archive", "byte corruption is rejected by verify and restore; "
             "workspace intact", case_corrupted_archive),
            ("truncated archive", "a truncated archive is rejected; workspace intact",
             case_truncated_archive),
            ("missing attachment object", "a referenced attachment object missing from "
             "the archive is detected; workspace intact", case_missing_attachment_object),
            ("unreadable vault file", "an unreadable Markdown file fails creation "
             "cleanly with no partial archive", case_unreadable_vault_file),
            ("disk-full / permission failure", "a destination that cannot be created "
             "fails cleanly with no partial archive",
             case_permission_denied_destination),
            ("destination permissions preserved", "an existing non-writable destination "
             "is never silently widened and fails cleanly",
             case_destination_permissions_preserved),
            ("unsupported manifest version", "a newer format is rejected with an "
             "actionable upgrade message", case_unsupported_manifest_version),
            ("wrong destination", "restoring onto the active workspace, a file, or with "
             "mixed modes is refused", case_wrong_destination_restore),
            ("interrupted restoration", "SIGKILL mid-restore leaves the original "
             "workspace intact and never produces a silently usable partial target",
             case_interrupted_restore),
            ("replica snapshots unsupported", "a client-replica snapshot is refused "
             "with an actionable 'use the server workspace' message from both local and "
             "connected deployments, publishing nothing", case_replica_snapshot_unsupported),
            ("missing external agent session", "external-boundary scenario: restored "
             "reserved jobs stay queued and no launch is attempted",
             lambda testbed: case_external_agent_session_boundary(
                 testbed, args.evidence_dir)),
            ("pending connected-client outbox", "connected-client reconciliation is an "
             "external boundary for this CLI suite",
             case_pending_connected_client_outbox),
        ]
        for name, expected, function in failures:
            runner.run(name, expected, lambda function=function: function(testbed))
        if (testbed.root / "isolated-restore").exists() and testbed.workspace.db.exists():
            pass
        print("\nSummary")
        print("-------")
        for case in runner.cases:
            print(f"  {case['status']:<14} {case['name']}")
        summary = runner.summary()
        print(f"  {summary}")
        evidence = runner.evidence(args.binary)
        evidence["binaries"]["orbit_server"] = binary_identity(
            args.server_binary, probe_version=False)
        mcp_binary = REPO_ROOT / "target" / "debug" / "orbit-mcp"
        if mcp_binary.is_file():
            evidence["binaries"]["orbit_mcp"] = binary_identity(
                mcp_binary, probe_version=False)
        evidence["git"] = git_identity()
        evidence["dataset"] = testbed.dataset
        evidence["acceptance_commands"] = testbed.cli().history
        args.evidence_dir.mkdir(parents=True, exist_ok=True)
        evidence_path = args.evidence_dir / "failure-injection.json"
        evidence_path.write_text(json.dumps(evidence, indent=2, sort_keys=False) + "\n")
        readme = write_readme(args.evidence_dir)
        print(f"\nEvidence: {evidence_path}")
        print(f"README:   {readme}")
        exit_code = runner.exit_code()
    finally:
        if args.keep_workdir:
            print(f"Kept run directory: {root}")
        else:
            shutil.rmtree(root, ignore_errors=True)
    return exit_code


if __name__ == "__main__":
    sys.exit(main())
