//! Deterministic planning and capacity primitives for Orbit Core.
//!
//! This crate deliberately contains no AI behavior. It turns an explicit time
//! window plus already-scheduled work into deterministic capacity and free-slot
//! calculations that later proposal engines can build on.

use orbit_domain::{HabitCadence, ScheduleBlock, ScheduleFlexibility};

const MINUTE_MS: i64 = 60_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlannerError {
    InvalidWindow,
    InvalidPolicy,
    InvalidAvailability,
    InvalidWeek,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlanningWindow {
    start_ms: i64,
    end_ms: i64,
}

impl PlanningWindow {
    pub fn new(start_ms: i64, end_ms: i64) -> Result<Self, PlannerError> {
        if end_ms <= start_ms {
            return Err(PlannerError::InvalidWindow);
        }
        Ok(Self { start_ms, end_ms })
    }

    pub fn start_ms(self) -> i64 {
        self.start_ms
    }

    pub fn end_ms(self) -> i64 {
        self.end_ms
    }

    pub fn duration_minutes(self) -> u32 {
        minutes_between(self.start_ms, self.end_ms)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Weekday {
    Monday,
    Tuesday,
    Wednesday,
    Thursday,
    Friday,
    Saturday,
    Sunday,
}

impl Weekday {
    fn index(self) -> usize {
        match self {
            Self::Monday => 0,
            Self::Tuesday => 1,
            Self::Wednesday => 2,
            Self::Thursday => 3,
            Self::Friday => 4,
            Self::Saturday => 5,
            Self::Sunday => 6,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WeekdaySet(u8);

impl WeekdaySet {
    pub fn from_days(days: &[Weekday]) -> Result<Self, PlannerError> {
        if days.is_empty() {
            return Err(PlannerError::InvalidAvailability);
        }
        let mut mask = 0u8;
        for day in days {
            mask |= 1u8 << day.index();
        }
        Ok(Self(mask))
    }

    pub fn weekdays() -> Self {
        Self(0b0001_1111)
    }

    pub fn all() -> Self {
        Self(0b0111_1111)
    }

    pub fn contains(self, day: Weekday) -> bool {
        self.0 & (1u8 << day.index()) != 0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DailyAvailability {
    pub start_minute: u16,
    pub end_minute: u16,
}

impl DailyAvailability {
    pub fn new(start_minute: u16, end_minute: u16) -> Result<Self, PlannerError> {
        if start_minute >= end_minute || end_minute > 24 * 60 {
            return Err(PlannerError::InvalidAvailability);
        }
        Ok(Self {
            start_minute,
            end_minute,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WeeklyAvailabilityRule {
    pub days: WeekdaySet,
    pub window: DailyAvailability,
}

impl WeeklyAvailabilityRule {
    pub fn new(days: WeekdaySet, window: DailyAvailability) -> Self {
        Self { days, window }
    }
}

/// Seven caller-resolved local day boundaries, Monday through Sunday.
///
/// Orbit intentionally does not perform timezone/DST conversion in the pure
/// planner. A platform adapter resolves local day boundaries and supplies them
/// here; day lengths may therefore vary around DST transitions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlanningWeek {
    days: [PlanningWindow; 7],
}

impl PlanningWeek {
    pub fn new(days: [PlanningWindow; 7]) -> Result<Self, PlannerError> {
        for pair in days.windows(2) {
            if pair[0].end_ms() != pair[1].start_ms() {
                return Err(PlannerError::InvalidWeek);
            }
        }
        Ok(Self { days })
    }

    pub fn day(self, day: Weekday) -> PlanningWindow {
        self.days[day.index()]
    }

    pub fn window(self) -> PlanningWindow {
        PlanningWindow {
            start_ms: self.days[0].start_ms(),
            end_ms: self.days[6].end_ms(),
        }
    }
}

/// Expands weekly availability rules against explicit local day boundaries.
/// Offsets are minutes from each supplied day boundary. Rules that do not fit
/// inside a caller-resolved short day fail closed rather than spilling into the
/// following day.
pub fn expand_weekly_availability(
    week: PlanningWeek,
    rules: &[WeeklyAvailabilityRule],
) -> Result<Vec<AvailabilitySlot>, PlannerError> {
    let weekdays = [
        Weekday::Monday,
        Weekday::Tuesday,
        Weekday::Wednesday,
        Weekday::Thursday,
        Weekday::Friday,
        Weekday::Saturday,
        Weekday::Sunday,
    ];
    let mut slots = Vec::new();
    for day in weekdays {
        let day_window = week.day(day);
        for rule in rules.iter().filter(|rule| rule.days.contains(day)) {
            let start = day_window
                .start_ms()
                .checked_add(i64::from(rule.window.start_minute) * MINUTE_MS)
                .ok_or(PlannerError::InvalidAvailability)?;
            let end = day_window
                .start_ms()
                .checked_add(i64::from(rule.window.end_minute) * MINUTE_MS)
                .ok_or(PlannerError::InvalidAvailability)?;
            if start < day_window.start_ms() || end > day_window.end_ms() || end <= start {
                return Err(PlannerError::InvalidAvailability);
            }
            slots.push(AvailabilitySlot {
                start_ms: start,
                end_ms: end,
            });
        }
    }
    Ok(normalize_availability(week.window(), &slots))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FixedBlockBufferPolicy {
    pub before_minutes: u32,
    pub after_minutes: u32,
}

impl FixedBlockBufferPolicy {
    pub fn new(before_minutes: u32, after_minutes: u32) -> Self {
        Self {
            before_minutes,
            after_minutes,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HabitPlanningNeed {
    pub target_completions: u32,
    pub completed_completions: u32,
    pub remaining_completions: u32,
    pub duration_per_occurrence_minutes: u32,
    pub remaining_minutes: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HabitCapacityAssessment {
    pub need: HabitPlanningNeed,
    pub plannable_minutes: u32,
    pub fits_capacity: bool,
}

pub fn habit_planning_need(
    cadence: HabitCadence,
    completed_in_period: u32,
    duration_per_occurrence_minutes: u32,
) -> Result<HabitPlanningNeed, PlannerError> {
    if duration_per_occurrence_minutes == 0 {
        return Err(PlannerError::InvalidPolicy);
    }
    let target = match cadence {
        HabitCadence::Daily => 1_u32,
        HabitCadence::Weekly { target_per_week } => u32::from(target_per_week),
    };
    let remaining = target.saturating_sub(completed_in_period);
    Ok(HabitPlanningNeed {
        target_completions: target,
        completed_completions: completed_in_period,
        remaining_completions: remaining,
        duration_per_occurrence_minutes,
        remaining_minutes: remaining.saturating_mul(duration_per_occurrence_minutes),
    })
}

pub fn assess_habit_capacity(
    need: HabitPlanningNeed,
    capacity: &CapacityReport,
) -> HabitCapacityAssessment {
    HabitCapacityAssessment {
        need,
        plannable_minutes: capacity.plannable_minutes,
        fits_capacity: need.remaining_minutes <= capacity.plannable_minutes,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CapacityPolicy {
    /// Time intentionally kept unscheduled even when the calendar is empty.
    pub protected_minutes: u32,
    /// Optional upper bound on newly planned work in this window.
    pub max_new_work_minutes: Option<u32>,
}

impl CapacityPolicy {
    pub fn new(
        protected_minutes: u32,
        max_new_work_minutes: Option<u32>,
    ) -> Result<Self, PlannerError> {
        if matches!(max_new_work_minutes, Some(0)) {
            return Err(PlannerError::InvalidPolicy);
        }
        Ok(Self {
            protected_minutes,
            max_new_work_minutes,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AvailabilitySlot {
    pub start_ms: i64,
    pub end_ms: i64,
}

impl AvailabilitySlot {
    pub fn duration_minutes(self) -> u32 {
        minutes_between(self.start_ms, self.end_ms)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapacityReport {
    pub window: PlanningWindow,
    pub total_minutes: u32,
    pub occupied_minutes: u32,
    pub fixed_minutes: u32,
    pub movable_minutes: u32,
    pub free_minutes: u32,
    pub protected_minutes: u32,
    pub plannable_minutes: u32,
    pub free_slots: Vec<AvailabilitySlot>,
}

/// Computes deterministic capacity inside `window`.
///
/// Occupied time is the union of every overlapping schedule block so overlapping
/// blocks do not artificially consume more than 100% of the window. Fixed and
/// movable totals are reported independently for observability and may overlap.
/// Protected time is subtracted only from otherwise-free capacity and is never
/// silently represented as a fake schedule block.
pub fn assess_capacity(
    window: PlanningWindow,
    blocks: &[ScheduleBlock],
    policy: CapacityPolicy,
) -> CapacityReport {
    let occupied = merged_intervals(window, blocks.iter());
    let fixed = merged_intervals(
        window,
        blocks
            .iter()
            .filter(|block| block.flexibility() == ScheduleFlexibility::Fixed),
    );
    let movable = merged_intervals(
        window,
        blocks
            .iter()
            .filter(|block| block.flexibility() == ScheduleFlexibility::Movable),
    );
    let free_slots = invert_intervals(window, &occupied);

    let total_minutes = window.duration_minutes();
    let occupied_minutes = intervals_minutes(&occupied);
    let fixed_minutes = intervals_minutes(&fixed);
    let movable_minutes = intervals_minutes(&movable);
    let free_minutes = total_minutes.saturating_sub(occupied_minutes);
    let protected_minutes = policy.protected_minutes.min(free_minutes);
    let available_after_protection = free_minutes.saturating_sub(protected_minutes);
    let plannable_minutes = match policy.max_new_work_minutes {
        Some(limit) => available_after_protection.min(limit),
        None => available_after_protection,
    };

    CapacityReport {
        window,
        total_minutes,
        occupied_minutes,
        fixed_minutes,
        movable_minutes,
        free_minutes,
        protected_minutes,
        plannable_minutes,
        free_slots,
    }
}

/// Returns free slots at least `minimum_minutes` long. A zero minimum simply
/// returns every non-empty free slot.
pub fn available_slots(
    window: PlanningWindow,
    blocks: &[ScheduleBlock],
    minimum_minutes: u32,
) -> Vec<AvailabilitySlot> {
    let occupied = merged_intervals(window, blocks.iter());
    invert_intervals(window, &occupied)
        .into_iter()
        .filter(|slot| slot.duration_minutes() >= minimum_minutes)
        .collect()
}

fn merged_intervals_with_fixed_buffers(
    window: PlanningWindow,
    blocks: &[ScheduleBlock],
    buffers: FixedBlockBufferPolicy,
) -> Vec<(i64, i64)> {
    let before_ms = i64::from(buffers.before_minutes) * MINUTE_MS;
    let after_ms = i64::from(buffers.after_minutes) * MINUTE_MS;
    let mut intervals: Vec<_> = blocks
        .iter()
        .filter_map(|block| {
            let (raw_start, raw_end) = if block.flexibility() == ScheduleFlexibility::Fixed {
                (
                    block.starts_at_ms().saturating_sub(before_ms),
                    block.ends_at_ms().saturating_add(after_ms),
                )
            } else {
                (block.starts_at_ms(), block.ends_at_ms())
            };
            let start = raw_start.max(window.start_ms());
            let end = raw_end.min(window.end_ms());
            (end > start).then_some((start, end))
        })
        .collect();
    intervals.sort_unstable_by_key(|(start, end)| (*start, *end));
    let mut merged: Vec<(i64, i64)> = Vec::new();
    for (start, end) in intervals {
        if let Some((_, previous_end)) = merged.last_mut()
            && start <= *previous_end
        {
            *previous_end = (*previous_end).max(end);
            continue;
        }
        merged.push((start, end));
    }
    merged
}

fn merged_intervals<'a>(
    window: PlanningWindow,
    blocks: impl Iterator<Item = &'a ScheduleBlock>,
) -> Vec<(i64, i64)> {
    let mut intervals: Vec<_> = blocks
        .filter_map(|block| {
            let start = block.starts_at_ms().max(window.start_ms);
            let end = block.ends_at_ms().min(window.end_ms);
            (end > start).then_some((start, end))
        })
        .collect();
    intervals.sort_unstable_by_key(|(start, end)| (*start, *end));

    let mut merged: Vec<(i64, i64)> = Vec::new();
    for (start, end) in intervals {
        if let Some((_, previous_end)) = merged.last_mut()
            && start <= *previous_end
        {
            *previous_end = (*previous_end).max(end);
            continue;
        }
        merged.push((start, end));
    }
    merged
}

fn invert_intervals(window: PlanningWindow, occupied: &[(i64, i64)]) -> Vec<AvailabilitySlot> {
    let mut cursor = window.start_ms;
    let mut slots = Vec::new();
    for &(start, end) in occupied {
        if start > cursor {
            slots.push(AvailabilitySlot {
                start_ms: cursor,
                end_ms: start,
            });
        }
        cursor = cursor.max(end);
    }
    if cursor < window.end_ms {
        slots.push(AvailabilitySlot {
            start_ms: cursor,
            end_ms: window.end_ms,
        });
    }
    slots
}

fn intervals_minutes(intervals: &[(i64, i64)]) -> u32 {
    intervals
        .iter()
        .map(|(start, end)| minutes_between(*start, *end))
        .sum()
}

fn minutes_between(start_ms: i64, end_ms: i64) -> u32 {
    if end_ms <= start_ms {
        return 0;
    }
    ((end_ms - start_ms) / MINUTE_MS).min(u32::MAX as i64) as u32
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnscheduledReason {
    NotEligible,
    MissingEstimate,
    CapacityExhausted,
    NoFittingSlot,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProposedTaskBlock {
    pub task_id: orbit_domain::TaskId,
    pub start_ms: i64,
    pub end_ms: i64,
}

impl ProposedTaskBlock {
    pub fn duration_minutes(&self) -> u32 {
        minutes_between(self.start_ms, self.end_ms)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnscheduledTask {
    pub task_id: orbit_domain::TaskId,
    pub reason: UnscheduledReason,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PlanProposal {
    pub scheduled: Vec<ProposedTaskBlock>,
    pub unscheduled: Vec<UnscheduledTask>,
}

/// Builds a deterministic, non-mutating proposal for new task blocks.
///
/// Tasks are never split in this initial policy. Eligible work is ordered by
/// earliest deadline, then workflow urgency, earliest-start constraint, and
/// finally stable task id. Capacity protection is enforced as a total budget;
/// protected time is not materialized as a fake schedule block.
pub fn propose_tasks(
    window: PlanningWindow,
    blocks: &[ScheduleBlock],
    tasks: &[orbit_domain::Task],
    policy: CapacityPolicy,
) -> PlanProposal {
    let availability = [AvailabilitySlot {
        start_ms: window.start_ms(),
        end_ms: window.end_ms(),
    }];
    propose_tasks_with_availability(window, blocks, tasks, policy, &availability)
}

/// Builds a deterministic proposal restricted to explicit availability slots.
/// Capacity protection applies to the free time inside those slots, not to all
/// empty calendar time.
pub fn propose_tasks_with_availability(
    window: PlanningWindow,
    blocks: &[ScheduleBlock],
    tasks: &[orbit_domain::Task],
    policy: CapacityPolicy,
    availability: &[AvailabilitySlot],
) -> PlanProposal {
    propose_tasks_with_availability_and_buffers(
        window,
        blocks,
        tasks,
        policy,
        availability,
        FixedBlockBufferPolicy::default(),
    )
}

/// Availability-aware proposal generation with prep/decompression buffers around
/// fixed commitments. Buffers do not create synthetic schedule blocks and do not
/// force gaps between movable task blocks.
pub fn propose_tasks_with_availability_and_buffers(
    window: PlanningWindow,
    blocks: &[ScheduleBlock],
    tasks: &[orbit_domain::Task],
    policy: CapacityPolicy,
    availability: &[AvailabilitySlot],
    fixed_buffers: FixedBlockBufferPolicy,
) -> PlanProposal {
    let allowed = normalize_availability(window, availability);
    let occupied = merged_intervals_with_fixed_buffers(window, blocks, fixed_buffers);
    let mut slots = subtract_intervals(&allowed, &occupied);
    let free_minutes: u32 = slots.iter().map(|slot| slot.duration_minutes()).sum();
    let protected = policy.protected_minutes.min(free_minutes);
    let after_protection = free_minutes.saturating_sub(protected);
    let remaining_budget = match policy.max_new_work_minutes {
        Some(limit) => after_protection.min(limit),
        None => after_protection,
    };
    propose_tasks_from_slots(tasks, &mut slots, remaining_budget)
}

/// Day adapter over the generic planner. The caller supplies the resolved local
/// day window plus the availability intervals that are valid within it.
pub fn propose_day(
    day: PlanningWindow,
    blocks: &[ScheduleBlock],
    tasks: &[orbit_domain::Task],
    policy: CapacityPolicy,
    availability: &[AvailabilitySlot],
) -> PlanProposal {
    propose_tasks_with_availability(day, blocks, tasks, policy, availability)
}

/// Week adapter that expands recurring weekly availability and plans across the
/// resulting horizon. Capacity policy is horizon-level in this initial version;
/// per-day capacity policy can be layered on later without changing recurrence.
pub fn propose_week(
    week: PlanningWeek,
    blocks: &[ScheduleBlock],
    tasks: &[orbit_domain::Task],
    policy: CapacityPolicy,
    rules: &[WeeklyAvailabilityRule],
) -> Result<PlanProposal, PlannerError> {
    let availability = expand_weekly_availability(week, rules)?;
    Ok(propose_tasks_with_availability(
        week.window(),
        blocks,
        tasks,
        policy,
        &availability,
    ))
}

fn propose_tasks_from_slots(
    tasks: &[orbit_domain::Task],
    slots: &mut Vec<AvailabilitySlot>,
    mut remaining_budget: u32,
) -> PlanProposal {
    let mut proposal = PlanProposal::default();

    let mut ordered: Vec<&orbit_domain::Task> = tasks.iter().collect();
    ordered.sort_by_key(|task| {
        let planning = task.planning();
        (
            planning.deadline_at_ms().unwrap_or(i64::MAX),
            status_rank(task.status()),
            planning.earliest_start_at_ms().unwrap_or(i64::MIN),
            task.id().as_str().to_owned(),
        )
    });

    for task in ordered {
        if !is_plannable_status(task.status()) {
            proposal.unscheduled.push(UnscheduledTask {
                task_id: task.id().clone(),
                reason: UnscheduledReason::NotEligible,
            });
            continue;
        }

        let planning = task.planning();
        let Some(estimate) = planning.estimated_minutes() else {
            proposal.unscheduled.push(UnscheduledTask {
                task_id: task.id().clone(),
                reason: UnscheduledReason::MissingEstimate,
            });
            continue;
        };

        if estimate > remaining_budget {
            proposal.unscheduled.push(UnscheduledTask {
                task_id: task.id().clone(),
                reason: UnscheduledReason::CapacityExhausted,
            });
            continue;
        }

        let duration_ms = i64::from(estimate) * MINUTE_MS;
        let mut chosen = None;
        for (index, slot) in slots.iter().copied().enumerate() {
            let start = slot
                .start_ms
                .max(planning.earliest_start_at_ms().unwrap_or(i64::MIN));
            let Some(end) = start.checked_add(duration_ms) else {
                continue;
            };
            if end > slot.end_ms {
                continue;
            }
            if planning
                .deadline_at_ms()
                .is_some_and(|deadline| end > deadline)
            {
                continue;
            }
            chosen = Some((index, start, end));
            break;
        }

        let Some((slot_index, start, end)) = chosen else {
            proposal.unscheduled.push(UnscheduledTask {
                task_id: task.id().clone(),
                reason: UnscheduledReason::NoFittingSlot,
            });
            continue;
        };

        let original = slots.remove(slot_index);
        if original.start_ms < start {
            slots.push(AvailabilitySlot {
                start_ms: original.start_ms,
                end_ms: start,
            });
        }
        if end < original.end_ms {
            slots.push(AvailabilitySlot {
                start_ms: end,
                end_ms: original.end_ms,
            });
        }
        slots.sort_unstable_by_key(|slot| (slot.start_ms, slot.end_ms));
        remaining_budget = remaining_budget.saturating_sub(estimate);
        proposal.scheduled.push(ProposedTaskBlock {
            task_id: task.id().clone(),
            start_ms: start,
            end_ms: end,
        });
    }

    proposal
}

fn normalize_availability(
    window: PlanningWindow,
    availability: &[AvailabilitySlot],
) -> Vec<AvailabilitySlot> {
    let mut intervals: Vec<(i64, i64)> = availability
        .iter()
        .filter_map(|slot| {
            let start = slot.start_ms.max(window.start_ms());
            let end = slot.end_ms.min(window.end_ms());
            (end > start).then_some((start, end))
        })
        .collect();
    intervals.sort_unstable_by_key(|(start, end)| (*start, *end));
    let mut merged: Vec<(i64, i64)> = Vec::new();
    for (start, end) in intervals {
        if let Some((_, previous_end)) = merged.last_mut()
            && start <= *previous_end
        {
            *previous_end = (*previous_end).max(end);
            continue;
        }
        merged.push((start, end));
    }
    merged
        .into_iter()
        .map(|(start_ms, end_ms)| AvailabilitySlot { start_ms, end_ms })
        .collect()
}

fn subtract_intervals(
    availability: &[AvailabilitySlot],
    occupied: &[(i64, i64)],
) -> Vec<AvailabilitySlot> {
    let mut result = Vec::new();
    for slot in availability {
        let mut fragments = vec![(slot.start_ms, slot.end_ms)];
        for &(occupied_start, occupied_end) in occupied {
            let mut next = Vec::new();
            for (start, end) in fragments {
                if !intervals_overlap(start, end, occupied_start, occupied_end) {
                    next.push((start, end));
                    continue;
                }
                if start < occupied_start {
                    next.push((start, occupied_start));
                }
                if occupied_end < end {
                    next.push((occupied_end, end));
                }
            }
            fragments = next;
        }
        result.extend(
            fragments
                .into_iter()
                .filter(|(start, end)| end > start)
                .map(|(start_ms, end_ms)| AvailabilitySlot { start_ms, end_ms }),
        );
    }
    result.sort_unstable_by_key(|slot| (slot.start_ms, slot.end_ms));
    result
}

fn is_plannable_status(status: orbit_domain::TaskStatus) -> bool {
    matches!(
        status,
        orbit_domain::TaskStatus::InProgress
            | orbit_domain::TaskStatus::Ready
            | orbit_domain::TaskStatus::Planned
            | orbit_domain::TaskStatus::Inbox
    )
}

fn status_rank(status: orbit_domain::TaskStatus) -> u8 {
    match status {
        orbit_domain::TaskStatus::InProgress => 0,
        orbit_domain::TaskStatus::Ready => 1,
        orbit_domain::TaskStatus::Planned => 2,
        orbit_domain::TaskStatus::Inbox => 3,
        _ => 4,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanViolation {
    UnknownTask(orbit_domain::TaskId),
    DuplicateTask(orbit_domain::TaskId),
    TaskNotEligible(orbit_domain::TaskId),
    MissingEstimate(orbit_domain::TaskId),
    DurationMismatch(orbit_domain::TaskId),
    OutsidePlanningWindow(orbit_domain::TaskId),
    BeforeEarliestStart(orbit_domain::TaskId),
    AfterDeadline(orbit_domain::TaskId),
    OverlapsExistingSchedule(orbit_domain::TaskId),
    ViolatesFixedBuffer(orbit_domain::TaskId),
    OverlapsProposedTask(orbit_domain::TaskId),
    CapacityExceeded,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedPlanProposal(PlanProposal);

impl ValidatedPlanProposal {
    pub fn proposal(&self) -> &PlanProposal {
        &self.0
    }

    pub fn into_inner(self) -> PlanProposal {
        self.0
    }
}

/// Validates a proposal independently from proposal generation. This is the
/// trust boundary used by future AI/external planners: no proposal is safe to
/// apply merely because it was produced by a trusted-looking caller.
pub fn validate_proposal(
    window: PlanningWindow,
    blocks: &[ScheduleBlock],
    tasks: &[orbit_domain::Task],
    policy: CapacityPolicy,
    proposal: PlanProposal,
) -> Result<ValidatedPlanProposal, Vec<PlanViolation>> {
    validate_proposal_with_buffers(
        window,
        blocks,
        tasks,
        policy,
        FixedBlockBufferPolicy::default(),
        proposal,
    )
}

pub fn validate_proposal_with_buffers(
    window: PlanningWindow,
    blocks: &[ScheduleBlock],
    tasks: &[orbit_domain::Task],
    policy: CapacityPolicy,
    fixed_buffers: FixedBlockBufferPolicy,
    proposal: PlanProposal,
) -> Result<ValidatedPlanProposal, Vec<PlanViolation>> {
    use std::collections::{BTreeMap, BTreeSet};

    let task_map: BTreeMap<_, _> = tasks.iter().map(|task| (task.id().clone(), task)).collect();
    let mut seen = BTreeSet::new();
    let mut violations = Vec::new();
    let mut proposed_intervals: Vec<(i64, i64)> = Vec::new();
    let mut planned_minutes = 0u32;

    for item in &proposal.scheduled {
        let task_id = item.task_id.clone();
        if !seen.insert(task_id.clone()) {
            violations.push(PlanViolation::DuplicateTask(task_id.clone()));
        }
        let Some(task) = task_map.get(&task_id).copied() else {
            violations.push(PlanViolation::UnknownTask(task_id));
            continue;
        };

        if !is_plannable_status(task.status()) {
            violations.push(PlanViolation::TaskNotEligible(task_id.clone()));
        }
        let planning = task.planning();
        let Some(estimate) = planning.estimated_minutes() else {
            violations.push(PlanViolation::MissingEstimate(task_id.clone()));
            continue;
        };
        if item.end_ms <= item.start_ms || item.duration_minutes() != estimate {
            violations.push(PlanViolation::DurationMismatch(task_id.clone()));
        }
        if item.start_ms < window.start_ms || item.end_ms > window.end_ms {
            violations.push(PlanViolation::OutsidePlanningWindow(task_id.clone()));
        }
        if planning
            .earliest_start_at_ms()
            .is_some_and(|earliest| item.start_ms < earliest)
        {
            violations.push(PlanViolation::BeforeEarliestStart(task_id.clone()));
        }
        if planning
            .deadline_at_ms()
            .is_some_and(|deadline| item.end_ms > deadline)
        {
            violations.push(PlanViolation::AfterDeadline(task_id.clone()));
        }
        if blocks.iter().any(|block| {
            intervals_overlap(
                item.start_ms,
                item.end_ms,
                block.starts_at_ms(),
                block.ends_at_ms(),
            )
        }) {
            violations.push(PlanViolation::OverlapsExistingSchedule(task_id.clone()));
        }
        let before_ms = i64::from(fixed_buffers.before_minutes) * MINUTE_MS;
        let after_ms = i64::from(fixed_buffers.after_minutes) * MINUTE_MS;
        if blocks.iter().any(|block| {
            block.flexibility() == ScheduleFlexibility::Fixed
                && !intervals_overlap(
                    item.start_ms,
                    item.end_ms,
                    block.starts_at_ms(),
                    block.ends_at_ms(),
                )
                && intervals_overlap(
                    item.start_ms,
                    item.end_ms,
                    block.starts_at_ms().saturating_sub(before_ms),
                    block.ends_at_ms().saturating_add(after_ms),
                )
        }) {
            violations.push(PlanViolation::ViolatesFixedBuffer(task_id.clone()));
        }
        if proposed_intervals
            .iter()
            .any(|&(start, end)| intervals_overlap(item.start_ms, item.end_ms, start, end))
        {
            violations.push(PlanViolation::OverlapsProposedTask(task_id.clone()));
        }
        proposed_intervals.push((item.start_ms, item.end_ms));
        planned_minutes = planned_minutes.saturating_add(estimate);
    }

    let full_availability = [AvailabilitySlot {
        start_ms: window.start_ms(),
        end_ms: window.end_ms(),
    }];
    let allowed = normalize_availability(window, &full_availability);
    let occupied = merged_intervals_with_fixed_buffers(window, blocks, fixed_buffers);
    let free = subtract_intervals(&allowed, &occupied);
    let free_minutes: u32 = free.iter().map(|slot| slot.duration_minutes()).sum();
    let protected = policy.protected_minutes.min(free_minutes);
    let after_protection = free_minutes.saturating_sub(protected);
    let plannable = match policy.max_new_work_minutes {
        Some(limit) => after_protection.min(limit),
        None => after_protection,
    };
    if planned_minutes > plannable {
        violations.push(PlanViolation::CapacityExceeded);
    }

    if violations.is_empty() {
        Ok(ValidatedPlanProposal(proposal))
    } else {
        Err(violations)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HabitPlanRequest {
    pub habit_id: orbit_domain::HabitId,
    pub cadence: HabitCadence,
    pub active: bool,
    pub completed_in_period: u32,
    pub duration_per_occurrence_minutes: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProposedHabitBlock {
    pub habit_id: orbit_domain::HabitId,
    pub occurrence_index: u32,
    pub start_ms: i64,
    pub end_ms: i64,
}

impl ProposedHabitBlock {
    pub fn duration_minutes(&self) -> u32 {
        minutes_between(self.start_ms, self.end_ms)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnscheduledHabitReason {
    Inactive,
    NoRemainingCompletions,
    CapacityExhausted,
    NoFittingSlot,
    InvalidDuration,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnscheduledHabit {
    pub habit_id: orbit_domain::HabitId,
    pub remaining_occurrences: u32,
    pub reason: UnscheduledHabitReason,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HabitPlanProposal {
    pub scheduled: Vec<ProposedHabitBlock>,
    pub unscheduled: Vec<UnscheduledHabit>,
}

/// Schedules remaining habit occurrences directly as habit blocks. Habits do
/// not become tasks; each proposal item remains keyed by `HabitId`.
pub fn propose_habits_with_availability_and_buffers(
    window: PlanningWindow,
    blocks: &[ScheduleBlock],
    requests: &[HabitPlanRequest],
    policy: CapacityPolicy,
    availability: &[AvailabilitySlot],
    fixed_buffers: FixedBlockBufferPolicy,
) -> HabitPlanProposal {
    let allowed = normalize_availability(window, availability);
    let occupied = merged_intervals_with_fixed_buffers(window, blocks, fixed_buffers);
    let mut slots = subtract_intervals(&allowed, &occupied);
    let free_minutes: u32 = slots.iter().map(|slot| slot.duration_minutes()).sum();
    let protected = policy.protected_minutes.min(free_minutes);
    let after_protection = free_minutes.saturating_sub(protected);
    let mut remaining_budget = policy
        .max_new_work_minutes
        .map_or(after_protection, |limit| after_protection.min(limit));
    let mut proposal = HabitPlanProposal::default();
    let mut ordered = requests.to_vec();
    ordered.sort_by(|left, right| left.habit_id.as_str().cmp(right.habit_id.as_str()));

    for request in ordered {
        if !request.active {
            proposal.unscheduled.push(UnscheduledHabit {
                habit_id: request.habit_id,
                remaining_occurrences: 0,
                reason: UnscheduledHabitReason::Inactive,
            });
            continue;
        }
        if request.duration_per_occurrence_minutes == 0 {
            proposal.unscheduled.push(UnscheduledHabit {
                habit_id: request.habit_id,
                remaining_occurrences: 0,
                reason: UnscheduledHabitReason::InvalidDuration,
            });
            continue;
        }
        let need = match habit_planning_need(
            request.cadence,
            request.completed_in_period,
            request.duration_per_occurrence_minutes,
        ) {
            Ok(need) => need,
            Err(_) => {
                proposal.unscheduled.push(UnscheduledHabit {
                    habit_id: request.habit_id,
                    remaining_occurrences: 0,
                    reason: UnscheduledHabitReason::InvalidDuration,
                });
                continue;
            }
        };
        if need.remaining_completions == 0 {
            proposal.unscheduled.push(UnscheduledHabit {
                habit_id: request.habit_id,
                remaining_occurrences: 0,
                reason: UnscheduledHabitReason::NoRemainingCompletions,
            });
            continue;
        }

        let mut scheduled_count = 0u32;
        for occurrence_index in 0..need.remaining_completions {
            let duration = request.duration_per_occurrence_minutes;
            if duration > remaining_budget {
                break;
            }
            let duration_ms = i64::from(duration) * MINUTE_MS;
            let chosen = slots.iter().copied().enumerate().find_map(|(index, slot)| {
                let end = slot.start_ms.checked_add(duration_ms)?;
                (end <= slot.end_ms).then_some((index, slot.start_ms, end))
            });
            let Some((slot_index, start, end)) = chosen else {
                break;
            };
            let original = slots.remove(slot_index);
            if end < original.end_ms {
                slots.push(AvailabilitySlot {
                    start_ms: end,
                    end_ms: original.end_ms,
                });
                slots.sort_unstable_by_key(|slot| (slot.start_ms, slot.end_ms));
            }
            remaining_budget = remaining_budget.saturating_sub(duration);
            proposal.scheduled.push(ProposedHabitBlock {
                habit_id: request.habit_id.clone(),
                occurrence_index,
                start_ms: start,
                end_ms: end,
            });
            scheduled_count += 1;
        }
        if scheduled_count < need.remaining_completions {
            let remaining = need.remaining_completions - scheduled_count;
            let reason = if remaining_budget < request.duration_per_occurrence_minutes {
                UnscheduledHabitReason::CapacityExhausted
            } else {
                UnscheduledHabitReason::NoFittingSlot
            };
            proposal.unscheduled.push(UnscheduledHabit {
                habit_id: request.habit_id,
                remaining_occurrences: remaining,
                reason,
            });
        }
    }
    proposal
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HabitPlanViolation {
    UnknownHabit(orbit_domain::HabitId),
    InactiveHabit(orbit_domain::HabitId),
    TooManyOccurrences(orbit_domain::HabitId),
    DurationMismatch(orbit_domain::HabitId),
    OutsidePlanningWindow(orbit_domain::HabitId),
    OverlapsExistingSchedule(orbit_domain::HabitId),
    ViolatesFixedBuffer(orbit_domain::HabitId),
    OverlapsProposedBlock(orbit_domain::HabitId),
    CapacityExceeded,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedHabitPlanProposal(HabitPlanProposal);

impl ValidatedHabitPlanProposal {
    pub fn proposal(&self) -> &HabitPlanProposal {
        &self.0
    }
    pub fn into_inner(self) -> HabitPlanProposal {
        self.0
    }
}

pub fn validate_habit_proposal_with_buffers(
    window: PlanningWindow,
    blocks: &[ScheduleBlock],
    requests: &[HabitPlanRequest],
    policy: CapacityPolicy,
    fixed_buffers: FixedBlockBufferPolicy,
    proposal: HabitPlanProposal,
) -> Result<ValidatedHabitPlanProposal, Vec<HabitPlanViolation>> {
    use std::collections::{BTreeMap, BTreeSet};
    let request_map: BTreeMap<_, _> = requests
        .iter()
        .map(|request| (request.habit_id.clone(), request))
        .collect();
    let mut per_habit_count: BTreeMap<orbit_domain::HabitId, u32> = BTreeMap::new();
    let mut seen_items = BTreeSet::new();
    let mut proposed_intervals = Vec::new();
    let mut planned_minutes = 0u32;
    let mut violations = Vec::new();

    for item in &proposal.scheduled {
        let Some(request) = request_map.get(&item.habit_id).copied() else {
            violations.push(HabitPlanViolation::UnknownHabit(item.habit_id.clone()));
            continue;
        };
        if !request.active {
            violations.push(HabitPlanViolation::InactiveHabit(item.habit_id.clone()));
        }
        if !seen_items.insert((item.habit_id.clone(), item.occurrence_index)) {
            violations.push(HabitPlanViolation::TooManyOccurrences(
                item.habit_id.clone(),
            ));
        }
        let count = per_habit_count.entry(item.habit_id.clone()).or_insert(0);
        *count += 1;
        let allowed_count = habit_planning_need(
            request.cadence,
            request.completed_in_period,
            request.duration_per_occurrence_minutes,
        )
        .map(|need| need.remaining_completions)
        .unwrap_or(0);
        if *count > allowed_count {
            violations.push(HabitPlanViolation::TooManyOccurrences(
                item.habit_id.clone(),
            ));
        }
        if item.end_ms <= item.start_ms
            || item.duration_minutes() != request.duration_per_occurrence_minutes
        {
            violations.push(HabitPlanViolation::DurationMismatch(item.habit_id.clone()));
        }
        if item.start_ms < window.start_ms() || item.end_ms > window.end_ms() {
            violations.push(HabitPlanViolation::OutsidePlanningWindow(
                item.habit_id.clone(),
            ));
        }
        if blocks.iter().any(|block| {
            intervals_overlap(
                item.start_ms,
                item.end_ms,
                block.starts_at_ms(),
                block.ends_at_ms(),
            )
        }) {
            violations.push(HabitPlanViolation::OverlapsExistingSchedule(
                item.habit_id.clone(),
            ));
        }
        let before_ms = i64::from(fixed_buffers.before_minutes) * MINUTE_MS;
        let after_ms = i64::from(fixed_buffers.after_minutes) * MINUTE_MS;
        if blocks.iter().any(|block| {
            block.flexibility() == ScheduleFlexibility::Fixed
                && !intervals_overlap(
                    item.start_ms,
                    item.end_ms,
                    block.starts_at_ms(),
                    block.ends_at_ms(),
                )
                && intervals_overlap(
                    item.start_ms,
                    item.end_ms,
                    block.starts_at_ms().saturating_sub(before_ms),
                    block.ends_at_ms().saturating_add(after_ms),
                )
        }) {
            violations.push(HabitPlanViolation::ViolatesFixedBuffer(
                item.habit_id.clone(),
            ));
        }
        if proposed_intervals
            .iter()
            .any(|&(start, end)| intervals_overlap(item.start_ms, item.end_ms, start, end))
        {
            violations.push(HabitPlanViolation::OverlapsProposedBlock(
                item.habit_id.clone(),
            ));
        }
        proposed_intervals.push((item.start_ms, item.end_ms));
        planned_minutes = planned_minutes.saturating_add(item.duration_minutes());
    }

    let availability = [AvailabilitySlot {
        start_ms: window.start_ms(),
        end_ms: window.end_ms(),
    }];
    let allowed = normalize_availability(window, &availability);
    let occupied = merged_intervals_with_fixed_buffers(window, blocks, fixed_buffers);
    let free = subtract_intervals(&allowed, &occupied);
    let free_minutes: u32 = free.iter().map(|slot| slot.duration_minutes()).sum();
    let after_protection = free_minutes.saturating_sub(policy.protected_minutes.min(free_minutes));
    let plannable = policy
        .max_new_work_minutes
        .map_or(after_protection, |limit| after_protection.min(limit));
    if planned_minutes > plannable {
        violations.push(HabitPlanViolation::CapacityExceeded);
    }

    if violations.is_empty() {
        Ok(ValidatedHabitPlanProposal(proposal))
    } else {
        Err(violations)
    }
}

fn intervals_overlap(left_start: i64, left_end: i64, right_start: i64, right_end: i64) -> bool {
    left_start < right_end && right_start < left_end
}

#[cfg(test)]
mod tests {
    use super::*;
    use orbit_domain::{ScheduleBlockId, ScheduleTarget};

    fn block(
        id: &str,
        start_minute: i64,
        duration_minutes: u32,
        flexibility: ScheduleFlexibility,
    ) -> ScheduleBlock {
        ScheduleBlock::new(
            ScheduleBlockId::new(id).expect("id"),
            id,
            start_minute * MINUTE_MS,
            duration_minutes,
            flexibility,
            ScheduleTarget::Standalone,
        )
        .expect("block")
    }

    #[test]
    fn invalid_windows_and_zero_work_caps_fail_closed() {
        assert_eq!(
            PlanningWindow::new(10, 10),
            Err(PlannerError::InvalidWindow)
        );
        assert_eq!(
            CapacityPolicy::new(0, Some(0)),
            Err(PlannerError::InvalidPolicy)
        );
    }

    #[test]
    fn overlapping_schedule_blocks_are_counted_once_for_capacity() {
        let window = PlanningWindow::new(0, 8 * 60 * MINUTE_MS).expect("window");
        let blocks = vec![
            block("fixed", 60, 120, ScheduleFlexibility::Fixed),
            block("movable", 120, 120, ScheduleFlexibility::Movable),
        ];

        let report = assess_capacity(window, &blocks, CapacityPolicy::default());

        assert_eq!(report.total_minutes, 480);
        assert_eq!(report.occupied_minutes, 180);
        assert_eq!(report.fixed_minutes, 120);
        assert_eq!(report.movable_minutes, 120);
        assert_eq!(report.free_minutes, 300);
        assert_eq!(report.plannable_minutes, 300);
    }

    #[test]
    fn protected_time_and_work_cap_limit_new_planning_without_fake_blocks() {
        let window = PlanningWindow::new(0, 10 * 60 * MINUTE_MS).expect("window");
        let blocks = vec![block("meeting", 60, 60, ScheduleFlexibility::Fixed)];
        let policy = CapacityPolicy::new(120, Some(180)).expect("policy");

        let report = assess_capacity(window, &blocks, policy);

        assert_eq!(report.free_minutes, 540);
        assert_eq!(report.protected_minutes, 120);
        assert_eq!(report.plannable_minutes, 180);
        assert_eq!(report.free_slots.len(), 2);
    }

    #[test]
    fn free_slots_are_clipped_to_window_and_filter_by_minimum_size() {
        let window = PlanningWindow::new(60 * MINUTE_MS, 300 * MINUTE_MS).expect("window");
        let blocks = vec![
            block("before-and-in", 0, 90, ScheduleFlexibility::Fixed),
            block("middle", 120, 30, ScheduleFlexibility::Movable),
            block("late", 240, 120, ScheduleFlexibility::Fixed),
        ];

        let slots = available_slots(window, &blocks, 45);

        assert_eq!(
            slots,
            vec![AvailabilitySlot {
                start_ms: 150 * MINUTE_MS,
                end_ms: 240 * MINUTE_MS,
            }]
        );
    }

    fn task_with_plan(
        id: &str,
        status: orbit_domain::TaskStatus,
        estimate: Option<u32>,
        earliest: Option<i64>,
        deadline: Option<i64>,
    ) -> orbit_domain::Task {
        let mut task = orbit_domain::Task::new(
            orbit_domain::TaskId::new(id).expect("id"),
            id,
            orbit_domain::TaskScope::standalone(),
        )
        .expect("task");
        match status {
            orbit_domain::TaskStatus::Inbox => {}
            orbit_domain::TaskStatus::Planned
            | orbit_domain::TaskStatus::Ready
            | orbit_domain::TaskStatus::Deferred
            | orbit_domain::TaskStatus::Cancelled => {
                task.transition_to(status).expect("status");
            }
            orbit_domain::TaskStatus::InProgress | orbit_domain::TaskStatus::Blocked => {
                task.transition_to(orbit_domain::TaskStatus::Ready)
                    .expect("ready");
                task.transition_to(status).expect("status");
            }
            other => panic!("unsupported test status: {other:?}"),
        }
        task.set_planning(
            orbit_domain::TaskPlanning::new(estimate, earliest, deadline).expect("planning"),
        );
        task
    }

    #[test]
    fn proposal_orders_deadlines_and_respects_earliest_start() {
        let window = PlanningWindow::new(0, 240 * MINUTE_MS).expect("window");
        let tasks = vec![
            task_with_plan(
                "later",
                orbit_domain::TaskStatus::Ready,
                Some(60),
                None,
                Some(240 * MINUTE_MS),
            ),
            task_with_plan(
                "urgent",
                orbit_domain::TaskStatus::Ready,
                Some(30),
                Some(60 * MINUTE_MS),
                Some(120 * MINUTE_MS),
            ),
        ];

        let proposal = propose_tasks(window, &[], &tasks, CapacityPolicy::default());

        assert_eq!(proposal.scheduled.len(), 2);
        assert_eq!(proposal.scheduled[0].task_id.as_str(), "urgent");
        assert_eq!(proposal.scheduled[0].start_ms, 60 * MINUTE_MS);
        assert_eq!(proposal.scheduled[1].task_id.as_str(), "later");
        assert_eq!(proposal.scheduled[1].start_ms, 0);
    }

    #[test]
    fn proposal_does_not_split_tasks_and_reports_reasons() {
        let window = PlanningWindow::new(0, 120 * MINUTE_MS).expect("window");
        let blocks = vec![block("middle", 45, 30, ScheduleFlexibility::Fixed)];
        let tasks = vec![
            task_with_plan(
                "too-big",
                orbit_domain::TaskStatus::Ready,
                Some(60),
                None,
                None,
            ),
            task_with_plan("missing", orbit_domain::TaskStatus::Ready, None, None, None),
            task_with_plan(
                "blocked",
                orbit_domain::TaskStatus::Blocked,
                Some(15),
                None,
                None,
            ),
        ];

        let proposal = propose_tasks(window, &blocks, &tasks, CapacityPolicy::default());

        assert!(proposal.scheduled.is_empty());
        assert_eq!(proposal.unscheduled.len(), 3);
        assert!(
            proposal
                .unscheduled
                .iter()
                .any(|item| item.task_id.as_str() == "too-big"
                    && item.reason == UnscheduledReason::NoFittingSlot)
        );
        assert!(
            proposal
                .unscheduled
                .iter()
                .any(|item| item.task_id.as_str() == "missing"
                    && item.reason == UnscheduledReason::MissingEstimate)
        );
        assert!(
            proposal
                .unscheduled
                .iter()
                .any(|item| item.task_id.as_str() == "blocked"
                    && item.reason == UnscheduledReason::NotEligible)
        );
    }

    #[test]
    fn proposal_respects_protected_capacity_budget() {
        let window = PlanningWindow::new(0, 180 * MINUTE_MS).expect("window");
        let tasks = vec![
            task_with_plan("a", orbit_domain::TaskStatus::Ready, Some(60), None, None),
            task_with_plan("b", orbit_domain::TaskStatus::Ready, Some(60), None, None),
            task_with_plan("c", orbit_domain::TaskStatus::Ready, Some(60), None, None),
        ];
        let policy = CapacityPolicy::new(60, None).expect("policy");

        let proposal = propose_tasks(window, &[], &tasks, policy);

        assert_eq!(proposal.scheduled.len(), 2);
        assert_eq!(proposal.unscheduled.len(), 1);
        assert_eq!(
            proposal.unscheduled[0].reason,
            UnscheduledReason::CapacityExhausted
        );
    }

    #[test]
    fn generated_proposal_passes_independent_validation() {
        let window = PlanningWindow::new(0, 180 * MINUTE_MS).expect("window");
        let tasks = vec![
            task_with_plan("a", orbit_domain::TaskStatus::Ready, Some(30), None, None),
            task_with_plan("b", orbit_domain::TaskStatus::Ready, Some(45), None, None),
        ];
        let proposal = propose_tasks(window, &[], &tasks, CapacityPolicy::default());
        let validated = validate_proposal(
            window,
            &[],
            &tasks,
            CapacityPolicy::default(),
            proposal.clone(),
        )
        .expect("generated proposal should validate");
        assert_eq!(validated.proposal(), &proposal);
    }

    #[test]
    fn validator_rejects_external_proposal_that_breaks_constraints() {
        let window = PlanningWindow::new(0, 120 * MINUTE_MS).expect("window");
        let tasks = vec![task_with_plan(
            "a",
            orbit_domain::TaskStatus::Ready,
            Some(30),
            Some(30 * MINUTE_MS),
            Some(90 * MINUTE_MS),
        )];
        let blocks = vec![block("meeting", 60, 30, ScheduleFlexibility::Fixed)];
        let proposal = PlanProposal {
            scheduled: vec![ProposedTaskBlock {
                task_id: tasks[0].id().clone(),
                start_ms: 15 * MINUTE_MS,
                end_ms: 75 * MINUTE_MS,
            }],
            unscheduled: Vec::new(),
        };

        let violations =
            validate_proposal(window, &blocks, &tasks, CapacityPolicy::default(), proposal)
                .expect_err("invalid proposal must fail");

        assert!(
            violations
                .iter()
                .any(|v| matches!(v, PlanViolation::DurationMismatch(_)))
        );
        assert!(
            violations
                .iter()
                .any(|v| matches!(v, PlanViolation::BeforeEarliestStart(_)))
        );
        assert!(
            violations
                .iter()
                .any(|v| matches!(v, PlanViolation::OverlapsExistingSchedule(_)))
        );
    }

    #[test]
    fn weekly_availability_expands_against_explicit_day_boundaries() {
        let day = 24 * 60 * MINUTE_MS;
        let days = std::array::from_fn(|index| {
            PlanningWindow::new(index as i64 * day, (index as i64 + 1) * day).expect("day")
        });
        let week = PlanningWeek::new(days).expect("week");
        let rule = WeeklyAvailabilityRule::new(
            WeekdaySet::weekdays(),
            DailyAvailability::new(9 * 60, 17 * 60).expect("availability"),
        );

        let slots = expand_weekly_availability(week, &[rule]).expect("expand");

        assert_eq!(slots.len(), 5);
        assert_eq!(slots[0].start_ms, 9 * 60 * MINUTE_MS);
        assert_eq!(slots[4].end_ms, 4 * day + 17 * 60 * MINUTE_MS);
    }

    #[test]
    fn week_requires_contiguous_caller_resolved_days() {
        let day = 24 * 60 * MINUTE_MS;
        let mut days = std::array::from_fn(|index| {
            PlanningWindow::new(index as i64 * day, (index as i64 + 1) * day).expect("day")
        });
        days[3] = PlanningWindow::new(4 * day, 5 * day).expect("shifted");
        assert_eq!(PlanningWeek::new(days), Err(PlannerError::InvalidWeek));
    }

    #[test]
    fn availability_restricts_planning_even_when_calendar_is_empty() {
        let window = PlanningWindow::new(0, 8 * 60 * MINUTE_MS).expect("window");
        let tasks = vec![task_with_plan(
            "a",
            orbit_domain::TaskStatus::Ready,
            Some(90),
            None,
            None,
        )];
        let availability = vec![AvailabilitySlot {
            start_ms: 120 * MINUTE_MS,
            end_ms: 180 * MINUTE_MS,
        }];

        let proposal = propose_day(
            window,
            &[],
            &tasks,
            CapacityPolicy::default(),
            &availability,
        );

        assert!(proposal.scheduled.is_empty());
        assert_eq!(
            proposal.unscheduled[0].reason,
            UnscheduledReason::CapacityExhausted
        );
    }

    #[test]
    fn fixed_commitment_buffers_reserve_time_without_fake_schedule_blocks() {
        let window = PlanningWindow::new(0, 240 * MINUTE_MS).expect("window");
        let blocks = vec![block("meeting", 120, 30, ScheduleFlexibility::Fixed)];
        let tasks = vec![task_with_plan(
            "a",
            orbit_domain::TaskStatus::Ready,
            Some(60),
            Some(75 * MINUTE_MS),
            Some(165 * MINUTE_MS),
        )];
        let availability = [AvailabilitySlot {
            start_ms: 0,
            end_ms: 240 * MINUTE_MS,
        }];
        let proposal = propose_tasks_with_availability_and_buffers(
            window,
            &blocks,
            &tasks,
            CapacityPolicy::default(),
            &availability,
            FixedBlockBufferPolicy::new(30, 30),
        );

        assert!(proposal.scheduled.is_empty());
        assert_eq!(
            proposal.unscheduled[0].reason,
            UnscheduledReason::NoFittingSlot
        );
    }

    #[test]
    fn validator_rejects_task_inside_fixed_commitment_buffer() {
        let window = PlanningWindow::new(0, 240 * MINUTE_MS).expect("window");
        let blocks = vec![block("meeting", 120, 30, ScheduleFlexibility::Fixed)];
        let tasks = vec![task_with_plan(
            "a",
            orbit_domain::TaskStatus::Ready,
            Some(30),
            None,
            None,
        )];
        let proposal = PlanProposal {
            scheduled: vec![ProposedTaskBlock {
                task_id: tasks[0].id().clone(),
                start_ms: 90 * MINUTE_MS,
                end_ms: 120 * MINUTE_MS,
            }],
            unscheduled: Vec::new(),
        };

        let violations = validate_proposal_with_buffers(
            window,
            &blocks,
            &tasks,
            CapacityPolicy::default(),
            FixedBlockBufferPolicy::new(30, 0),
            proposal,
        )
        .expect_err("buffer violation");

        assert!(
            violations
                .iter()
                .any(|violation| matches!(violation, PlanViolation::ViolatesFixedBuffer(_)))
        );
    }
}

#[cfg(test)]
mod habit_planning_tests {
    use super::*;

    #[test]
    fn weekly_habit_need_tracks_remaining_completions_without_creating_tasks() {
        let need = habit_planning_need(HabitCadence::Weekly { target_per_week: 4 }, 2, 30).unwrap();
        assert_eq!(need.remaining_completions, 2);
        assert_eq!(need.remaining_minutes, 60);
    }

    #[test]
    fn completed_habit_period_requires_no_additional_capacity() {
        let need = habit_planning_need(HabitCadence::Daily, 1, 20).unwrap();
        assert_eq!(need.remaining_completions, 0);
        assert_eq!(need.remaining_minutes, 0);
    }
}

#[cfg(test)]
mod habit_schedule_proposal_tests {
    use super::*;
    use orbit_domain::HabitId;

    fn request(id: &str, completed: u32) -> HabitPlanRequest {
        HabitPlanRequest {
            habit_id: HabitId::new(id).unwrap(),
            cadence: HabitCadence::Weekly { target_per_week: 3 },
            active: true,
            completed_in_period: completed,
            duration_per_occurrence_minutes: 20,
        }
    }

    #[test]
    fn habits_are_scheduled_directly_without_task_conversion() {
        let window = PlanningWindow::new(0, 120 * MINUTE_MS).unwrap();
        let availability = [AvailabilitySlot {
            start_ms: 0,
            end_ms: 120 * MINUTE_MS,
        }];
        let proposal = propose_habits_with_availability_and_buffers(
            window,
            &[],
            &[request("habit-1", 1)],
            CapacityPolicy::default(),
            &availability,
            FixedBlockBufferPolicy::default(),
        );
        assert_eq!(proposal.scheduled.len(), 2);
        assert_eq!(proposal.scheduled[0].habit_id.as_str(), "habit-1");
    }

    #[test]
    fn external_habit_proposals_cannot_exceed_remaining_cadence() {
        let window = PlanningWindow::new(0, 120 * MINUTE_MS).unwrap();
        let habit = HabitId::new("habit-1").unwrap();
        let proposal = HabitPlanProposal {
            scheduled: vec![
                ProposedHabitBlock {
                    habit_id: habit.clone(),
                    occurrence_index: 0,
                    start_ms: 0,
                    end_ms: 20 * MINUTE_MS,
                },
                ProposedHabitBlock {
                    habit_id: habit.clone(),
                    occurrence_index: 1,
                    start_ms: 20 * MINUTE_MS,
                    end_ms: 40 * MINUTE_MS,
                },
            ],
            unscheduled: vec![],
        };
        let result = validate_habit_proposal_with_buffers(
            window,
            &[],
            &[request("habit-1", 2)],
            CapacityPolicy::default(),
            FixedBlockBufferPolicy::default(),
            proposal,
        );
        assert!(result.is_err());
    }
    #[test]
    fn proposal_validity_changes_when_new_completion_reduces_remaining_demand() {
        let window = PlanningWindow::new(0, 120 * MINUTE_MS).unwrap();
        let habit = HabitId::new("habit-1").unwrap();
        let proposal = HabitPlanProposal {
            scheduled: vec![
                ProposedHabitBlock {
                    habit_id: habit.clone(),
                    occurrence_index: 0,
                    start_ms: 0,
                    end_ms: 20 * MINUTE_MS,
                },
                ProposedHabitBlock {
                    habit_id: habit.clone(),
                    occurrence_index: 1,
                    start_ms: 20 * MINUTE_MS,
                    end_ms: 40 * MINUTE_MS,
                },
            ],
            unscheduled: vec![],
        };
        let initially_valid = validate_habit_proposal_with_buffers(
            window,
            &[],
            &[request("habit-1", 1)],
            CapacityPolicy::default(),
            FixedBlockBufferPolicy::default(),
            proposal.clone(),
        );
        assert!(initially_valid.is_ok());
        let stale = validate_habit_proposal_with_buffers(
            window,
            &[],
            &[request("habit-1", 2)],
            CapacityPolicy::default(),
            FixedBlockBufferPolicy::default(),
            proposal,
        );
        assert!(stale.is_err());
    }
}
