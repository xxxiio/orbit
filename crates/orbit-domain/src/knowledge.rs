use crate::{DomainError, HabitId, KnowledgeItemId, MilestoneId, ProjectId, TaskId};

/// Preset Orbit knowledge categories. Orbit intentionally does not expose an
/// arbitrary user-defined database/schema builder; these kinds carry stable
/// semantics for retrieval, UI, AI context, and external agents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum KnowledgeKind {
    Note,
    Decision,
    Finding,
    Assumption,
    Risk,
    Reference,
    Concept,
    Resource,
    MeetingNote,
    DailyNote,
    ProjectContext,
}

impl KnowledgeKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Note => "note",
            Self::Decision => "decision",
            Self::Finding => "finding",
            Self::Assumption => "assumption",
            Self::Risk => "risk",
            Self::Reference => "reference",
            Self::Concept => "concept",
            Self::Resource => "resource",
            Self::MeetingNote => "meeting_note",
            Self::DailyNote => "daily_note",
            Self::ProjectContext => "project_context",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum KnowledgeProvenance {
    Human,
    Agent(String),
    Import(String),
    Automation(String),
}

impl KnowledgeProvenance {
    pub fn validate(self) -> Result<Self, DomainError> {
        let source = match &self {
            Self::Human => return Ok(self),
            Self::Agent(source) | Self::Import(source) | Self::Automation(source) => source,
        };
        if source.trim().is_empty() {
            return Err(DomainError::EmptyKnowledgeProvenance);
        }
        Ok(self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KnowledgeStatus {
    Active,
    Resolved,
    Superseded,
    Archived,
}

impl KnowledgeStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Resolved => "resolved",
            Self::Superseded => "superseded",
            Self::Archived => "archived",
        }
    }

