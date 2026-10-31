//! Shared typed-reference grammar for Markdown knowledge.
//!
//! A typed reference is the Markdown link `[project:orbit](orbit://project:orbit)`:
//! the link text and its `orbit://` destination are the same `<kind>:<id>` token.
//! The CLI writes exactly these bytes and the vault read path matched exactly
//! these bytes, so this module is the single definition of that grammar.
//!
//! There is deliberately no second parser: if the writer and the reader ever
//! disagreed about a byte, "the note references the project" and "the project
//! lists this note" would silently disagree, and the disagreement would be
//! invisible until a user followed a link that was not there.
//!
//! The byte grammar stays kind-agnostic so the CLI can keep its local-only
//! `habit:` kind without re-implementing the token format; [`TypedReferenceKind`]
//! is the typed view, and it names only the entities the authority owns.

use crate::{
    knowledge_operations::KnowledgeOperations,
    knowledge_service::{KnowledgeDocumentInfo, KnowledgeService, KnowledgeServiceError},
    store::{AuthorityError, AuthorityStore},
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// The same identity rule as note and command ids: a reference id is not a
/// free-form string. It is never trimmed, percent-decoded or inferred from a
/// path, because an id that "nearly" matches an entity is not a reference.
pub const MAX_REFERENCE_ID_LEN: usize = 128;

/// Fixed refusal text. The browser quotes these verbatim, so they are part of
/// the wire contract: no paths, ids or SQL leak through a refusal.
pub const INVALID_REFERENCE_COMMAND: &str = "invalid reference command";
/// A malformed target on a read. Reads have no command envelope to blame, so
/// the refusal names the target instead of an absent command.
pub const INVALID_REFERENCE_TARGET: &str = "invalid reference target";
pub const UNSUPPORTED_REFERENCE_KIND: &str = "unsupported reference target kind";
pub const UNKNOWN_REFERENCE_TARGET: &str = "unknown reference target";
pub const INVALID_NOTE_ID: &str = "invalid note id";
pub const NOTE_DOES_NOT_EXIST: &str = "note does not exist";

fn valid_reference_kind(kind: &str) -> bool {
    !kind.is_empty()
        && kind.len() <= 32
        && kind
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b == b'_' || b.is_ascii_digit())
        && kind.as_bytes()[0].is_ascii_lowercase()
}

/// 1..=128 ASCII alphanumeric, `-` or `_`.
pub fn valid_reference_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= MAX_REFERENCE_ID_LEN
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// Split one `<kind>:<id>` token. Both halves are validated, so a token that
/// fails either rule is prose and is never rewritten into a reference.
///
/// The kind is checked for *syntax* only: which kinds name an authority entity
/// is transport policy, and the CLI legitimately knows one more than the server.
pub fn parse_reference_token(token: &str) -> Option<(&str, &str)> {
    let (kind, id) = token.split_once(':')?;
    (valid_reference_kind(kind) && valid_reference_id(id)).then_some((kind, id))
}

/// The canonical Markdown link for one token. This is the only place the byte
/// shape of a typed reference is produced.
pub fn reference_token(kind: &str, id: &str) -> String {
    format!("[{kind}:{id}](orbit://{kind}:{id})")
}

/// Every canonical `<kind>:<id>` token in one body, deduplicated and ordered.
///
/// The link text and the destination must be the *same* token: a link whose text
/// is a label (`[Orbit](orbit://project:orbit)`) or whose destination names a
/// different entity (`[project:web](orbit://project:orbit)`) is prose, not a
/// typed reference, and is neither read nor rewritten here.
fn canonical_targets(body: &str) -> Vec<(&str, &str)> {
    let mut found = BTreeSet::new();
    let mut rest = body;
    while let Some(open) = rest.find('[') {
        let candidate = &rest[open..];
        // Advance past this bracket whatever happens next, so the scan is
        // linear and a body full of `[` cannot loop.
        rest = &rest[open + 1..];
        let Some(close) = candidate.find(']') else {
            break;
        };
        let text = &candidate[1..close];
        let Some((kind, id)) = parse_reference_token(text) else {
            continue;
        };
        if candidate.starts_with(&reference_token(kind, id)) {
            found.insert((kind, id));
        }
    }
    found.into_iter().collect()
}

