"""Source-contract smoke checks for accepted-state planning navigation.

These checks cannot replace Cargo builds or executable Rust regression tests.
They catch accidental adapter drift in the user-facing navigation surfaces.
Rust-syntax assertions are whitespace-insensitive so the `cargo fmt` gate and
this suite cannot disagree.
"""
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]


def source(path):
    return (ROOT / path).read_text(encoding="utf-8")


def compact(text):
    return re.sub(r"\s+", "", text)


def test_core_uses_one_accepted_work_brief_for_both_drilldowns():
    core = compact(source("crates/orbit-runtime/src/planning_focus.rs"))
    for symbol in ("pub fn focus_task", "pub fn focus_group", "pub struct TaskFocus", "pub struct GroupFocus"):
        assert compact(symbol) in core, symbol
    assert compact("task.blocked_by.retain") in core
    assert compact("task.unblocks.retain") in core
    assert "walk_trails" in core
    assert compact("max_neighbors>100") in core
    assert compact("max_tasks>100") in core
    assert "pending" in core


def test_authority_and_connected_client_have_fenced_views():
    host = source("crates/orbit-runtime/src/lib.rs")
    client = source("crates/orbit-client/src/unified/planning_queries.rs")
    for name in ("planning_focus", "planning_group"):
        assert f"pub fn {name}" in host
        assert compact(f"pub fn {name}") in compact(client)
    assert "planning brief revision is stale" in host
    assert "planning brief revision is stale" in client
    assert "accepted_snapshot()" in client


def test_cli_and_http_surface_do_not_mutate_ledger():
    commands = source("crates/orbit-cli/src/commands.rs")
    cli = source("crates/orbit-cli/src/task_commands.rs")
    server = compact(source("crates/orbit-server/src/main.rs"))
    assert "PlanningFocus(TaskPlanningFocusArgs)" in commands
    assert "PlanningGroup(TaskPlanningGroupArgs)" in commands
    assert "TaskSubcommand::PlanningFocus" in cli
    assert "TaskSubcommand::PlanningGroup" in cli
    assert compact('"/v1/authoritative/planning-focus",get(') in server
    assert compact('"/v1/authoritative/planning-group",get(') in server
    assert ".planning_focus" in server
    assert ".planning_group" in server


def test_mcp_registry_and_both_adapters_are_consistent():
    registry = source("crates/orbit-runtime/src/capabilities.rs")
    main = source("crates/orbit-mcp/src/main.rs")
    embedded = source("crates/orbit-mcp/src/embedded_backend.rs")
    connected = source("crates/orbit-mcp/src/connected_backend.rs")
    for name in ("planning.task_focus", "planning.group_focus"):
        for text in (registry, main, embedded, connected):
            assert name in text
    for text in (embedded, connected):
        assert "unrestricted planning read access" in text
        assert compact("work_brief(&request)") in compact(text)


def test_terminal_paths_share_the_same_read_model():
    ui = source("crates/orbit-tui/src/planning_ui.rs")
    local = source("crates/orbit-tui/src/main.rs") + source("crates/orbit-tui/src/app.rs")
    connected = source("crates/orbit-tui/src/connected.rs")
    assert "render_focus" in ui
    assert "render_group_focus" in ui
    assert "today_focus" in local
    assert "today_project_focus" in local
    assert '"plan.focus"' in connected
    assert '"plan.group"' in connected
    assert "planning_focus::focus_task" in connected
    assert "planning_focus::focus_group" in connected


def test_pure_core_has_focused_regression_coverage():
    core = source("crates/orbit-runtime/src/planning_focus.rs")
    for scenario in (
        "scoped_prerequisite_never_leaks_identity",
        "project_and_milestone_scope_never_mix_projects",
        "diamonds_produce_two_deterministic_paths_without_foreign_lookups",
        "missing_upstream_task_is_not_disclosed_in_a_path",
        "group_bottlenecks_ignore_nonvisible_prerequisite_ids",
        "group_focus_is_deterministic_and_nonmutating",
    ):
        assert scenario in core