    pub fn can_transition_to(self, next: Self) -> bool {
        if self == next {
            return true;
        }
        match self {
            Self::Active => matches!(next, Self::Resolved | Self::Superseded | Self::Archived),
            Self::Resolved | Self::Superseded => matches!(next, Self::Archived),
            Self::Archived => false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum KnowledgeTarget {
    Project(ProjectId),
    Milestone(MilestoneId),
    Task(TaskId),
    Habit(HabitId),
    Knowledge(KnowledgeItemId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KnowledgeRelationKind {
    About,
    Supports,
    DependsOn,
    Supersedes,
    References,
    Related,
}

impl KnowledgeRelationKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::About => "about",
            Self::Supports => "supports",
            Self::DependsOn => "depends_on",
            Self::Supersedes => "supersedes",
            Self::References => "references",
            Self::Related => "related",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KnowledgeRelation {
    kind: KnowledgeRelationKind,
    target: KnowledgeTarget,
}

impl KnowledgeRelation {
    pub fn new(kind: KnowledgeRelationKind, target: KnowledgeTarget) -> Self {
        Self { kind, target }
    }
    pub fn kind(&self) -> KnowledgeRelationKind {
        self.kind
    }
    pub fn target(&self) -> &KnowledgeTarget {
        &self.target
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KnowledgeItem {
    id: KnowledgeItemId,
    kind: KnowledgeKind,
    title: String,
    body_markdown: String,
    status: KnowledgeStatus,
    provenance: KnowledgeProvenance,
    created_at_ms: i64,
    updated_at_ms: i64,
    relations: Vec<KnowledgeRelation>,
}

impl KnowledgeItem {
    pub fn new(
        id: KnowledgeItemId,
        kind: KnowledgeKind,
        title: impl Into<String>,
        body_markdown: impl Into<String>,
        provenance: KnowledgeProvenance,
        created_at_ms: i64,
    ) -> Result<Self, DomainError> {
        let title = title.into();
        if title.trim().is_empty() {
            return Err(DomainError::EmptyKnowledgeTitle);
        }
        Ok(Self {
            id,
            kind,
            title,
            body_markdown: body_markdown.into(),
            status: KnowledgeStatus::Active,
            provenance: provenance.validate()?,
            created_at_ms,
            updated_at_ms: created_at_ms,
            relations: Vec::new(),
        })
    }

    /// Reconstructs a persisted/synced knowledge item after validating the same
    /// invariants enforced by normal mutations. Infrastructure adapters use
    /// this instead of replaying lifecycle transitions solely to recover state.
    // Keep the persisted-record restoration signature stable across storage and sync callers.
    #[allow(clippy::too_many_arguments)]
    pub fn restore(
        id: KnowledgeItemId,
        kind: KnowledgeKind,
        title: impl Into<String>,
        body_markdown: impl Into<String>,
        status: KnowledgeStatus,
        provenance: KnowledgeProvenance,
        created_at_ms: i64,
        updated_at_ms: i64,
        relations: Vec<KnowledgeRelation>,
    ) -> Result<Self, DomainError> {
        let title = title.into();
        if title.trim().is_empty() {
            return Err(DomainError::EmptyKnowledgeTitle);
        }
        if updated_at_ms < created_at_ms {
            return Err(DomainError::KnowledgeTimestampRegression);
        }
        let provenance = provenance.validate()?;
        let mut seen = Vec::new();
        for relation in &relations {
            if matches!(relation.target(), KnowledgeTarget::Knowledge(target) if target == &id) {
                return Err(DomainError::SelfKnowledgeRelation);
            }
            if seen.contains(relation) {
                return Err(DomainError::DuplicateKnowledgeRelation);
            }
            seen.push(relation.clone());
        }
        Ok(Self {
            id,
            kind,
            title,
            body_markdown: body_markdown.into(),
            status,
            provenance,
            created_at_ms,
            updated_at_ms,
            relations,
        })
    }

    pub fn id(&self) -> &KnowledgeItemId {
        &self.id
    }
    pub fn kind(&self) -> KnowledgeKind {
        self.kind
    }
    pub fn title(&self) -> &str {
        &self.title
    }
    pub fn body_markdown(&self) -> &str {
        &self.body_markdown
    }
    pub fn status(&self) -> KnowledgeStatus {
        self.status
    }
    pub fn provenance(&self) -> &KnowledgeProvenance {
        &self.provenance
    }
    pub fn created_at_ms(&self) -> i64 {
        self.created_at_ms
    }
    pub fn updated_at_ms(&self) -> i64 {
        self.updated_at_ms
    }
    pub fn relations(&self) -> &[KnowledgeRelation] {
        &self.relations
    }

    pub fn update_content(
        &mut self,
        title: impl Into<String>,
        body_markdown: impl Into<String>,
        updated_at_ms: i64,
    ) -> Result<(), DomainError> {
        let title = title.into();
        if title.trim().is_empty() {
            return Err(DomainError::EmptyKnowledgeTitle);
        }
        if updated_at_ms < self.updated_at_ms {
            return Err(DomainError::KnowledgeTimestampRegression);
        }
        self.title = title;
        self.body_markdown = body_markdown.into();
        self.updated_at_ms = updated_at_ms;
        Ok(())
    }

    pub fn transition_to(
        &mut self,
        next: KnowledgeStatus,
        updated_at_ms: i64,
    ) -> Result<(), DomainError> {
        if !self.status.can_transition_to(next) {
            return Err(DomainError::InvalidKnowledgeStatusTransition {
                from: self.status.as_str(),
                to: next.as_str(),
            });
        }
        if updated_at_ms < self.updated_at_ms {
            return Err(DomainError::KnowledgeTimestampRegression);
        }
        self.status = next;
        self.updated_at_ms = updated_at_ms;
        Ok(())
    }

    pub fn add_relation(
        &mut self,
        relation: KnowledgeRelation,
        updated_at_ms: i64,
    ) -> Result<(), DomainError> {
        if matches!(relation.target(), KnowledgeTarget::Knowledge(id) if id == &self.id) {
            return Err(DomainError::SelfKnowledgeRelation);
        }
        if self.relations.contains(&relation) {
            return Err(DomainError::DuplicateKnowledgeRelation);
        }
        if updated_at_ms < self.updated_at_ms {
            return Err(DomainError::KnowledgeTimestampRegression);
        }
        self.relations.push(relation);
        self.updated_at_ms = updated_at_ms;
        Ok(())
    }

    pub fn remove_relation(
        &mut self,
        relation: &KnowledgeRelation,
        updated_at_ms: i64,
    ) -> Result<bool, DomainError> {
        if updated_at_ms < self.updated_at_ms {
            return Err(DomainError::KnowledgeTimestampRegression);
        }
        let before = self.relations.len();
        self.relations.retain(|candidate| candidate != relation);
        let changed = before != self.relations.len();
        if changed {
            self.updated_at_ms = updated_at_ms;
        }
        Ok(changed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item() -> KnowledgeItem {
        KnowledgeItem::new(
            KnowledgeItemId::new("knowledge-1").unwrap(),
            KnowledgeKind::Decision,
            "Core language",
            "Use Rust for the Orbit engine.",
            KnowledgeProvenance::Human,
            100,
        )
        .unwrap()
    }

    #[test]
    fn knowledge_uses_preset_kind_and_structured_lifecycle() {
        let mut item = item();
        assert_eq!(item.kind(), KnowledgeKind::Decision);
        item.transition_to(KnowledgeStatus::Superseded, 200)
            .unwrap();
        assert_eq!(item.status(), KnowledgeStatus::Superseded);
        item.transition_to(KnowledgeStatus::Archived, 300).unwrap();
        assert_eq!(item.status(), KnowledgeStatus::Archived);
    }

    #[test]
    fn knowledge_can_link_to_execution_and_other_knowledge() {
        let mut item = item();
        item.add_relation(
            KnowledgeRelation::new(
                KnowledgeRelationKind::About,
                KnowledgeTarget::Project(ProjectId::new("orbit").unwrap()),
            ),
            101,
        )
        .unwrap();
        item.add_relation(
            KnowledgeRelation::new(
                KnowledgeRelationKind::References,
                KnowledgeTarget::Knowledge(KnowledgeItemId::new("knowledge-2").unwrap()),
            ),
            102,
        )
        .unwrap();
        assert_eq!(item.relations().len(), 2);
    }

    #[test]
    fn knowledge_rejects_self_links_and_blank_provenance_sources() {
        let mut item = item();
        assert_eq!(
            item.add_relation(
                KnowledgeRelation::new(
                    KnowledgeRelationKind::Related,
                    KnowledgeTarget::Knowledge(item.id().clone())
                ),
                101
            ),
            Err(DomainError::SelfKnowledgeRelation)
        );
        assert_eq!(
            KnowledgeProvenance::Agent(" ".into()).validate(),
            Err(DomainError::EmptyKnowledgeProvenance)
        );
    }

    #[test]
    fn timestamps_are_monotonic() {
        let mut item = item();
        assert_eq!(
            item.update_content("Changed", "Body", 99),
            Err(DomainError::KnowledgeTimestampRegression)
        );
    }
}
