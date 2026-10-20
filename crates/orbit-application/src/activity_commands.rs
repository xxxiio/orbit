use orbit_domain::{ActivityId, ActivityKind, ActivityOrigin, ActivityRecord, ActivitySubject};

use crate::{ActivityRepository, ApplicationError, Clock};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordActivityCommand {
    pub id: ActivityId,
    pub subject: ActivitySubject,
    pub kind: ActivityKind,
    pub origin: ActivityOrigin,
    pub summary: String,
}

pub fn record_activity(
    repository: &mut impl ActivityRepository,
    clock: &impl Clock,
    command: RecordActivityCommand,
) -> Result<ActivityRecord, ApplicationError> {
    if repository.get(&command.id)?.is_some() {
        return Err(ApplicationError::AlreadyExists {
            entity: "activity",
            id: command.id.to_string(),
        });
    }

    let activity = ActivityRecord::new(
        command.id,
        command.subject,
        command.kind,
        command.origin,
        clock.now_ms(),
        command.summary,
    )?;

    repository.append(activity.clone())?;
    Ok(activity)
}
