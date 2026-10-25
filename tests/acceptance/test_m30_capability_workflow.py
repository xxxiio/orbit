"""M30 acceptance: trusted capability execution through the real public path.

Runs the actual ``orbit`` binary against an isolated workspace and proves the
whole composition path with no in-process shortcuts:

    create project -> capability install -> publish manifest -> bind
    -> create work item -> offers -> refused request (not_authorized)
    -> authorize -> request -> execute -> receipt -> inspect
    -> restart -> replay (no duplicate effect)

The note count is asserted through ``orbit blueprint item`` (a real read
projection), never by reading SQLite directly.
"""
from __future__ import annotations

import json
import os
import subprocess
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parents[2]
BINARY = Path(os.environ.get("ORBIT_CLI_BIN", ROOT / "target" / "debug" / "orbit"))
MANIFEST = ROOT / "blueprints" / "examples" / "capability-reference.json"
CAPABILITY = "orbit.reference.work-item-note"


class Cli:
    """One process invocation per call: no shared in-process state."""

    def __init__(self, env: dict[str, str]):
        self.env = env
        self.history: list[tuple[str, ...]] = []

    def run(self, *argv: str, expect: str = "success") -> subprocess.CompletedProcess:
        self.history.append(argv)
        result = subprocess.run(
            [str(BINARY), "--json", *argv],
            text=True,
            capture_output=True,
            env=self.env,
            timeout=120,
        )
        if expect == "success" and result.returncode != 0:
            raise AssertionError(
                f"command {argv!r} unexpectedly failed ({result.returncode})\n"
                f"stdout={result.stdout}\nstderr={result.stderr}"
            )
        if expect == "failure" and result.returncode == 0:
            raise AssertionError(f"expected failure from {argv!r}; got {result.stdout}")
        return result

    def json(self, *argv: str) -> dict:
        result = self.run(*argv)
        try:
            parsed = json.loads(result.stdout)
        except json.JSONDecodeError as error:
            raise AssertionError(
                f"non-JSON response for {argv!r}: {result.stdout}"
            ) from error
        assert isinstance(parsed, dict), parsed
        return parsed

    def result(self, *argv: str):
        return self.json(*argv)["result"]

    def failure(self, *argv: str) -> subprocess.CompletedProcess:
        return self.run(*argv, expect="failure")


def isolated_environment(tmp_path: Path) -> tuple[dict[str, str], Path]:
    workspace = tmp_path / "workspace"
    workspace.mkdir()
    home = tmp_path / "home"
    home.mkdir()
    database = workspace / "orbit.sqlite"
    env = {
        key: value
        for key, value in os.environ.items()
        if not key.startswith("ORBIT_")
    }
    env.update(
        {
            "HOME": str(home),
            "ORBIT_CONFIG": str(home / "config.toml"),
            "ORBIT_DB": str(database),
        }
    )
    return env, database


@pytest.fixture()
def workspace(tmp_path: Path):
    if not BINARY.is_file():
        pytest.fail(f"orbit binary is missing: {BINARY}; run cargo build -p orbit-cli")
    env, database = isolated_environment(tmp_path)
    yield Cli(env), env, database, tmp_path


