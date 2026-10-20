//! Capability-specific CLI adapter; delegates mutations to the authoritative host.
use super::*;

pub(super) fn run_habit_host(
    host: &mut orbit_runtime::AuthoritativeHost,
    command: HabitSubcommand,
) -> Result<Response, CliError> {
    match command {
        HabitSubcommand::Add(args) => {
            let cadence = match args.weekly_target {
                Some(target_per_week) => HabitCadence::Weekly { target_per_week },
                None => HabitCadence::Daily,
            };
            let id = HabitId::new(generate_id("habit")).map_err(CliError::domain)?;
            let expected_revision = host.snapshot().map_err(CliError::repository)?.0;
            let (_, _, habit) = host
                .create_habit_accepted(
                    "orbit-cli",
                    &generate_id("command"),
                    expected_revision,
                    CreateHabitWithActivityCommand {
                        habit: CreateHabitCommand {
                            id: id.clone(),
                            name: args.name,
                            cadence,
                        },
                        activity_id: ActivityId::new(generate_id("activity"))
                            .map_err(CliError::domain)?,
                        origin: ActivityOrigin::Human,
                        summary: format!("Habit {} created", id),
                    },
                )
                .map_err(CliError::application)?;
            Ok(Response::Habit {
                habit: HabitView::from(&habit),
                action: "created",
            })
        }
        HabitSubcommand::List(args) => {
            let mut habits = host
                .application_queries()
                .habits()
                .map_err(CliError::application)?;
            if args.active {
                habits.retain(Habit::is_active);
            } else if args.paused {
                habits.retain(|habit| !habit.is_active());
            }
            if let Some(query) = normalized_filter(args.query)? {
                habits.retain(|habit| text_matches(&query, habit.id().as_str(), habit.name()));
            }
            habits.sort_by(|left, right| left.id().as_str().cmp(right.id().as_str()));
            Ok(Response::Habits {
                habits: habits.iter().map(HabitView::from).collect(),
            })
        }
        HabitSubcommand::Pause(args) => set_habit_active_host(host, args.id, false, "paused"),
        HabitSubcommand::Resume(args) => set_habit_active_host(host, args.id, true, "resumed"),
        HabitSubcommand::Check(args) => {
            let habit_id = HabitId::new(args.id).map_err(CliError::domain)?;
            let occurrence = {
                let revision = host.snapshot().map_err(CliError::repository)?.0;
                let (_, _, accepted_id) = host
                    .record_habit_occurrence_accepted(
                        "orbit-cli",
                        &generate_id("command"),
                        revision,
                        RecordHabitOccurrenceWithActivityCommand {
                            id: HabitOccurrenceId::new(
                                args.occurrence_id
                                    .unwrap_or_else(|| generate_id("habit-occurrence")),
                            )
                            .map_err(CliError::domain)?,
                            habit_id: habit_id.clone(),
                            occurred_at_ms: args.at_ms,
                            duration_minutes: args.duration_minutes,
                            activity_id: ActivityId::new(generate_id("activity"))
                                .map_err(CliError::domain)?,
                            origin: ActivityOrigin::Human,
                            summary: format!("Habit {} completed", habit_id),
                        },
                    )
                    .map_err(CliError::application)?;
                host.application_queries()
                    .habit_occurrences(&habit_id)
                    .map_err(CliError::application)?
                    .into_iter()
                    .find(|o| o.id().as_str() == accepted_id.as_str())
                    .ok_or_else(|| {
                        CliError::application(orbit_application::ApplicationError::NotFound {
                            entity: "habit_occurrence",
                            id: accepted_id.to_owned(),
                        })
                    })?
            };
            Ok(Response::HabitOccurrence {
                occurrence: HabitOccurrenceView::from(&occurrence),
                action: "recorded",
            })
        }
        HabitSubcommand::History(args) => {
            let habit_id = HabitId::new(args.id).map_err(CliError::domain)?;
            let occurrences = host
                .application_queries()
                .habit_occurrences(&habit_id)
                .map_err(CliError::application)?;
            Ok(Response::HabitOccurrences {
                habit_id: habit_id.to_string(),
                occurrences: occurrences.iter().map(HabitOccurrenceView::from).collect(),
            })
        }
    }
}
fn set_habit_active_host(
    host: &mut orbit_runtime::AuthoritativeHost,
    raw_id: String,
    active: bool,
    action: &'static str,
) -> Result<Response, CliError> {
    let id = HabitId::new(raw_id).map_err(CliError::domain)?;
    let habit = {
        let revision = host.snapshot().map_err(CliError::repository)?.0;
        let (_, _, _accepted_id) = host
            .set_habit_active_accepted(
                "orbit-cli",
                &generate_id("command"),
                revision,
                SetHabitActiveWithActivityCommand {
                    id: id.clone(),
                    active,
                    activity_id: ActivityId::new(generate_id("activity"))
                        .map_err(CliError::domain)?,
                    origin: ActivityOrigin::Human,
                    summary: format!("Habit {} {}", id, action),
                },
            )
            .map_err(CliError::application)?;
        host.application_queries()
            .habits()
            .map_err(CliError::application)?
            .into_iter()
            .find(|h| h.id() == &id)
            .ok_or_else(|| {
                CliError::application(orbit_application::ApplicationError::NotFound {
                    entity: "habit",
                    id: id.to_string(),
                })
            })?
    };
    Ok(Response::Habit {
        habit: HabitView::from(&habit),
        action,
    })
}

