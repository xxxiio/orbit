# Habit execution contract

Orbit habits are not recurring tasks. They have their own cadence, occurrence history, streak semantics,
and planning demand.

## Durable occurrence evidence

Each completion is a `HabitOccurrence` containing a stable id, habit id, occurrence timestamp, and
optional positive duration. Persistence uses the `HabitOccurrenceRepository` port. Recording an
occurrence through the application service commits the occurrence and `ActivityKind::Completed`
semantic activity in one transaction.

Paused habits reject new occurrences. Existing history remains unchanged when a habit is paused.

## Periods and streaks

The pure domain does not infer local midnight or timezone rules. Clients/platform adapters provide
non-overlapping `HabitPeriod` boundaries. Daily cadence requires at least one completion per period;
weekly cadence requires `target_per_week` completions. Evaluation exposes per-period fulfillment,
completion counts, time spent, current streak, longest streak, and totals.

This makes DST and travel behavior explicit instead of embedding an incorrect fixed-24-hour assumption.

## Planning participation

`orbit-planner` derives a `HabitPlanningNeed` from cadence, completions already recorded in the current
period, and a caller-supplied duration per occurrence. The need exposes remaining completions and
remaining minutes, and can be compared with normal Orbit capacity.

This does not create task rows. A UI/planner may propose schedule blocks targeted directly at a habit,
using the existing `ScheduleTarget::Habit` model.

## Routines

A routine is an ordered, lightweight grouping of stable habit references. It is organizational/planning metadata, not a parent task or lifecycle owner. A habit may belong to multiple routines, and removing a habit from a routine does not pause, delete, or otherwise mutate the habit itself.

Routine membership is persisted separately from habit state and changes are semantic-activity journaled. The ordering is intentional so clients may present sequences such as a morning or shutdown routine without encoding that sequence as task dependencies.

## Direct habit scheduling

Habit planning produces a dedicated typed proposal keyed by `HabitId`; it never creates task rows. Each remaining occurrence is proposed as a contiguous movable block and, when applied, becomes `ScheduleTarget::Habit(habit_id)`.

Habit proposals are untrusted until deterministic validation. Apply re-reads the canonical habit state and completion occurrences inside the transaction before scheduling, preventing a stale preview from over-scheduling a cadence after a completion was recorded elsewhere.
