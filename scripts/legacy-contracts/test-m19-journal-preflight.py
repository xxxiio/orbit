#!/usr/bin/env python3
"""Source-level check: recovery must validate the whole journal before publication.
This is a guard, not a substitute for compiled Rust crash/restart tests.
"""
from pathlib import Path
source = (Path(__file__).resolve().parents[2] / 'crates/orbit-runtime/src/knowledge_operations.rs').read_text()
assert 'fn preflight(&self) -> Result<Vec<JournalEntry>, OperationError>' in source
assert 'let entries = self.preflight()?;' in source
assert source.index('let entries = self.preflight()?;') < source.index('for item in entries {', source.index('pub fn recover(&self)'))
assert 'if !paths.insert(target.clone())' in source
for name in ('corrupt_later_entry_does_not_publish_earlier_valid_entry',
             'late_external_conflict_does_not_publish_earlier_entry',
             'ambiguous_multiple_intents_for_one_note_fail_before_publication'):
    assert f'fn {name}()' in source
print('M19 journal preflight source contract: passed (Rust execution deferred to M15)')
