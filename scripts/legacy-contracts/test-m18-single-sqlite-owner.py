#!/usr/bin/env python3
"""Source regression check for one SQLite owner per Orbit host (not Rust acceptance)."""
from pathlib import Path
r=Path(__file__).resolve().parents[2]
host=(r/'crates/orbit-runtime/src/lib.rs').read_text()
ledger=(r/'crates/orbit-runtime/src/store.rs').read_text()
app=(r/'crates/orbit-storage/src/lib.rs').read_text()
assert 'application: SqliteStore' not in host
assert 'let application = SqliteStore::open(' not in host
assert 'let authority = store::AuthorityStore::open(path)?;' in host
assert 'pub fn application_store(&self)' not in host
assert 'pub(crate) conn: orbit_storage::SqliteStore' in ledger
assert 'Connection::open(path)?' not in ledger.split('impl AuthorityStore {',1)[1].split('pub fn open',1)[1].split('pub fn ',1)[0]
assert 'orbit_storage::SqliteStore::open(path)' in ledger
assert 'orbit_storage::SqliteStore::in_memory()' in ledger
assert 'impl std::ops::Deref for SqliteStore' in app
print('M18 single owner per runtime source contract: passed')