/// Whether one body carries the exact canonical token for `target`.
///
/// This is the CLI's whole read side: it must agree byte for byte with what
/// [`add_typed_reference`] writes, including for the CLI-only `habit:` kind.
pub fn contains_reference(body: &str, target: &str) -> bool {
    parse_reference_token(target)
        .is_some_and(|(kind, id)| body.contains(&reference_token(kind, id)))
}

/// Append one reference token to a note body.
///
/// An already-present reference returns the body unchanged with `changed`
/// false: a reference is set membership, not a counter, and re-adding it must
/// not publish a new revision. The bytes appended are exactly the CLI's.
pub fn add_typed_reference(body: &str, target: &str) -> (String, bool) {
    let Some((kind, id)) = parse_reference_token(target) else {
        return (body.to_owned(), false);
    };
    if contains_reference(body, target) {
        return (body.to_owned(), false);
    }
    let mut text = body.to_owned();
    if !text.ends_with('\n') {
        text.push('\n');
    }
    text.push('\n');
    text.push_str(&reference_token(kind, id));
    text.push('\n');
    (text, true)
}

/// Remove every occurrence of one reference token.
///
/// An absent reference returns the body unchanged with `changed` false. Only the
/// canonical token is removed: a hand-written or half-edited link is left alone
/// rather than partially deleted.
pub fn remove_typed_reference(body: &str, target: &str) -> (String, bool) {
    let Some((kind, id)) = parse_reference_token(target) else {
        return (body.to_owned(), false);
    };
    if !contains_reference(body, target) {
        return (body.to_owned(), false);
    }
    (body.replace(&reference_token(kind, id), ""), true)
}

/// The entity kinds the authority itself owns and can resolve. Declared in
/// reference order (project before task) so "ordered kind then id" is the
/// enum's own ordering rather than a separate comparator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TypedReferenceKind {
    Project,
    Task,
}

impl TypedReferenceKind {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "project" => Some(Self::Project),
            "task" => Some(Self::Task),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Project => "project",
            Self::Task => "task",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct TypedReference {
    pub kind: TypedReferenceKind,
    pub id: String,
}

impl TypedReference {
    /// `<kind>:<id>`, the form the command envelope and the read query use.
    pub fn as_target(&self) -> String {
        format!("{}:{}", self.kind.as_str(), self.id)
    }
}

/// Parse a target that must name an authority entity. A malformed token and an
/// unsupported kind are different refusals, so a client can tell a typo from a
/// feature this server does not have.
pub fn authority_target(target: &str) -> Result<TypedReference, &'static str> {
    let (kind, id) = parse_reference_token(target).ok_or(INVALID_REFERENCE_TARGET)?;
    let kind = TypedReferenceKind::parse(kind).ok_or(UNSUPPORTED_REFERENCE_KIND)?;
    Ok(TypedReference {
        kind,
        id: id.to_owned(),
    })
}

/// Parse `<kind>:<id>` when the kind names an authority entity.
///
/// `None` covers both a malformed token and an unsupported kind; callers that
/// must refuse the two differently use [`authority_target`] instead.
pub fn parse_typed_reference(target: &str) -> Option<TypedReference> {
    authority_target(target).ok()
}

/// The canonical token for one typed reference.
///
/// The typed form of [`reference_token`]: the id comes from a parsed reference,
/// so callers cannot accidentally serialise an unvalidated string into a link.
pub fn typed_reference_token(reference: &TypedReference) -> String {
    reference_token(reference.kind.as_str(), &reference.id)
}

/// Every distinct authority reference in one note body, ordered kind then id.
///
/// The CLI-only kinds are dropped here by construction: they are not entities
/// this module can name, and a reference read must not invent one over HTTP.
pub fn find_typed_references(body: &str) -> Vec<TypedReference> {
    let mut references = BTreeSet::new();
    for (kind, id) in canonical_targets(body) {
        if let Some(kind) = TypedReferenceKind::parse(kind) {
            references.insert(TypedReference {
                kind,
                id: id.to_owned(),
            });
        }
    }
    references.into_iter().collect()
}

/// Add or remove, as the command envelope says it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkdownReferenceAction {
    Add,
    Remove,
}

impl MarkdownReferenceAction {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "add" => Some(Self::Add),
            "remove" => Some(Self::Remove),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Add => "add",
            Self::Remove => "remove",
        }
    }
}

/// The reference command payload, exactly as it arrives and as it is journaled.
///
/// `action` stays a string so an unknown action is refused with the fixed
/// envelope message instead of a deserializer's diagnostics.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MarkdownReferenceCommand {
    pub note_id: String,
    pub target: String,
    pub action: String,
}