def test_capability_workflow(workspace):
    cli, env, database, tmp_path = workspace

    project = cli.json("project", "add", "Capability Demo")["project"]["id"]
    assert database.is_file(), "the local authority workspace was not created"

    # Ordering rule: a manifest whose capability is not installed cannot be
    # published, even though the manifest itself is well formed.
    premature = cli.failure("blueprint", "import", str(MANIFEST))
    assert "unavailable capability" in premature.stderr, premature.stderr
    assert ["capability-reference", 1] not in cli.result("blueprint", "list")

    installed = cli.result("blueprint", "capability", "install", CAPABILITY)
    assert installed["capability_id"] == CAPABILITY
    assert installed["provider"] == "orbit"
    assert installed["permission"] == "mutate"
    # Installation is idempotent for an identical contract.
    assert cli.result("blueprint", "capability", "install", CAPABILITY) == installed

    published = cli.result("blueprint", "import", str(MANIFEST))
    assert published["id"] == "capability-reference"
    assert published["required_capabilities"] == [CAPABILITY]

    # A new project is created with the bundled Basic workflow, so the explicit
    # supported operation here is `apply` (replace the pinned workflow).
    binding = cli.result("blueprint", "apply", project, "capability-reference", "--version", "1")
    assert binding == {
        "project_id": project,
        "blueprint_id": "capability-reference",
        "version": 1,
    }

    item = cli.result("blueprint", "create-item", project, "item-1", "note-task")
    assert item["id"] == "item-1"
    assert item["state"] == "open"
    assert item["revision"] == 0

    offers = cli.result("blueprint", "action", "offers", "item-1")
    assert len(offers) == 1
    offer = offers[0]
    assert offer["action_id"] == "append_note"
    assert offer["capability"] == CAPABILITY
    assert offer["from"] == "open"
    assert offer["to"] == "closed"
    assert offer["external_effect"] is False
    assert offer["declared_parameters"] == {"note": ""}
    assert offer["availability"]["state"] == "unavailable"
    assert offer["availability"]["code"] == "not_authorized"

    note_input = json.dumps({"note": "hello durable"})
    request_args = (
        "blueprint",
        "action",
        "request",
        "item-1",
        "--action",
        "append_note",
        "--revision",
        "0",
        "--command",
        "cmd-m30-1",
        "--input",
        note_input,
    )
    refused = cli.failure(*request_args)
    assert "not_authorized" in refused.stderr, refused.stderr
    assert cli.result("blueprint", "action", "list") == []

    # Authorization is a separate, explicitly confirmed operation.
    unconfirmed = cli.failure(
        "blueprint", "capability", "authorize", CAPABILITY, "--project", project
    )
    assert "--confirm" in unconfirmed.stderr, unconfirmed.stderr
    grant = cli.result(
        "blueprint", "capability", "authorize", CAPABILITY, "--project", project, "--confirm"
    )
    assert grant["capability_id"] == CAPABILITY
    assert grant["project_id"] == project

    status = cli.result("blueprint", "capability", "list", "--project", project)
    reference = [row for row in status if row["capability_id"] == CAPABILITY]
    assert len(reference) == 1, status
    row = reference[0]
    assert row["installed"] is True
    assert row["executor_registered"] is True
    assert row["available"] is True
    assert row["authorized_projects"] == [project]
    # The opt-in habit recurrence package is composed but not installed, so it
    # is listed as unavailable rather than silently appearing usable.
    habit = [row for row in status if row["capability_id"] == "orbit.habit.recurrence"]
    assert len(habit) == 1, status
    assert habit[0]["installed"] is False
    assert habit[0]["available"] is False

    assert cli.result("blueprint", "action", "offers", "item-1")[0]["availability"] == {
        "state": "available"
    }

    request = cli.result(*request_args)
    assert request["command_id"] == "cmd-m30-1"
    assert request["state"] == "pending"
    assert request["item_revision"] == 0
    plan = json.loads(request["plan_json"])
    assert plan["parameters"] == {"note": "hello durable"}
    # The exact command ID is echoed, so a script can replay it.
    assert cli.result(*request_args) == request

    receipt = cli.result("blueprint", "action", "execute", "cmd-m30-1")
    assert receipt["command_id"] == "cmd-m30-1"
    assert receipt["outcome"] == "succeeded"
    assert receipt["provider"] == "orbit"
    assert receipt["item_id"] == "item-1"
    assert receipt["action_id"] == "append_note"

    shown = cli.result("blueprint", "action", "show", "cmd-m30-1")
    assert shown["request"]["state"] == "succeeded"
    assert shown["receipt"] == receipt
    assert cli.result("blueprint", "action", "show", "cmd-m30-1") == shown

    item = cli.result("blueprint", "item", "item-1")["capability_notes"]
    assert [note["note"] for note in item] == ["hello durable"]
    assert [note["command_id"] for note in item] == ["cmd-m30-1"]
    assert item[0]["tag"] is None

    # A brand new process sees the same durable receipt and effect.
    restarted = Cli(env)
    assert restarted.result("blueprint", "action", "show", "cmd-m30-1")["receipt"] == receipt
    persisted = restarted.result("blueprint", "item", "item-1")
    assert persisted["item"]["state"] == "open"
    assert len(persisted["capability_notes"]) == 1

    # Replaying `execute` returns the recorded receipt and never duplicates the
    # durable note.
    assert restarted.result("blueprint", "action", "execute", "cmd-m30-1") == receipt
    assert len(restarted.result("blueprint", "item", "item-1")["capability_notes"]) == 1

    listed = restarted.result(
        "blueprint", "action", "list", "--item", "item-1", "--state", "succeeded"
    )
    assert [action["command_id"] for action in listed] == ["cmd-m30-1"]

    # A second command for the same (item, revision, action) is fenced.
    duplicate = restarted.failure(
        "blueprint",
        "action",
        "request",
        "item-1",
        "--action",
        "append_note",
        "--revision",
        "0",
        "--command",
        "cmd-m30-2",
        "--input",
        note_input,
    )
    assert duplicate.returncode != 0

    # Reconciliation of an already settled action is refused; `--confirm` stays
    # mandatory even before the authority is asked.
    refused_reconcile = restarted.failure(
        "blueprint", "action", "reconcile", "cmd-m30-1", "--succeed", "--confirm"
    )
    assert refused_reconcile.returncode != 0

    # A second item covers the public pending path. A crashed claim (state
    # `claimed` with no receipt) has no public CLI operation that can produce it,
    # so that lost-acknowledgement recovery is proven by the runtime tests; this
    # file must not fake durable state through SQLite.
    second = restarted.result("blueprint", "create-item", project, "item-2", "note-task")
    assert second["state"] == "open"
    assert second["revision"] == 0
    assert restarted.result("blueprint", "item", "item-2")["capability_notes"] == []

    lost_request_args = (
        "blueprint",
        "action",
        "request",
        "item-2",
        "--action",
        "append_note",
        "--revision",
        "0",
        "--command",
        "cmd-m30-lost",
        "--input",
        note_input,
    )
    pending_request = restarted.result(*lost_request_args)
    assert pending_request["state"] == "pending"

    unconfirmed_reconcile = restarted.failure(
        "blueprint", "action", "reconcile", "cmd-m30-lost", "--fail"
    )
    assert "--confirm" in unconfirmed_reconcile.stderr, unconfirmed_reconcile.stderr

    # A pending request has not executed: it can never be reconciled, and the
    # refusal leaves it untouched and still executable.
    premature_reconcile = restarted.failure(
        "blueprint", "action", "reconcile", "cmd-m30-lost", "--fail", "--confirm"
    )
    assert premature_reconcile.returncode != 0
    still_pending = restarted.result("blueprint", "action", "show", "cmd-m30-lost")
    assert still_pending["request"]["state"] == "pending"
    assert still_pending["receipt"] is None
    assert restarted.result("blueprint", "item", "item-2")["capability_notes"] == []

    # The pending request still goes through the normal, explicit execution path.
    second_receipt = restarted.result("blueprint", "action", "execute", "cmd-m30-lost")
    assert second_receipt["outcome"] == "succeeded"
    assert second_receipt["command_id"] == "cmd-m30-lost"
    second_notes = restarted.result("blueprint", "item", "item-2")["capability_notes"]
    assert [note["command_id"] for note in second_notes] == ["cmd-m30-lost"]

    # Connected deployments must fail closed instead of queueing a command that
    # can never execute.
    connected_env = dict(env)
    connected_env["ORBIT_AUTHORITY_DB"] = str(tmp_path / "replica.sqlite")
    connected_env["ORBIT_AUTHORITY_SERVER"] = "https://authority.example.test"
    refusal = Cli(connected_env).failure("blueprint", "action", "offers", "item-1")
    combined = refusal.stdout + refusal.stderr
    assert "capability actions require the local authoritative workspace" in combined, combined
    assert "M25 EC-004" in combined, combined
    assert not (tmp_path / "replica.sqlite").exists(), "connected refusal must not write"
