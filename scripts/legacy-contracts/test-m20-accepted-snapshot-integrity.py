#!/usr/bin/env python3
"""Static regression gates only; Rust tests remain required for acceptance."""
from pathlib import Path
s=(Path(__file__).resolve().parents[2]/'crates/orbit-client/src/authority.rs').read_text()
checks={
 'snapshot checks persisted metadata': 'accepted snapshot metadata is inconsistent',
 'snapshot checks every accepted collection': 'accepted snapshot missing {key}',
 'acceptance checks actual stored state': 'let previous = self.snapshot()?;',
 'same revision checks full projection': 'fingerprint != old_fingerprint || &old != data',
 'unchanged-revision test': 'fn equal_revision_cannot_rewrite_accepted_projection()',
 'corrupt-cache test': 'fn malformed_persisted_snapshot_fails_closed()',
}
for label,fragment in checks.items():
    assert fragment in s,label
print('M20 accepted snapshot integrity source guards: PASS (not compiled Rust acceptance)')
