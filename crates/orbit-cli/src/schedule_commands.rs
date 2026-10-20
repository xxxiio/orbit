//! Capability-specific CLI adapter; delegates mutations to the authoritative host.
use super::*;

pub(super) fn run_schedule_host(
    host: &mut orbit_runtime::AuthoritativeHost,
    command: ScheduleSubcommand,
) -> Result<Response, CliError> {
    match command {
        ScheduleSubcommand::Add(args) => {
            let target = schedule_target(args.task, args.habit)?;
            let flexibility = if args.fixed {
                ScheduleFlexibility::Fixed
            } else {
                ScheduleFlexibility::Movable
            };
            let id = ScheduleBlockId::new(generate_id("schedule")).map_err(CliError::domain)?;
            let block = {
                let revision = host.snapshot().map_err(CliError::repository)?.0;
                let (_, _, _accepted_id) = host
                    .create_schedule_block_accepted(
                        "orbit-cli",
                        &generate_id("command"),
                        revision,
                        CreateScheduleBlockWithActivityCommand {
                            block: CreateScheduleBlockCommand {
                                id: id.clone(),
                                title: args.title,
                                starts_at_ms: args.start_ms,
                                duration_minutes: args.duration_minutes,
                                flexibility,
                                target,
                            },
                            activity_id: ActivityId::new(generate_id("activity"))
                                .map_err(CliError::domain)?,
                            origin: ActivityOrigin::Human,
                            summary: format!("Schedule block {} created", id),
                        },
                    )
                    .map_err(CliError::application)?;
                host.application_queries()
                    .schedule_block(&id)
                    .map_err(CliError::application)?
            };
            Ok(Response::ScheduleBlock {
                block: ScheduleBlockView::from(&block),
                action: "created",
            })
        }
        ScheduleSubcommand::List(args) => {
            if args.to_ms <= args.from_ms {
                return Err(CliError::Usage(
                    "--to-ms must be greater than --from-ms".into(),
                ));
            }
            let mut blocks = host
                .application_queries()
                .schedule_between(args.from_ms, args.to_ms)
                .map_err(CliError::application)?;
            if let Some(query) = normalized_filter(args.query)? {
                blocks.retain(|block| text_matches(&query, block.id().as_str(), block.title()));
            }
            Ok(Response::ScheduleBlocks {
                blocks: blocks.iter().map(ScheduleBlockView::from).collect(),
            })
        }
        ScheduleSubcommand::Reschedule(args) => {
            let id = ScheduleBlockId::new(args.id).map_err(CliError::domain)?;
            let block = {
                let revision = host.snapshot().map_err(CliError::repository)?.0;
                let (_, _, _accepted_id) = host
                    .reschedule_block_accepted(
                        "orbit-cli",
                        &generate_id("command"),
                        revision,
                        RescheduleBlockWithActivityCommand {
                            id: id.clone(),
                            starts_at_ms: args.start_ms,
                            activity_id: ActivityId::new(generate_id("activity"))
                                .map_err(CliError::domain)?,
                            origin: ActivityOrigin::Human,
                            summary: format!("Schedule block {} rescheduled", id),
                        },
                    )
                    .map_err(CliError::application)?;
                host.application_queries()
                    .schedule_block(&id)
                    .map_err(CliError::application)?
            };
            Ok(Response::ScheduleBlock {
                block: ScheduleBlockView::from(&block),
                action: "rescheduled",
            })
        }
    }
}
