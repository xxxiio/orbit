use crate::{DomainError, HabitId, MilestoneId, ProjectId, TaskId};

/// A stable reference to an entity that can participate in dependencies or blockers.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum WorkRef {
    Project(ProjectId),
    Milestone(MilestoneId),
    Task(TaskId),
    Habit(HabitId),
}

/// Directed dependency: `dependent` cannot be considered unblocked until
/// `prerequisite` is satisfied by application-level policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dependency {
    dependent: WorkRef,
    prerequisite: WorkRef,
}

impl Dependency {
    pub fn new(dependent: WorkRef, prerequisite: WorkRef) -> Result<Self, DomainError> {
        if dependent == prerequisite {
            return Err(DomainError::SelfDependency);
        }

        Ok(Self {
            dependent,
            prerequisite,
        })
    }

    pub fn dependent(&self) -> &WorkRef {
        &self.dependent
    }

    pub fn prerequisite(&self) -> &WorkRef {
        &self.prerequisite
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BlockerStatus {
    Open,
    Resolved,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Blocker {
    target: WorkRef,
    reason: String,
    status: BlockerStatus,
}

impl Blocker {
    pub fn new(target: WorkRef, reason: impl Into<String>) -> Result<Self, DomainError> {
        let reason = reason.into();
        if reason.trim().is_empty() {
            return Err(DomainError::EmptyBlockerReason);
        }

        Ok(Self {
            target,
            reason,
            status: BlockerStatus::Open,
        })
    }

    pub fn target(&self) -> &WorkRef {
        &self.target
    }

    pub fn reason(&self) -> &str {
        &self.reason
    }

    pub fn status(&self) -> BlockerStatus {
        self.status
    }

    pub fn resolve(&mut self) {
        self.status = BlockerStatus::Resolved;
    }

    pub fn reopen(&mut self) {
        self.status = BlockerStatus::Open;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn task(value: &str) -> WorkRef {
        WorkRef::Task(TaskId::new(value).expect("valid task id"))
    }

    #[test]
    fn dependency_rejects_self_reference() {
        let target = task("task-1");

        assert_eq!(
            Dependency::new(target.clone(), target),
            Err(DomainError::SelfDependency)
        );
    }

    #[test]
    fn dependency_preserves_direction() {
        let dependent = task("task-2");
        let prerequisite = task("task-1");
        let dependency =
            Dependency::new(dependent.clone(), prerequisite.clone()).expect("valid dependency");

        assert_eq!(dependency.dependent(), &dependent);
        assert_eq!(dependency.prerequisite(), &prerequisite);
    }

    #[test]
    fn blocker_requires_reason_and_can_be_resolved() {
        let mut blocker =
            Blocker::new(task("task-1"), "Waiting for source data").expect("valid blocker");

        assert_eq!(blocker.status(), BlockerStatus::Open);
        blocker.resolve();
        assert_eq!(blocker.status(), BlockerStatus::Resolved);
        blocker.reopen();
        assert_eq!(blocker.status(), BlockerStatus::Open);
    }

    #[test]
    fn blank_blocker_reason_is_rejected() {
        assert_eq!(
            Blocker::new(task("task-1"), "   "),
            Err(DomainError::EmptyBlockerReason)
        );
    }
}