/// The accepted answer. `changed` says whether *this* invocation published
/// bytes; a replay of an already-accepted command reports the stored answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MarkdownReferenceReceipt {
    pub command_id: String,
    pub note_id: String,
    pub revision: i64,
    pub changed: bool,
    pub target: String,
    pub action: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReferenceNote {
    pub id: String,
    pub path: String,
    pub title: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarkdownReferences {
    pub target: String,
    pub notes: Vec<ReferenceNote>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectKnowledgeNote {
    pub id: String,
    pub path: String,
    pub title: String,
    /// Every reference that caused this note's inclusion, ordered kind then id.
    pub references: Vec<TypedReference>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectKnowledge {
    pub project_id: String,
    pub notes: Vec<ProjectKnowledgeNote>,
    /// The project's task ids, so a client can name a reference without a
    /// second lookup.
    pub tasks: Vec<String>,
}

/// One physical pass over the vault: every note with the exact bytes that were
/// used to identify it.
///
/// Discovery already walks the whole tree, so reads that inspect every note
/// must not call the per-note lookup, which would repeat that walk once per
/// note. No index is maintained here: the files stay the only source of truth.
pub(crate) fn scan_notes(
    operations: &KnowledgeOperations,
) -> Result<Vec<(KnowledgeDocumentInfo, String)>, KnowledgeServiceError> {
    let service = KnowledgeService::new(operations);
    let mut notes = Vec::new();
    for info in service.discover()? {
        let body = operations.vault().read_markdown(&info.path)?;
        // Discovery and this read are not one atomic snapshot. A file rewritten
        // in between must fail closed rather than be attributed to the id it
        // used to carry.
        if KnowledgeService::identity(&body)? != info.id {
            return Err(KnowledgeServiceError::IdentityConflict);
        }
        notes.push((info, body));
    }
    Ok(notes)
}

/// Notes carrying the exact token for `target`, ordered by path.
///
/// A reference to a deleted entity is still returned: the note really does say
/// it, and only the caller knows whether to offer the broken link or drop it.
pub(crate) fn reference_notes(
    notes: &[(KnowledgeDocumentInfo, String)],
    target: &TypedReference,
) -> Vec<ReferenceNote> {
    let mut found: Vec<ReferenceNote> = notes
        .iter()
        .filter(|(_, body)| find_typed_references(body).contains(target))
        .map(|(info, _)| ReferenceNote {
            id: info.id.clone(),
            path: info.path.clone(),
            title: info.title.clone(),
        })
        .collect();
    found.sort_by(|a, b| a.path.cmp(&b.path).then_with(|| a.id.cmp(&b.id)));
    found
}

/// Notes referencing `project_id` or one of its live tasks.
///
/// Only the references that caused the inclusion are reported: a note may also
/// mention unrelated projects, and listing those here would make the project
/// view claim relationships it does not have.
pub(crate) fn project_knowledge_notes(
    notes: &[(KnowledgeDocumentInfo, String)],
    project_id: &str,
    tasks: &[String],
) -> Vec<ProjectKnowledgeNote> {
    let tasks: BTreeSet<&str> = tasks.iter().map(String::as_str).collect();
    let mut found = Vec::new();
    for (info, body) in notes {
        let references: Vec<TypedReference> = find_typed_references(body)
            .into_iter()
            .filter(|reference| match reference.kind {
                TypedReferenceKind::Project => reference.id == project_id,
                TypedReferenceKind::Task => tasks.contains(reference.id.as_str()),
            })
            .collect();
        if references.is_empty() {
            continue;
        }
        found.push(ProjectKnowledgeNote {
            id: info.id.clone(),
            path: info.path.clone(),
            title: info.title.clone(),
            references,
        });
    }
    found.sort_by(|a, b| a.path.cmp(&b.path).then_with(|| a.id.cmp(&b.id)));
    found
}

/// Durable intent record for the reference command.
///
/// The derived `MarkdownCommand::Update` lives in the existing Markdown ledger,
/// but it cannot answer "was *this* command already accepted?": the same
/// reference command derives different expected bytes once the token is
/// present, so replaying it through that ledger's payload comparison would
/// report a conflict for a legitimate retry. This small journal owns exactly
/// that question. It is not an index over notes; it holds one row per accepted
/// command id.
pub(crate) fn schema(store: &AuthorityStore) -> Result<(), AuthorityError> {
    store.conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS markdown_reference_commands (
            command_id TEXT PRIMARY KEY NOT NULL,
            actor TEXT NOT NULL,
            payload TEXT NOT NULL,
            receipt TEXT NOT NULL
        );",
    )?;
    Ok(())
}

/// The stored answer for an already-accepted command id.
///
/// A reused id with a different actor or payload is the same immutable-sequence
/// conflict the Markdown ledger reports, so one id always names one intent.
pub(crate) fn replayed(
    store: &AuthorityStore,
    actor: &str,
    command_id: &str,
    payload: &str,
) -> Result<Option<MarkdownReferenceReceipt>, AuthorityError> {
    use rusqlite::OptionalExtension;
    let prior: Option<(String, String, String)> = store
        .conn
        .query_row(
            "SELECT actor,payload,receipt FROM markdown_reference_commands WHERE command_id=?1",
            [command_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;
    match prior {
        None => Ok(None),
        Some((stored_actor, stored_payload, receipt))
            if stored_actor == actor && stored_payload == payload =>
        {
            serde_json::from_str(&receipt)
                .map(Some)
                .map_err(|_| AuthorityError::Invalid("invalid reference receipt"))
        }
        Some(_) => Err(AuthorityError::ImmutableConflict),
    }
}

/// Record one accepted answer.
///
/// This is written after the Markdown ledger accepts, in its own statement. A
/// crash in between leaves the body published and the command unrecorded; the
/// next attempt then recomputes an idempotent mutation (the token is already
/// present or already absent) and reports `changed: false`. Publication stays
/// exactly-once because the text mutation itself is a no-op, not because of
/// this row.
pub(crate) fn record(
    store: &mut AuthorityStore,
    actor: &str,
    command_id: &str,
    payload: &str,
    receipt: &MarkdownReferenceReceipt,
) -> Result<(), AuthorityError> {
    let encoded = serde_json::to_string(receipt)
        .map_err(|_| AuthorityError::Invalid("reference receipt serialization failed"))?;
    store.conn.execute(
        "INSERT INTO markdown_reference_commands(command_id,actor,payload,receipt) VALUES(?1,?2,?3,?4)",
        rusqlite::params![command_id, actor, payload, encoded],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        AuthoritativeHost,
        domain_commands::{ProjectCommandAction, ProjectCommandRequest, TaskAction, TaskCommand},
        markdown_commands::MarkdownCommand,
    };

    fn host() -> AuthoritativeHost {
        let host = AuthoritativeHost::open(":memory:").expect("in-memory authority");
        host.knowledge_vault
            .vault()
            .create_folder("Work")
            .expect("folder");
        host
    }

    fn project(host: &mut AuthoritativeHost, id: &str) {
        let revision = host.projects().expect("projects").0;
        host.project_command(
            "device-a",
            ProjectCommandRequest {
                command_id: format!("create-{id}"),
                expected_revision: revision,
                action: ProjectCommandAction::Create,
                project_id: id.into(),
                name: id.into(),
                status: None,
                workflow_id: None,
            },
        )
        .expect("project accepted");
    }

    fn task(host: &mut AuthoritativeHost, id: &str, project_id: &str) {
        let revision = host.projects().expect("projects").0;
        host.task_command(
            "device-a",
            TaskCommand {
                command_id: format!("create-{id}"),
                expected_revision: revision,
                action: TaskAction::Create {
                    task_id: id.into(),
                    title: id.into(),
                    project_id: Some(project_id.into()),
                    milestone_id: None,
                    description: String::new(),
                    status: None,
                    priority: None,
                    estimated_minutes: None,
                    deadline_at_ms: None,
                    earliest_start_at_ms: None,
                    acceptance_criteria: Vec::new(),
                    prerequisite_ids: Vec::new(),
                },
            },
        )
        .expect("task accepted");
    }

    fn note(host: &mut AuthoritativeHost, id: &str, path: &str, body: &str) {
        host.execute_markdown_command(
            "device-a",
            &format!("create-{id}"),
            MarkdownCommand::Create {
                note_id: id.into(),
                path: path.into(),
                body: body.into(),
            },
        )
        .expect("note accepted");
    }

    fn reference_command(note_id: &str, target: &str, action: &str) -> MarkdownReferenceCommand {
        MarkdownReferenceCommand {
            note_id: note_id.into(),
            target: target.into(),
            action: action.into(),
        }
    }

    // ---- grammar -----------------------------------------------------------

    #[test]
    fn token_round_trips_through_parse_and_serialise() {
        let longest = "a".repeat(MAX_REFERENCE_ID_LEN);
        for (kind, id) in [
            ("project", "orbit"),
            ("task", "task-1"),
            ("task", "A_b-9"),
            ("project", longest.as_str()),
        ] {
            let token = reference_token(kind, id);
            let target = format!("{kind}:{id}");
            assert_eq!(parse_reference_token(&target), Some((kind, id)));
            let body = add_typed_reference("", &target).0;
            assert!(body.contains(&token), "{body:?} must contain {token:?}");
            assert!(contains_reference(&body, &target));
            let found = find_typed_references(&body);
            assert_eq!(found.len(), 1);
            assert_eq!(found[0].id, id);
            assert_eq!(found[0].as_target(), target);
            assert_eq!(parse_typed_reference(&target), Some(found[0].clone()));
            assert_eq!(typed_reference_token(&found[0]), token);
        }
    }

    #[test]
    fn id_boundaries_are_enforced_on_both_sides() {
        assert!(valid_reference_id(&"a".repeat(128)));
        assert!(!valid_reference_id(&"a".repeat(129)));
        assert!(!valid_reference_id(""));
        assert!(!valid_reference_id("has space"));
        assert!(!valid_reference_id("dot.dot"));
        assert!(!valid_reference_id("slash/slash"));
        assert_eq!(parse_reference_token("project:"), None);
        assert_eq!(parse_reference_token(":orbit"), None);
        assert_eq!(parse_reference_token("project"), None);
        assert_eq!(parse_reference_token("Project:orbit"), None);
        assert_eq!(parse_reference_token("project:orbit:extra"), None);
    }

    #[test]
    fn only_the_canonical_symmetric_token_is_a_reference() {
        // A label, a stale destination and prose that merely looks like a link
        // are not references: the writer never produces them, so reading them
        // would create relationships no command ever committed.
        for body in [
            "[Orbit](orbit://project:orbit)",
            "[project:other](orbit://project:orbit)",
            "[project:orbit](orbit://project:orbit2)",
            "[project:orbit](project:orbit)",
            "project:orbit",
        ] {
            assert!(
                find_typed_references(body).is_empty(),
                "{body:?} is not a typed reference"
            );
            assert!(!contains_reference(body, "project:orbit"));
        }
        // The exact token is not a prefix match for a longer id.
        let body = "[project:orbit2](orbit://project:orbit2)";
        assert!(!contains_reference(body, "project:orbit"));
        assert!(contains_reference(body, "project:orbit2"));
    }

    #[test]
    fn references_are_deduplicated_and_ordered_kind_then_id() {
        let body = "\
[task:b](orbit://task:b)
[project:orbit](orbit://project:orbit)
[task:a](orbit://task:a)
[project:orbit](orbit://project:orbit)
";
        let found = find_typed_references(body);
        let targets: Vec<String> = found.iter().map(TypedReference::as_target).collect();
        assert_eq!(targets, vec!["project:orbit", "task:a", "task:b"]);
    }

    #[test]
    fn adding_twice_is_a_no_op_and_removing_restores_the_body() {
        let base = "---\norbit_id: note_1\n---\n# Example\n";
        let (added, changed) = add_typed_reference(base, "project:orbit");
        assert!(changed);
        assert!(added.ends_with("\n\n[project:orbit](orbit://project:orbit)\n"));
        let (again, changed) = add_typed_reference(&added, "project:orbit");
        assert!(!changed);
        assert_eq!(again, added);
        let (removed, changed) = remove_typed_reference(&added, "project:orbit");
        assert!(changed);
        assert!(!contains_reference(&removed, "project:orbit"));
        // Removal deletes the token and leaves the surrounding bytes exactly as
        // the CLI always has. Re-flowing the body here would be a second
        // grammar, and the note is the user's text, not ours to tidy.
        assert_eq!(removed, format!("{base}\n\n"));
        let (absent, changed) = remove_typed_reference(base, "project:orbit");
        assert!(!changed);
        assert_eq!(absent, base);
    }

    #[test]
    fn a_malformed_target_is_never_rewritten() {
        let base = "[project:orbit](orbit://project:orbit)\n";
        for target in ["project:", "nocolon", "project:bad id"] {
            assert_eq!(add_typed_reference(base, target), (base.to_owned(), false));
            assert_eq!(
                remove_typed_reference(base, target),
                (base.to_owned(), false)
            );
        }
    }

    #[test]
    fn habit_stays_a_cli_only_kind_but_its_bytes_are_shared() {
        // The byte grammar is kind-agnostic so the CLI keeps `habit:` without a
        // second parser; the authority never names it.
        let (body, changed) = add_typed_reference("", "habit:stretch");
        assert!(changed);
        assert!(contains_reference(&body, "habit:stretch"));
        assert!(find_typed_references(&body).is_empty());
        assert_eq!(
            authority_target("habit:stretch"),
            Err(UNSUPPORTED_REFERENCE_KIND)
        );
    }

    // ---- command -----------------------------------------------------------

    #[test]
    fn add_then_remove_publishes_one_revision_each_and_no_ops_afterwards() {
        let mut host = host();
        project(&mut host, "orbit");
        note(&mut host, "note-a", "Work/a.md", "# A\n");
        let added = host
            .execute_markdown_reference_command(
                "device-a",
                "ref-add",
                reference_command("note-a", "project:orbit", "add"),
            )
            .expect("accepted");
        assert!(added.changed);
        assert_eq!(added.revision, 2);
        assert_eq!(added.target, "project:orbit");
        assert_eq!(added.action, "add");
        assert_eq!(added.note_id, "note-a");
        let body = host.markdown_by_id("note-a").unwrap().unwrap().1;
        assert!(contains_reference(&body, "project:orbit"));

        let again = host
            .execute_markdown_reference_command(
                "device-a",
                "ref-add-2",
                reference_command("note-a", "project:orbit", "add"),
            )
            .expect("accepted");
        assert!(!again.changed);
        assert_eq!(again.revision, 2, "a no-op must not advance the revision");
        assert_eq!(host.markdown_by_id("note-a").unwrap().unwrap().1, body);

        let removed = host
            .execute_markdown_reference_command(
                "device-a",
                "ref-remove",
                reference_command("note-a", "project:orbit", "remove"),
            )
            .expect("accepted");
        assert!(removed.changed);
        assert_eq!(removed.revision, 3);
        assert!(!contains_reference(
            &host.markdown_by_id("note-a").unwrap().unwrap().1,
            "project:orbit"
        ));

        let absent = host
            .execute_markdown_reference_command(
                "device-a",
                "ref-remove-2",
                reference_command("note-a", "project:orbit", "remove"),
            )
            .expect("accepted");
        assert!(!absent.changed);
        assert_eq!(absent.revision, 3);
    }

    #[test]
    fn a_replayed_command_id_returns_the_stored_answer() {
        let mut host = host();
        project(&mut host, "orbit");
        note(&mut host, "note-a", "Work/a.md", "# A\n");
        let first = host
            .execute_markdown_reference_command(
                "device-a",
                "ref-1",
                reference_command("note-a", "project:orbit", "add"),
            )
            .expect("accepted");
        let replay = host
            .execute_markdown_reference_command(
                "device-a",
                "ref-1",
                reference_command("note-a", "project:orbit", "add"),
            )
            .expect("replayed");
        assert_eq!(replay, first);
        assert_eq!(host.markdown_note_revision("note-a").unwrap(), 2);
        let body = host.markdown_by_id("note-a").unwrap().unwrap().1;
        assert_eq!(body.matches("[project:orbit]").count(), 1);
    }

    #[test]
    fn a_reused_command_id_with_a_different_payload_is_an_immutable_conflict() {
        let mut host = host();
        project(&mut host, "orbit");
        project(&mut host, "web");
        note(&mut host, "note-a", "Work/a.md", "# A\n");
        host.execute_markdown_reference_command(
            "device-a",
            "ref-1",
            reference_command("note-a", "project:orbit", "add"),
        )
        .expect("accepted");
        for command in [
            reference_command("note-a", "project:web", "add"),
            reference_command("note-a", "project:orbit", "remove"),
        ] {
            assert!(matches!(
                host.execute_markdown_reference_command("device-a", "ref-1", command),
                Err(AuthorityError::ImmutableConflict)
            ));
        }
        assert_eq!(host.markdown_note_revision("note-a").unwrap(), 2);
    }

    #[test]
    fn an_unknown_entity_and_an_unsupported_kind_are_refused_without_writing() {
        let mut host = host();
        project(&mut host, "orbit");
        note(&mut host, "note-a", "Work/a.md", "# A\n");
        for (target, expected) in [
            ("project:missing", UNKNOWN_REFERENCE_TARGET),
            ("task:missing", UNKNOWN_REFERENCE_TARGET),
            ("habit:stretch", UNSUPPORTED_REFERENCE_KIND),
            ("work_item:w-1", UNSUPPORTED_REFERENCE_KIND),
        ] {
            assert!(matches!(
                host.execute_markdown_reference_command(
                    "device-a",
                    "ref-1",
                    reference_command("note-a", target, "add"),
                ),
                Err(AuthorityError::Invalid(message)) if message == expected
            ));
        }
        // The refused ids were never journalled, so a corrected command with the
        // same id still succeeds.
        host.execute_markdown_reference_command(
            "device-a",
            "ref-1",
            reference_command("note-a", "project:orbit", "add"),
        )
        .expect("accepted after refusal");
        assert_eq!(host.markdown_note_revision("note-a").unwrap(), 2);
    }

    #[test]
    fn malformed_envelopes_are_refused_before_any_vault_read() {
        let mut host = host();
        project(&mut host, "orbit");
        note(&mut host, "note-a", "Work/a.md", "# A\n");
        let cases = [
            (
                reference_command("", "project:orbit", "add"),
                INVALID_NOTE_ID,
            ),
            (
                reference_command("note-a", "project", "add"),
                INVALID_REFERENCE_COMMAND,
            ),
            (
                reference_command("note-a", "project:orbit", "replace"),
                INVALID_REFERENCE_COMMAND,
            ),
            (
                reference_command("note-a", "project:bad id", "add"),
                INVALID_REFERENCE_COMMAND,
            ),
        ];
        for (command, expected) in cases {
            assert!(matches!(
                host.execute_markdown_reference_command("device-a", "ref-1", command),
                Err(AuthorityError::Invalid(message)) if message == expected
            ));
        }
        assert!(matches!(
            host.execute_markdown_reference_command(
                "device-a",
                "update-forbidden",
                reference_command("note-a", "project:orbit", "add"),
            ),
            Err(AuthorityError::Invalid(INVALID_REFERENCE_COMMAND))
        ));
        assert_eq!(host.markdown_note_revision("note-a").unwrap(), 1);
    }

    #[test]
    fn a_missing_note_is_refused_without_touching_the_entity() {
        let mut host = host();
        project(&mut host, "orbit");
        assert!(matches!(
            host.execute_markdown_reference_command(
                "device-a",
                "ref-1",
                reference_command("note-missing", "project:orbit", "add"),
            ),
            Err(AuthorityError::Invalid(NOTE_DOES_NOT_EXIST))
        ));
    }

    // ---- reads -------------------------------------------------------------

    #[test]
    fn reference_read_is_ordered_by_path_and_keeps_broken_references() {
        let mut host = host();
        project(&mut host, "orbit");
        task(&mut host, "task-1", "orbit");
        note(&mut host, "note-b", "Work/b.md", "# B\n");
        note(&mut host, "note-a", "Work/a.md", "# A\n");
        note(&mut host, "note-unrelated", "Work/u.md", "# U\n");
        for (command, note_id, target) in [
            ("ref-b", "note-b", "project:orbit"),
            ("ref-a", "note-a", "project:orbit"),
            ("ref-u", "note-unrelated", "task:task-1"),
        ] {
            host.execute_markdown_reference_command(
                "device-a",
                command,
                reference_command(note_id, target, "add"),
            )
            .expect("accepted");
        }
        let read = host.markdown_references("project:orbit").expect("read");
        assert_eq!(read.target, "project:orbit");
        let paths: Vec<&str> = read.notes.iter().map(|n| n.path.as_str()).collect();
        assert_eq!(paths, vec!["Work/a.md", "Work/b.md"]);

        // A reference to an entity that no longer exists is still returned.
        host.authority
            .conn
            .execute("DELETE FROM tasks WHERE id='task-1'", [])
            .expect("task removed");
        let broken = host.markdown_references("task:task-1").expect("read");
        assert_eq!(broken.notes.len(), 1);
        assert_eq!(broken.notes[0].id, "note-unrelated");
    }

    #[test]
    fn reference_read_refuses_a_bad_target_rather_than_answering_empty() {
        let host = host();
        assert!(matches!(
            host.markdown_references("project"),
            Err(AuthorityError::Invalid(INVALID_REFERENCE_TARGET))
        ));
        assert!(matches!(
            host.markdown_references("habit:stretch"),
            Err(AuthorityError::Invalid(UNSUPPORTED_REFERENCE_KIND))
        ));
    }

    #[test]
    fn project_knowledge_is_the_union_of_project_and_task_references() {
        let mut host = host();
        project(&mut host, "orbit");
        project(&mut host, "web");
        task(&mut host, "task-1", "orbit");
        task(&mut host, "task-2", "orbit");
        task(&mut host, "task-web", "web");
        note(&mut host, "note-project", "Work/p.md", "# P\n");
        note(&mut host, "note-task", "Work/t.md", "# T\n");
        note(&mut host, "note-both", "Work/b.md", "# B\n");
        note(&mut host, "note-other", "Work/o.md", "# O\n");
        for (command, note_id, targets) in [
            ("ref-p", "note-project", vec!["project:orbit"]),
            ("ref-t", "note-task", vec!["task:task-2"]),
            (
                "ref-b",
                "note-both",
                vec!["task:task-1", "project:orbit", "task:task-web"],
            ),
            ("ref-o", "note-other", vec!["project:web"]),
        ] {
            for (index, target) in targets.into_iter().enumerate() {
                let command = format!("{command}-{index}");
                host.execute_markdown_reference_command(
                    "device-a",
                    &command,
                    reference_command(note_id, target, "add"),
                )
                .expect("accepted");
            }
        }
        let knowledge = host.project_knowledge("orbit").expect("read");
        assert_eq!(knowledge.project_id, "orbit");
        assert_eq!(knowledge.tasks, vec!["task-1", "task-2"]);
        let paths: Vec<&str> = knowledge.notes.iter().map(|n| n.path.as_str()).collect();
        assert_eq!(paths, vec!["Work/b.md", "Work/p.md", "Work/t.md"]);
        let both = knowledge
            .notes
            .iter()
            .find(|note| note.id == "note-both")
            .expect("union note");
        let targets: Vec<String> = both
            .references
            .iter()
            .map(TypedReference::as_target)
            .collect();
        // A reference to another project's task caused no inclusion and is not
        // reported as part of this project's knowledge.
        assert_eq!(targets, vec!["project:orbit", "task:task-1"]);
    }

    #[test]
    fn an_empty_project_answers_with_empty_lists() {
        let mut host = host();
        project(&mut host, "empty");
        note(&mut host, "note-a", "Work/a.md", "# A\n");
        let knowledge = host.project_knowledge("empty").expect("read");
        assert!(knowledge.notes.is_empty());
        assert!(knowledge.tasks.is_empty());
        assert!(matches!(
            host.project_knowledge("missing"),
            Err(AuthorityError::Invalid(UNKNOWN_REFERENCE_TARGET))
        ));
    }

    #[test]
    fn a_note_referencing_a_deleted_task_stays_in_the_reference_read() {
        let mut host = host();
        project(&mut host, "orbit");
        task(&mut host, "task-1", "orbit");
        note(&mut host, "note-a", "Work/a.md", "# A\n");
        host.execute_markdown_reference_command(
            "device-a",
            "ref-1",
            reference_command("note-a", "task:task-1", "add"),
        )
        .expect("accepted");
        host.authority
            .conn
            .execute("DELETE FROM tasks WHERE id='task-1'", [])
            .expect("task removed");

        let direct = host.markdown_references("task:task-1").expect("read");
        assert_eq!(direct.notes.len(), 1);
        let knowledge = host.project_knowledge("orbit").expect("read");
        assert!(
            knowledge.notes.is_empty(),
            "a deleted task cannot include a note in live project knowledge"
        );
        assert!(knowledge.tasks.is_empty());
    }

    #[test]
    fn malformed_vault_notes_fail_closed_instead_of_reading_empty() {
        let mut host = host();
        project(&mut host, "orbit");
        note(&mut host, "note-a", "Work/a.md", "# A\n");
        host.knowledge_vault
            .vault()
            .create_markdown("Work/orphan.md", "# no frontmatter\n")
            .expect("raw file");
        // Both reads scan the whole vault, so one unreadable note must not turn
        // into "this project has no knowledge".
        assert!(matches!(
            host.markdown_references("project:orbit"),
            Err(AuthorityError::Knowledge(_))
        ));
        assert!(matches!(
            host.project_knowledge("orbit"),
            Err(AuthorityError::Knowledge(_))
        ));
    }
}
