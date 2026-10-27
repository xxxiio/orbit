use crate::{DomainError, ProjectId};

/// Built-in project lifecycle states.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProjectStatus {
    Proposed,
    Active,
    OnHold,
    Completed,
    Cancelled,
    Archived,
}

impl ProjectStatus {
    /// Every built-in status in canonical display order. Callers derive their
    /// offered edges through [`Self::can_transition_to`]; the transition table
    /// itself is never duplicated outside this module.
    pub const ALL: [Self; 6] = [
        Self::Proposed,
        Self::Active,
        Self::OnHold,
        Self::Completed,
        Self::Cancelled,
        Self::Archived,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Proposed => "proposed",
            Self::Active => "active",
            Self::OnHold => "on_hold",
            Self::Completed => "completed",
            Self::Cancelled => "cancelled",
            Self::Archived => "archived",
        }
    }

    pub fn can_transition_to(self, next: Self) -> bool {
        if self == next {
            return true;
        }
        match self {
            Self::Proposed => matches!(next, Self::Active | Self::Cancelled | Self::Archived),
            Self::Active => matches!(
                next,
                Self::OnHold | Self::Completed | Self::Cancelled | Self::Archived
            ),
            Self::OnHold => matches!(next, Self::Active | Self::Cancelled | Self::Archived),
            Self::Completed => matches!(next, Self::Archived),
            Self::Cancelled => matches!(next, Self::Archived),
            Self::Archived => false,
        }
    }

    /// Parse a persisted or wire status token. Unknown tokens are reported as
    /// absent so each caller keeps its own fixed refusal text.
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "proposed" => Some(Self::Proposed),
            "active" => Some(Self::Active),
            "on_hold" => Some(Self::OnHold),
            "completed" => Some(Self::Completed),
            "cancelled" => Some(Self::Cancelled),
            "archived" => Some(Self::Archived),
            _ => None,
        }
    }
}

/// Opinionated project archetypes. These are strategies, not separate aggregates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProjectMode {
    Standard,
    Learning,
    HabitDriven,
    MetricTarget,
    Maintenance,
    Custom,
}

