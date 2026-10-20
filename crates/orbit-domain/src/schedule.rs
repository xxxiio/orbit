use crate::{DomainError, HabitId, ScheduleBlockId, TaskId};

/// Whether a scheduled block may be moved by a planner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ScheduleFlexibility {
    Fixed,
    Movable,
}

/// The thing a block schedules. A block may also be a standalone event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScheduleTarget {
    Standalone,
    Task(TaskId),
    Habit(HabitId),
}

/// A concrete allocation of time on the user's schedule.
///
/// Time is represented as Unix milliseconds at the domain boundary to keep the
/// core independent from UI/date-time libraries while retaining deterministic
/// ordering and duration semantics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScheduleBlock {
    id: ScheduleBlockId,
    title: String,
    starts_at_ms: i64,
    duration_minutes: u32,
    flexibility: ScheduleFlexibility,
    target: ScheduleTarget,
}

impl ScheduleBlock {
    pub fn new(
        id: ScheduleBlockId,
        title: impl Into<String>,
        starts_at_ms: i64,
        duration_minutes: u32,
        flexibility: ScheduleFlexibility,
        target: ScheduleTarget,
    ) -> Result<Self, DomainError> {
        let title = title.into();
        if title.trim().is_empty() {
            return Err(DomainError::EmptyScheduleBlockTitle);
        }
        if duration_minutes == 0 {
            return Err(DomainError::InvalidScheduleDuration);
        }

        // Reject timestamps whose end would overflow; every downstream
        // query and scheduling policy can then use a valid half-open interval.
        if starts_at_ms
            .checked_add(i64::from(duration_minutes) * 60_000)
            .is_none()
        {
            return Err(DomainError::InvalidScheduleDuration);
        }

        Ok(Self {
            id,
            title,
            starts_at_ms,
            duration_minutes,
            flexibility,
            target,
        })
    }

    pub fn id(&self) -> &ScheduleBlockId {
        &self.id
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn starts_at_ms(&self) -> i64 {
        self.starts_at_ms
    }

    pub fn duration_minutes(&self) -> u32 {
        self.duration_minutes
    }

    pub fn flexibility(&self) -> ScheduleFlexibility {
        self.flexibility
    }

    pub fn target(&self) -> &ScheduleTarget {
        &self.target
    }

    pub fn ends_at_ms(&self) -> i64 {
        self.starts_at_ms + i64::from(self.duration_minutes) * 60_000
    }

    pub fn reschedule(&mut self, starts_at_ms: i64) -> Result<(), DomainError> {
        if self.flexibility == ScheduleFlexibility::Fixed {
            return Err(DomainError::FixedScheduleBlockCannotMove);
        }

        if starts_at_ms
            .checked_add(i64::from(self.duration_minutes) * 60_000)
            .is_none()
        {
            return Err(DomainError::InvalidScheduleDuration);
        }
        self.starts_at_ms = starts_at_ms;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block_id() -> ScheduleBlockId {
        ScheduleBlockId::new("block-1").expect("valid schedule block id")
    }

    #[test]
    fn standalone_schedule_blocks_are_first_class() {
        let block = ScheduleBlock::new(
            block_id(),
            "Lunch",
            1_000_000,
            60,
            ScheduleFlexibility::Fixed,
            ScheduleTarget::Standalone,
        )
        .expect("valid schedule block");

        assert_eq!(block.target(), &ScheduleTarget::Standalone);
        assert_eq!(block.ends_at_ms(), 4_600_000);
    }

    #[test]
    fn task_and_habit_blocks_reference_entities_by_id() {
        let task_id = TaskId::new("task-1").expect("valid task id");
        let habit_id = HabitId::new("habit-1").expect("valid habit id");

        let task_block = ScheduleBlock::new(
            block_id(),
            "Deep work",
            0,
            90,
            ScheduleFlexibility::Movable,
            ScheduleTarget::Task(task_id.clone()),
        )
        .expect("valid task block");
        let habit_block = ScheduleBlock::new(
            ScheduleBlockId::new("block-2").expect("valid block id"),
            "Run",
            0,
            45,
            ScheduleFlexibility::Movable,
            ScheduleTarget::Habit(habit_id.clone()),
        )
        .expect("valid habit block");

        assert_eq!(task_block.target(), &ScheduleTarget::Task(task_id));
        assert_eq!(habit_block.target(), &ScheduleTarget::Habit(habit_id));
    }

    #[test]
    fn schedule_block_requires_title_and_positive_duration() {
        assert_eq!(
            ScheduleBlock::new(
                block_id(),
                "   ",
                0,
                30,
                ScheduleFlexibility::Movable,
                ScheduleTarget::Standalone,
            ),
            Err(DomainError::EmptyScheduleBlockTitle)
        );

        assert_eq!(
            ScheduleBlock::new(
                block_id(),
                "Focus",
                0,
                0,
                ScheduleFlexibility::Movable,
                ScheduleTarget::Standalone,
            ),
            Err(DomainError::InvalidScheduleDuration)
        );
    }

    #[test]
    fn movable_blocks_can_be_rescheduled_but_fixed_blocks_cannot() {
        let mut movable = ScheduleBlock::new(
            block_id(),
            "Study",
            100,
            30,
            ScheduleFlexibility::Movable,
            ScheduleTarget::Standalone,
        )
        .expect("valid movable block");
        movable.reschedule(200).expect("movable block may move");
        assert_eq!(movable.starts_at_ms(), 200);

        let mut fixed = ScheduleBlock::new(
            ScheduleBlockId::new("block-2").expect("valid block id"),
            "Appointment",
            100,
            30,
            ScheduleFlexibility::Fixed,
            ScheduleTarget::Standalone,
        )
        .expect("valid fixed block");

        assert_eq!(
            fixed.reschedule(200),
            Err(DomainError::FixedScheduleBlockCannotMove)
        );
        assert_eq!(fixed.starts_at_ms(), 100);
    }
}

#[cfg(test)]
mod interval_regression_tests {
    use super::*;
    #[test]
    fn overflow_is_rejected_on_create_and_move_without_mutating_block() {
        let id = ScheduleBlockId::new("overflow").unwrap();
        assert_eq!(
            ScheduleBlock::new(
                id.clone(),
                "Invalid",
                i64::MAX,
                60,
                ScheduleFlexibility::Movable,
                ScheduleTarget::Standalone
            )
            .unwrap_err(),
            DomainError::InvalidScheduleDuration
        );
        let mut block = ScheduleBlock::new(
            id,
            "Valid",
            0,
            60,
            ScheduleFlexibility::Movable,
            ScheduleTarget::Standalone,
        )
        .unwrap();
        assert_eq!(
            block.reschedule(i64::MAX),
            Err(DomainError::InvalidScheduleDuration)
        );
        assert_eq!(block.starts_at_ms(), 0);
    }
}