pub(super) fn run_routine_host(
    host: &mut orbit_runtime::AuthoritativeHost,
    command: RoutineSubcommand,
) -> Result<Response, CliError> {
    match command {
        RoutineSubcommand::Add(args) => {
            let id = RoutineId::new(generate_id("routine")).map_err(CliError::domain)?;
            let expected_revision = host.snapshot().map_err(CliError::repository)?.0;
            let (_, _, routine) = host
                .create_routine_accepted(
                    "orbit-cli",
                    &generate_id("command"),
                    expected_revision,
                    CreateRoutineWithActivityCommand {
                        id: id.clone(),
                        name: args.name,
                        activity_id: ActivityId::new(generate_id("activity"))
                            .map_err(CliError::domain)?,
                        origin: ActivityOrigin::Human,
                        summary: format!("Routine {} created", id),
                    },
                )
                .map_err(CliError::application)?;
            Ok(Response::Routine {
                routine: RoutineView::from(&routine),
                action: "created",
            })
        }
        RoutineSubcommand::List => {
            let mut routines = host
                .application_queries()
                .routines()
                .map_err(CliError::application)?;
            routines.sort_by(|left, right| left.id().as_str().cmp(right.id().as_str()));
            Ok(Response::Routines {
                routines: routines.iter().map(RoutineView::from).collect(),
            })
        }
        RoutineSubcommand::AddHabit(args) => {
            set_routine_habit_host(host, args, true, "added habit to")
        }
        RoutineSubcommand::RemoveHabit(args) => {
            set_routine_habit_host(host, args, false, "removed habit from")
        }
    }
}

fn set_routine_habit_host(
    host: &mut orbit_runtime::AuthoritativeHost,
    args: RoutineHabitArgs,
    present: bool,
    action: &'static str,
) -> Result<Response, CliError> {
    let routine_id = RoutineId::new(args.id).map_err(CliError::domain)?;
    let habit_id = HabitId::new(args.habit_id).map_err(CliError::domain)?;
    let routine = {
        let revision = host.snapshot().map_err(CliError::repository)?.0;
        let (_, _, _accepted_id) = host
            .set_routine_habit_membership_accepted(
                "orbit-cli",
                &generate_id("command"),
                revision,
                SetRoutineHabitMembershipWithActivityCommand {
                    routine_id: routine_id.clone(),
                    habit_id: habit_id.clone(),
                    present,
                    activity_id: ActivityId::new(generate_id("activity"))
                        .map_err(CliError::domain)?,
                    origin: ActivityOrigin::Human,
                    summary: format!(
                        "Routine {} membership changed for habit {}",
                        routine_id, habit_id
                    ),
                },
            )
            .map_err(CliError::application)?;
        host.application_queries()
            .routine(&routine_id)
            .map_err(CliError::application)?
    };
    Ok(Response::Routine {
        routine: RoutineView::from(&routine),
        action,
    })
}
