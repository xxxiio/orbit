# Orbit structured knowledge contract

Orbit knowledge is a preset structured subsystem for organizing useful life/work knowledge. It is not a generic user-defined database builder and does not require Obsidian or Notion.

## Preset kinds
- Note
- Decision
- Finding
- Assumption
- Risk
- Reference
- Concept
- Resource
- MeetingNote
- DailyNote
- ProjectContext

Each item has a stable `KnowledgeItemId`, title, Markdown-capable body, lifecycle status, provenance, monotonic timestamps, and typed relationships.

## Provenance
Knowledge provenance is explicit: human, named agent, named import source, or named automation. Non-human provenance sources may not be blank.

## Lifecycle
Items start active and can move to resolved, superseded, or archived according to deterministic transitions. Resolved/superseded items can later be archived; archived items are terminal.

## Relationships
Knowledge can point to projects, milestones, tasks, habits, and other knowledge items using preset relation kinds such as about, supports, depends-on, supersedes, references, and related.

Self-links and exact duplicate links are rejected. Relationships are semantic identity links, not folder-path identity.

## Data model boundary
Structured fields carry machine-readable semantics. The body remains expressive text/Markdown. Future persistence/search/AI/MCP/Flutter layers must consume this same semantic model rather than inventing separate note metadata formats.

## Persistence and activity
Knowledge is persisted in the same local SQLite materialized-state store as Orbit execution data. `knowledge_items` stores structured identity/content/lifecycle/provenance and `knowledge_relations` stores ordered typed edges. Knowledge mutations are application commands: state, semantic activity, and (when sync is enabled) the pending semantic sync change commit together.

Knowledge uses archive/supersede/resolved lifecycle rather than destructive deletion in the current contract.

## Sync
Knowledge is a first-class semantic sync entity. Newly written sync envelopes use wire version 2; Orbit remains able to read version-1 execution-only history. Knowledge sync carries typed fields and ordered relations, and conflict-free inbound materialization writes canonical SQLite state without generating activity or outbound echo changes. Locally pending fields and unresolved conflicts retain the same protections as execution entities.

## Portable import/export
The CLI exposes an explicit versioned JSON knowledge document. Export preserves stable id, kind, Markdown body, lifecycle, provenance, timestamps, and typed relations. Import validates the preset schema and referenced targets and then enters Orbit through the normal application/activity/sync transaction boundary. This is user-directed portability; it does not require an Orbit-hosted backend.

## CLI foundation
The headless surface supports create/list/show/update/status/link/unlink plus explicit export/import. Full-text retrieval, backlinks, graph/context assembly, and cross-entity knowledge search are MILESTONE-0009 concerns rather than hidden inside CRUD.

## Deterministic retrieval baseline
M9 starts from retrieval that does not require AI or embeddings:
- case-insensitive local text search across stable id, title, and Markdown body;
- preset kind/lifecycle filters and optional typed-target filtering;
- typed backlinks from any execution/knowledge target to the knowledge items that reference it;
- bounded one-hop context bundles with direct target-linked knowledge plus knowledge-to-knowledge neighbors;
- an inbox convention: active knowledge with no typed relationships is considered captured but unorganized.

Ordering is deterministic. Optional semantic/embedding retrieval may supplement this baseline later but may not replace it.


## Resource/attachment boundary

Knowledge resources separate portable metadata from device-local file location. `KnowledgeResource` contains the stable resource id, owning knowledge id, display name, optional MIME type, size/hash metadata, and creation time. A `LocalResourceLocator` contains only the local path for the current device.

Local paths and file bytes MUST NOT be emitted into semantic sync changes. Remote devices may receive resource metadata with no local locator and can explicitly relink the resource to a file on that device. Future content transports are opt-in and separate from the semantic sync protocol.

Portable context bundles for AI/MCP include resource metadata but not local paths or bytes.

## Semantic retrieval

Deterministic local retrieval remains mandatory. `SemanticKnowledgeRetriever` is an optional application port that may be implemented by a local embedding index or a user-selected remote provider. Hybrid retrieval may add semantic-only results, but absence/failure of semantic retrieval must never make deterministic search unavailable.

## Development 070: local index and exact reference locations

SQLite schema v12 introduces a local, content-backed `knowledge_fts` FTS5 index.
The initial migration rebuilds from canonical `knowledge_items`, and insert,
update and delete triggers maintain the index even when writes originate from
semantic replay. `SqliteStore::knowledge_index_candidates` returns bounded,
ranked candidate IDs; exact phrase navigation still uses the deterministic
reference matcher, which is the source of truth for original-text offsets.
The index stays on the user's local SQLite database and is never uploaded.

`knowledge_term_locations` maps Unicode case-folded matches back to original
scalar-value columns and shows previews from original content. The TUI `f`
shortcut displays navigable reference identifiers, fields and positions in its
Knowledge tab. CLI and scoped MCP term-reference operations remain available.

**Release acceptance (M15):** Rust build and regression execution, FTS5
availability on supported SQLite distributions, large-vault latency, migration
from schema v11, and real terminal navigation must be verified. The initial
TUI reference list does not yet open an editor at a selected offset.