impl ProjectMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Standard => "standard",
            Self::Learning => "learning",
            Self::HabitDriven => "habit_driven",
            Self::MetricTarget => "metric_target",
            Self::Maintenance => "maintenance",
            Self::Custom => "custom",
        }
    }

    pub fn parse(value: &str) -> Result<Self, DomainError> {
        match value {
            "standard" => Ok(Self::Standard),
            "learning" => Ok(Self::Learning),
            "habit_driven" => Ok(Self::HabitDriven),
            "metric_target" => Ok(Self::MetricTarget),
            "maintenance" => Ok(Self::Maintenance),
            "custom" => Ok(Self::Custom),
            _ => Err(DomainError::InvalidProjectMode),
        }
    }

    pub fn default_signals(self) -> Vec<ProjectProgressSignal> {
        match self {
            Self::Standard => vec![
                ProjectProgressSignal::Milestones,
                ProjectProgressSignal::Tasks,
                ProjectProgressSignal::Blockers,
                ProjectProgressSignal::Forecast,
            ],
            Self::Learning => vec![
                ProjectProgressSignal::Milestones,
                ProjectProgressSignal::LearningEvidence,
                ProjectProgressSignal::HabitConsistency,
                ProjectProgressSignal::TimeInvested,
            ],
            Self::HabitDriven => vec![
                ProjectProgressSignal::HabitConsistency,
                ProjectProgressSignal::TimeInvested,
            ],
            Self::MetricTarget => vec![
                ProjectProgressSignal::MetricValue,
                ProjectProgressSignal::Forecast,
            ],
            Self::Maintenance => vec![
                ProjectProgressSignal::MetricValue,
                ProjectProgressSignal::HabitConsistency,
                ProjectProgressSignal::Blockers,
            ],
            Self::Custom => vec![ProjectProgressSignal::Milestones],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ProjectProgressSignal {
    Milestones,
    Tasks,
    Blockers,
    Forecast,
    HabitConsistency,
    TimeInvested,
    LearningEvidence,
    MetricValue,
    KnowledgeEvidence,
}

impl ProjectProgressSignal {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Milestones => "milestones",
            Self::Tasks => "tasks",
            Self::Blockers => "blockers",
            Self::Forecast => "forecast",
            Self::HabitConsistency => "habit_consistency",
            Self::TimeInvested => "time_invested",
            Self::LearningEvidence => "learning_evidence",
            Self::MetricValue => "metric_value",
            Self::KnowledgeEvidence => "knowledge_evidence",
        }
    }

    pub fn parse(value: &str) -> Result<Self, DomainError> {
        match value {
            "milestones" => Ok(Self::Milestones),
            "tasks" => Ok(Self::Tasks),
            "blockers" => Ok(Self::Blockers),
            "forecast" => Ok(Self::Forecast),
            "habit_consistency" => Ok(Self::HabitConsistency),
            "time_invested" => Ok(Self::TimeInvested),
            "learning_evidence" => Ok(Self::LearningEvidence),
            "metric_value" => Ok(Self::MetricValue),
            "knowledge_evidence" => Ok(Self::KnowledgeEvidence),
            _ => Err(DomainError::InvalidProjectProgressSignal),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SuccessCriterionStatus {
    Pending,
    Satisfied,
    Waived,
}

impl SuccessCriterionStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Satisfied => "satisfied",
            Self::Waived => "waived",
        }
    }

    pub fn parse(value: &str) -> Result<Self, DomainError> {
        match value {
            "pending" => Ok(Self::Pending),
            "satisfied" => Ok(Self::Satisfied),
            "waived" => Ok(Self::Waived),
            _ => Err(DomainError::InvalidSuccessCriterionStatus),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectSuccessCriterion {
    key: String,
    description: String,
    required: bool,
    status: SuccessCriterionStatus,
}

impl ProjectSuccessCriterion {
    pub fn new(
        key: impl Into<String>,
        description: impl Into<String>,
        required: bool,
    ) -> Result<Self, DomainError> {
        let key = key.into();
        let description = description.into();
        if key.trim().is_empty() {
            return Err(DomainError::EmptySuccessCriterionKey);
        }
        if description.trim().is_empty() {
            return Err(DomainError::EmptySuccessCriterionDescription);
        }
        Ok(Self {
            key,
            description,
            required,
            status: SuccessCriterionStatus::Pending,
        })
    }
    pub fn key(&self) -> &str {
        &self.key
    }
    pub fn description(&self) -> &str {
        &self.description
    }
    pub fn required(&self) -> bool {
        self.required
    }
    pub fn status(&self) -> SuccessCriterionStatus {
        self.status
    }
    pub fn set_status(&mut self, status: SuccessCriterionStatus) {
        self.status = status;
    }
    pub fn is_complete(&self) -> bool {
        matches!(
            self.status,
            SuccessCriterionStatus::Satisfied | SuccessCriterionStatus::Waived
        )
    }
}

/// A bounded desired outcome. Project mode changes how Orbit interprets progress,
/// roadmap generation and completion evidence; it does not create a separate goal system.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Project {
    id: ProjectId,
    name: String,
    status: ProjectStatus,
    mode: ProjectMode,
    objective: Option<String>,
    progress_signals: Vec<ProjectProgressSignal>,
    success_criteria: Vec<ProjectSuccessCriterion>,
}

impl Project {
    /// Longest accepted project name, in bytes. The SQL authority rejects a
    /// longer name before it ever restores one into a domain aggregate, so both
    /// transports share one bound.
    pub const MAX_NAME_LEN: usize = 4096;

    pub fn new(id: ProjectId, name: impl Into<String>) -> Result<Self, DomainError> {
        let name = name.into();
        if name.trim().is_empty() {
            return Err(DomainError::EmptyProjectName);
        }
        let mode = ProjectMode::Standard;
        Ok(Self {
            id,
            name,
            status: ProjectStatus::Proposed,
            mode,
            objective: None,
            progress_signals: mode.default_signals(),
            success_criteria: Vec::new(),
        })
    }

    /// Rehydrate an accepted project from stored state.
    ///
    /// The canonical SQL row carries identity, name and status only, so the
    /// remaining facets keep the defaults [`Self::new`] assigns. Rehydration
    /// implies no lifecycle event and re-checks no transition: the stored
    /// status was accepted when it was written.
    pub fn restore(
        id: ProjectId,
        name: impl Into<String>,
        status: ProjectStatus,
    ) -> Result<Self, DomainError> {
        let mut project = Self::new(id, name)?;
        project.status = status;
        Ok(project)
    }

    /// Rename in place. Blank names are a domain invariant; length is bounded at
    /// the transport boundary through [`Self::MAX_NAME_LEN`], exactly like task
    /// titles, because the bound is about storage rather than identity.
    pub fn rename(&mut self, name: impl Into<String>) -> Result<(), DomainError> {
        let name = name.into();
        if name.trim().is_empty() {
            return Err(DomainError::EmptyProjectName);
        }
        self.name = name;
        Ok(())
    }

    pub fn id(&self) -> &ProjectId {
        &self.id
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn status(&self) -> ProjectStatus {
        self.status
    }
    pub fn mode(&self) -> ProjectMode {
        self.mode
    }
    pub fn objective(&self) -> Option<&str> {
        self.objective.as_deref()
    }
    pub fn progress_signals(&self) -> &[ProjectProgressSignal] {
        &self.progress_signals
    }
    pub fn success_criteria(&self) -> &[ProjectSuccessCriterion] {
        &self.success_criteria
    }

    pub fn set_mode(&mut self, mode: ProjectMode, reset_signals: bool) {
        self.mode = mode;
        if reset_signals {
            self.progress_signals = mode.default_signals();
        }
    }

    pub fn set_objective(&mut self, objective: Option<String>) -> Result<(), DomainError> {
        if objective
            .as_ref()
            .is_some_and(|value| value.trim().is_empty())
        {
            return Err(DomainError::EmptyProjectObjective);
        }
        self.objective = objective;
        Ok(())
    }

    pub fn set_progress_signals(
        &mut self,
        signals: Vec<ProjectProgressSignal>,
    ) -> Result<(), DomainError> {
        if signals.is_empty() {
            return Err(DomainError::EmptyProjectProgressSignals);
        }
        let mut deduped = Vec::with_capacity(signals.len());
        for signal in signals {
            if !deduped.contains(&signal) {
                deduped.push(signal);
            }
        }
        self.progress_signals = deduped;
        Ok(())
    }

    pub fn replace_success_criteria(
        &mut self,
        mut criteria: Vec<ProjectSuccessCriterion>,
    ) -> Result<(), DomainError> {
        let mut keys = std::collections::BTreeSet::new();
        for criterion in &criteria {
            if !keys.insert(criterion.key().to_owned()) {
                return Err(DomainError::DuplicateSuccessCriterionKey);
            }
        }
        // Criterion ordering is not semantic. Canonicalize by key so independently
        // merged sync fields converge to the same persisted/project representation.
        criteria.sort_by(|left, right| left.key().cmp(right.key()));
        self.success_criteria = criteria;
        Ok(())
    }

    pub fn set_success_criterion_status(
        &mut self,
        key: &str,
        status: SuccessCriterionStatus,
    ) -> Result<(), DomainError> {
        let criterion = self
            .success_criteria
            .iter_mut()
            .find(|item| item.key() == key)
            .ok_or(DomainError::UnknownSuccessCriterion)?;
        criterion.set_status(status);
        Ok(())
    }

    pub fn required_completion_ready(&self) -> bool {
        self.success_criteria
            .iter()
            .filter(|criterion| criterion.required())
            .all(ProjectSuccessCriterion::is_complete)
    }

    pub fn transition_to(&mut self, next: ProjectStatus) -> Result<(), DomainError> {
        if next == ProjectStatus::Completed && !self.required_completion_ready() {
            return Err(DomainError::ProjectCompletionCriteriaUnsatisfied);
        }
        if !self.status.can_transition_to(next) {
            return Err(DomainError::InvalidProjectStatusTransition {
                from: self.status.as_str(),
                to: next.as_str(),
            });
        }
        self.status = next;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn project_id() -> ProjectId {
        ProjectId::new("project-1").unwrap()
    }

    #[test]
    fn project_mode_supplies_opinionated_default_signals() {
        let mut project = Project::new(project_id(), "Learn Japanese").unwrap();
        project.set_mode(ProjectMode::Learning, true);
        assert!(
            project
                .progress_signals()
                .contains(&ProjectProgressSignal::LearningEvidence)
        );
        assert!(
            project
                .progress_signals()
                .contains(&ProjectProgressSignal::HabitConsistency)
        );
    }

    #[test]
    fn required_criteria_gate_completion() {
        let mut project = Project::new(project_id(), "Orbit").unwrap();
        project.transition_to(ProjectStatus::Active).unwrap();
        project
            .replace_success_criteria(vec![
                ProjectSuccessCriterion::new("tests", "Smoke tests pass", true).unwrap(),
            ])
            .unwrap();
        assert_eq!(
            project.transition_to(ProjectStatus::Completed),
            Err(DomainError::ProjectCompletionCriteriaUnsatisfied)
        );
        project
            .set_success_criterion_status("tests", SuccessCriterionStatus::Satisfied)
            .unwrap();
        project.transition_to(ProjectStatus::Completed).unwrap();
    }

    #[test]
    fn statuses_parse_round_trip_and_expose_their_edges() {
        assert_eq!(ProjectStatus::ALL.len(), 6);
        for status in ProjectStatus::ALL {
            assert_eq!(ProjectStatus::parse(status.as_str()), Some(status));
            assert_eq!(status.as_str(), status.as_str());
        }
        assert_eq!(ProjectStatus::parse("Proposed"), None);
        assert_eq!(ProjectStatus::parse("backlog"), None);
        // A status never offers itself as an edge, even though the domain allows
        // a same-state transition as a no-op.
        let offered: Vec<&str> = ProjectStatus::ALL
            .into_iter()
            .filter(|next| *next != ProjectStatus::Proposed)
            .filter(|next| ProjectStatus::Proposed.can_transition_to(*next))
            .map(ProjectStatus::as_str)
            .collect();
        assert_eq!(offered, vec!["active", "cancelled", "archived"]);
    }

    #[test]
    fn rename_rejects_blank_names_and_keeps_the_status() {
        let mut project = Project::new(project_id(), "Orbit").unwrap();
        project.transition_to(ProjectStatus::Active).unwrap();
        assert_eq!(project.rename("   "), Err(DomainError::EmptyProjectName));
        assert_eq!(project.name(), "Orbit");
        project.rename("Orbit Web").unwrap();
        assert_eq!(project.name(), "Orbit Web");
        assert_eq!(project.status(), ProjectStatus::Active);
    }

    #[test]
    fn restore_rehydrates_persisted_state_without_replaying_lifecycle() {
        // `Proposed -> Completed` is not a legal transition, but a row written
        // before this rule (or by a later migration) must still load.
        let project = Project::restore(project_id(), "Archive", ProjectStatus::Completed).unwrap();
        assert_eq!(project.name(), "Archive");
        assert_eq!(project.status(), ProjectStatus::Completed);
        assert_eq!(project.mode(), ProjectMode::Standard);
        assert!(project.success_criteria().is_empty());
        assert_eq!(
            Project::restore(project_id(), "  ", ProjectStatus::Active),
            Err(DomainError::EmptyProjectName)
        );
    }
}
