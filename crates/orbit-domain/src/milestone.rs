use crate::{DomainError, MilestoneId, ProjectId};

/// Built-in milestone lifecycle states.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MilestoneStatus {
    Planned,
    InProgress,
    Blocked,
    Completed,
    Cancelled,
}

impl MilestoneStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Planned => "planned",
            Self::InProgress => "in_progress",
            Self::Blocked => "blocked",
            Self::Completed => "completed",
            Self::Cancelled => "cancelled",
        }
    }

    pub fn can_transition_to(self, next: Self) -> bool {
        if self == next {
            return true;
        }

        match self {
            Self::Planned => matches!(next, Self::InProgress | Self::Blocked | Self::Cancelled),
            Self::InProgress => matches!(next, Self::Blocked | Self::Completed | Self::Cancelled),
            Self::Blocked => matches!(next, Self::InProgress | Self::Cancelled),
            Self::Completed | Self::Cancelled => false,
        }
    }
}

/// A project-scoped checkpoint representing a meaningful intermediate outcome.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Milestone {
    id: MilestoneId,
    project_id: ProjectId,
    name: String,
    status: MilestoneStatus,
}

impl Milestone {
    pub fn new(
        id: MilestoneId,
        project_id: ProjectId,
        name: impl Into<String>,
    ) -> Result<Self, DomainError> {
        let name = name.into();
        if name.trim().is_empty() {
            return Err(DomainError::EmptyMilestoneName);
        }

        Ok(Self {
            id,
            project_id,
            name,
            status: MilestoneStatus::Planned,
        })
    }

    pub fn id(&self) -> &MilestoneId {
        &self.id
    }

    pub fn project_id(&self) -> &ProjectId {
        &self.project_id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn status(&self) -> MilestoneStatus {
        self.status
    }

    pub fn belongs_to(&self, project_id: &ProjectId) -> bool {
        &self.project_id == project_id
    }

    pub fn transition_to(&mut self, next: MilestoneStatus) -> Result<(), DomainError> {
        if !self.status.can_transition_to(next) {
            return Err(DomainError::InvalidMilestoneStatusTransition {
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
        ProjectId::new("project-1").expect("valid project id")
    }

    fn milestone_id() -> MilestoneId {
        MilestoneId::new("milestone-1").expect("valid milestone id")
    }

    #[test]
    fn milestone_always_belongs_to_exactly_one_project() {
        let project = project_id();
        let milestone =
            Milestone::new(milestone_id(), project.clone(), "MVP").expect("valid milestone");

        assert_eq!(milestone.project_id(), &project);
        assert!(milestone.belongs_to(&project));
        assert_eq!(milestone.status(), MilestoneStatus::Planned);
    }

    #[test]
    fn milestone_name_must_not_be_blank() {
        let result = Milestone::new(milestone_id(), project_id(), "   ");

        assert_eq!(result, Err(DomainError::EmptyMilestoneName));
    }

    #[test]
    fn milestone_can_be_blocked_and_resumed() {
        let mut milestone =
            Milestone::new(milestone_id(), project_id(), "MVP").expect("valid milestone");

        milestone
            .transition_to(MilestoneStatus::Blocked)
            .expect("planned -> blocked");
        milestone
            .transition_to(MilestoneStatus::InProgress)
            .expect("blocked -> in progress");
        milestone
            .transition_to(MilestoneStatus::Completed)
            .expect("in progress -> completed");

        assert_eq!(milestone.status(), MilestoneStatus::Completed);
    }

    #[test]
    fn completed_milestone_is_terminal() {
        let mut milestone =
            Milestone::new(milestone_id(), project_id(), "MVP").expect("valid milestone");
        milestone
            .transition_to(MilestoneStatus::InProgress)
            .expect("planned -> in progress");
        milestone
            .transition_to(MilestoneStatus::Completed)
            .expect("in progress -> completed");

        let error = milestone
            .transition_to(MilestoneStatus::InProgress)
            .expect_err("completed milestone must remain terminal");

        assert_eq!(
            error,
            DomainError::InvalidMilestoneStatusTransition {
                from: "completed",
                to: "in_progress",
            }
        );
    }
}
