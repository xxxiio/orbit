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
}
