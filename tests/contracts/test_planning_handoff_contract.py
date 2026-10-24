"""Source-level interface guard only; executable Rust acceptance is M15.

Rust-syntax assertions are whitespace-insensitive so the `cargo fmt` gate and
this suite cannot disagree.
"""
import re
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
TUI = ROOT / "crates/orbit-tui/src"

def read(rel): return (ROOT/rel).read_text()

def compact(text): return re.sub(r"\s+", "", text)

def test_shared_dispatcher_and_revision_checks():
    src=compact(read('crates/orbit-runtime/src/planning_handoff.rs'))
    for term in ['resolve_task_action(brief,request,selection)',
                 'resolve_group_action(brief,request,selection)',
                 'authoritative_write_performed:false','outside accepted visible planning scope',
                 'PreviewOperation::CalendarReview','PreviewOperation::DependencyAddition',
                 'select_from_menu']:
        assert compact(term) in src, term

def test_shared_shell_owns_the_only_tui_handoff_surface():
    # The legacy connected event loop is retired. One shared shell serves both
    # deployments, so the selectable-index and revision checks exist in exactly
    # one implementation and no connected module may re-implement them.
    #
    # The I5 split moved these owners: actions live in `app/actions.rs`, the
    # rendered handoff in `planning_ui.rs`. Both are asserted by name below so a
    # future split cannot silently drop the guard.
    actions = compact((TUI / "app/actions.rs").read_text(encoding="utf-8"))
    navigation = compact((TUI / "app/navigation.rs").read_text(encoding="utf-8"))
    ui = compact((TUI / "planning_ui.rs").read_text(encoding="utf-8"))
    modal = compact((TUI / "input/modal.rs").read_text(encoding="utf-8"))
    shell = actions + navigation + ui + modal

    for term in ['select_from_menu', 'render_handoff',
                 'PreviewOperation::CompletionImpact',
                 'latest.revision!=brief.revision', 'app.today_select_action']:
        assert compact(term) in shell, term

    # The exact owner of each term, so a term cannot drift into a module that
    # never executes it.
    assert compact('pub fn today_select_action(&mut self,index:usize)') in actions
    assert compact('planning_handoff::select_from_menu(&menu,index)') in actions
    assert compact('latest.revision!=brief.revision') in actions
    assert compact('==orbit_runtime::planning_handoff::PreviewOperation::CompletionImpact') in actions
    assert compact('planning_ui::render_handoff(&handoff)') in actions
    assert compact('pub fn render_handoff(') in ui
    assert compact('app.today_select_action(index)') in modal

    entry = read('crates/orbit-tui/src/main.rs')
    assert 'mod connected;' not in entry
    assert not (ROOT / 'crates/orbit-tui/src/connected.rs').exists()
    for path in ('crates/orbit-tui/src/connected_ops.rs', 'crates/orbit-tui/src/backend.rs'):
        owner = compact(read(path))
        assert 'render_handoff' not in owner, path
        assert 'select_from_menu' not in owner, path

def test_machine_adapters_do_not_enqueue_from_handoff():
    sources=[compact(read(path)) for path in (
        'crates/orbit-cli/src/task_commands.rs',
        'crates/orbit-server/src/main.rs',
        'crates/orbit-mcp/src/embedded_backend.rs',
        'crates/orbit-mcp/src/connected_backend.rs')]
    for src in sources:
        assert 'planning_handoff::dispatch_task' in src
        assert 'planning_handoff::dispatch_group' in src
    assert 'planning-handoff-task' in sources[1]
    assert 'planning-handoff-group' in sources[1]
    assert 'planning.handoff_task' in sources[2] and 'planning.handoff_task' in sources[3]
    assert 'planning.handoff_group' in sources[2] and 'planning.handoff_group' in sources[3]

def test_capability_registry_has_structured_read_only_tools():
    registry=compact(read('crates/orbit-runtime/src/capabilities.rs'))
    for id in ['planning.handoff_task','planning.handoff_group']:
        marker=compact(f'id:"{id}"')
        index=registry.index(marker)
        descriptor=registry[index:index+900]
        assert 'AuthorityRequirement::AcceptedRead' in descriptor, id
        assert 'offline_accepted_snapshot:true' in descriptor, id
        assert 'deterministic:true' in descriptor, id

def test_no_explicit_approval_bypass_in_new_core_handoff():
    src=compact(read('crates/orbit-runtime/src/planning_handoff.rs').split('#[cfg(test)]')[0])
    for forbidden in ['.submit_command(','.apply_plan(','.queue(','.flush(','.commit(']:
        assert forbidden not in src, forbidden
