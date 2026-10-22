use crate::DomainError;
use std::fmt;

macro_rules! typed_id {
    ($name:ident, $kind:literal) => {
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Result<Self, DomainError> {
                let value = value.into();
                if value.trim().is_empty() {
                    return Err(DomainError::EmptyIdentifier { kind: $kind });
                }
                Ok(Self(value))
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }

            pub fn into_inner(self) -> String {
                self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }
    };
}

typed_id!(TaskId, "task");
typed_id!(ProjectId, "project");
typed_id!(MilestoneId, "milestone");
typed_id!(HabitId, "habit");
typed_id!(HabitOccurrenceId, "habit_occurrence");
typed_id!(RoutineId, "routine");
typed_id!(ScheduleBlockId, "schedule_block");
typed_id!(ActivityId, "activity");
typed_id!(KnowledgeItemId, "knowledge_item");
typed_id!(KnowledgeResourceId, "knowledge_resource");
typed_id!(AcceptanceCriterionId, "acceptance_criterion");
typed_id!(AttachmentId, "attachment");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_ids_are_distinct_and_preserve_their_value() {
        let task = TaskId::new("task-123").expect("valid task id");
        let project = ProjectId::new("project-123").expect("valid project id");

        assert_eq!(task.as_str(), "task-123");
        assert_eq!(project.as_str(), "project-123");
    }

    #[test]
    fn typed_ids_reject_blank_values() {
        assert_eq!(
            TaskId::new("   "),
            Err(DomainError::EmptyIdentifier { kind: "task" })
        );
    }
}
