from pathlib import Path
root=Path(__file__).resolve().parents[2]
cli=(root/'crates/orbit-cli/src/lib.rs').read_text()
svc=(root/'crates/orbit-runtime/src/application_services.rs').read_text()
assert 'fn run_routine_host(' in cli
assert 'fn set_routine_habit_host(' in cli
assert 'Command::Routine(_) => unreachable!' in cli
assert 'fn run_routine(store:' not in cli
assert 'fn set_routine_habit_cli(' not in cli
for method in ('create_routine','set_routine_habit_membership'):
    assert f'pub(crate) fn {method}(' in svc
    assert (f'host.{method}_accepted(' in cli)
assert 'application_queries().routines()' in cli
print('M18 routine host-boundary checks passed')
