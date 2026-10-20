use std::collections::{BTreeMap, BTreeSet};

use orbit_domain::{
    Habit, HabitCadence, HabitId, HabitOccurrence, HabitOccurrenceId, Milestone, MilestoneId,
    MilestoneStatus, Project, ProjectId, ProjectMode, ProjectProgressSignal, ProjectStatus,
    ProjectSuccessCriterion, Routine, RoutineId, ScheduleBlock, ScheduleBlockId,
    ScheduleFlexibility, ScheduleTarget, SuccessCriterionStatus, Task, TaskId, TaskPlanning,
    TaskScope, TaskStatus,
};

use crate::{
    ApplicationError, HabitOccurrenceRepository, HabitRepository, MetricEvidenceRepository,
    MetricObservation, MetricTargetRecord, MilestoneRepository, ProjectEvidenceBinding,
    ProjectEvidenceRepository, ProjectRepository, RoutineRepository, ScheduleRepository,
    SemanticEntityKind, SyncFieldVersion, SyncOutboxRepository, TaskRepository, UnitOfWork,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaterializedSemanticField {
    pub field: String,
    pub value: Option<String>,
    pub device_id: String,
    pub sequence: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaterializedSemanticEntity {
    pub kind: SemanticEntityKind,
    pub id: String,
    pub fields: Vec<MaterializedSemanticField>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct InboundMaterializationReport {
    pub applied_entities: usize,
    pub deferred_entities: usize,
    pub protected_local_fields: usize,
}

/// Applies transport-independent, conflict-free semantic state into canonical
/// Orbit repositories without emitting activity or outbound sync changes.
///
/// Fields with a locally pending outbox mutation are protected: canonical local
/// state remains visible until that mutation is exported and the sync engine can
/// either merge it or surface a conflict. Field-version bookkeeping is advanced
/// only for fields actually materialized.
pub fn materialize_inbound_semantic_state(
    unit_of_work: &mut impl UnitOfWork,
    entities: Vec<MaterializedSemanticEntity>,
) -> Result<InboundMaterializationReport, ApplicationError> {
    unit_of_work.transact(|repositories| {
        let pending = SyncOutboxRepository::list_pending_semantic_changes(repositories)?;
        let protected: BTreeSet<(SemanticEntityKind, String, String)> = pending
            .into_iter()
            .flat_map(|reserved| {
                let kind = reserved.change.entity_kind;
                let id = reserved.change.entity_id;
                reserved
                    .change
                    .mutations
                    .into_iter()
                    .map(move |mutation| (kind, id.clone(), mutation.field))
            })
            .collect();

        let mut report = InboundMaterializationReport::default();
        for entity in entities {
            let mut fields = BTreeMap::new();
            let mut versions = Vec::new();
            for field in entity.fields {
                if protected.contains(&(entity.kind, entity.id.clone(), field.field.clone())) {
                    report.protected_local_fields += 1;
                    continue;
                }
                versions.push(SyncFieldVersion {
                    entity_kind: entity.kind,
                    entity_id: entity.id.clone(),
                    field: field.field.clone(),
                    device_id: field.device_id.clone(),
                    sequence: field.sequence,
                });
                fields.insert(field.field, field.value);
            }

            if fields.is_empty() {
                continue;
            }

            let applied = match entity.kind {
                SemanticEntityKind::Project => {
                    materialize_project(repositories, &entity.id, &fields)?
                }
                SemanticEntityKind::Milestone => {
                    materialize_milestone(repositories, &entity.id, &fields)?
                }
                SemanticEntityKind::Task => materialize_task(repositories, &entity.id, &fields)?,
                SemanticEntityKind::Habit => materialize_habit(repositories, &entity.id, &fields)?,
                SemanticEntityKind::HabitOccurrence => {
                    materialize_habit_occurrence(repositories, &entity.id, &fields)?
                }
                SemanticEntityKind::Routine => {
                    materialize_routine(repositories, &entity.id, &fields)?
                }
                SemanticEntityKind::ScheduleBlock => {
                    materialize_schedule(repositories, &entity.id, &fields)?
                }
                SemanticEntityKind::Metric => {
                    materialize_metric(repositories, &entity.id, &fields)?
                }
            };

            if applied {
                for version in versions {
                    SyncOutboxRepository::set_known_sync_field_version(repositories, version)?;
                }
                report.applied_entities += 1;
            } else {
                report.deferred_entities += 1;
            }
        }
        Ok(report)
    })
}

fn materialize_project(
    repositories: &mut impl UnitOfWork,
    id: &str,
    fields: &BTreeMap<String, Option<String>>,
) -> Result<bool, ApplicationError> {
    let id = ProjectId::new(id.to_owned())?;
    let existing = ProjectRepository::get(repositories, &id)?;
    let Some(name) = value_or(
        fields,
        "name",
        existing.as_ref().map(|v| v.name().to_owned()),
    ) else {
        return Ok(false);
    };
    let status = value_or(
        fields,
        "status",
        existing.as_ref().map(|v| v.status().as_str().to_owned()),
    )
    .unwrap_or_else(|| "proposed".into());
    let mode = value_or(
        fields,
        "mode",
        existing.as_ref().map(|v| v.mode().as_str().to_owned()),
    )
    .unwrap_or_else(|| "standard".into());
    let objective = if fields.contains_key("objective") {
        fields.get("objective").cloned().flatten()
    } else {
        existing
            .as_ref()
            .and_then(|v| v.objective().map(ToOwned::to_owned))
    };
    let signals = value_or(
        fields,
        "progress_signals",
        existing.as_ref().map(|v| {
            v.progress_signals()
                .iter()
                .map(|signal| signal.as_str())
                .collect::<Vec<_>>()
                .join(",")
        }),
    )
    .unwrap_or_else(|| "milestones,tasks,blockers,forecast".into());
    let mut project = Project::new(id.clone(), name)?;
    project.set_mode(ProjectMode::parse(&mode)?, false);
    project.set_objective(objective)?;
    project.set_progress_signals(
        signals
            .split(',')
            .filter(|value| !value.is_empty())
            .map(ProjectProgressSignal::parse)
            .collect::<Result<Vec<_>, _>>()?,
    )?;
    let mut criteria = existing
        .as_ref()
        .map(|v| v.success_criteria().to_vec())
        .unwrap_or_default();
    for (field, value) in fields
        .iter()
        .filter(|(field, _)| field.starts_with("criterion:"))
    {
        let key = field.trim_start_matches("criterion:");
        criteria.retain(|criterion| criterion.key() != key);
        if let Some(value) = value {
            criteria.push(decode_project_criterion(key, value)?);
        }
    }
    project.replace_success_criteria(criteria)?;
    restore_project_status(&mut project, &status)?;
    // Avoid partially applying an entity whose referenced habit has not arrived yet.
    for (field, _value) in fields.iter().filter(|(field, value)| {
        field.starts_with("evidence:habit:") && value.as_deref() == Some("1")
    }) {
        let habit_id = HabitId::new(field.trim_start_matches("evidence:habit:").to_owned())?;
        if HabitRepository::get(repositories, &habit_id)?.is_none() {
            return Ok(false);
        }
    }
    ProjectRepository::save(repositories, project)?;
    for (field, value) in fields
        .iter()
        .filter(|(field, _)| field.starts_with("evidence:"))
    {
        let rest = field.strip_prefix("evidence:").unwrap_or("");
        let (kind, evidence_id) =
            rest.split_once(':')
                .ok_or(ApplicationError::InconsistentReference {
                    relation: "project_evidence_field",
                })?;
        if evidence_id.trim().is_empty() || evidence_id.trim() != evidence_id {
            return Err(ApplicationError::InconsistentReference {
                relation: "project_evidence_id",
            });
        }
        let binding = match kind {
            "habit" => ProjectEvidenceBinding::Habit(HabitId::new(evidence_id.to_owned())?),
            "metric" => ProjectEvidenceBinding::Metric(evidence_id.to_owned()),
            _ => {
                return Err(ApplicationError::InconsistentReference {
                    relation: "project_evidence_kind",
                });
            }
        };
        match value.as_deref() {
            Some("1") => {
                if let ProjectEvidenceBinding::Habit(ref habit_id) = binding
                    && HabitRepository::get(repositories, habit_id)?.is_none()
                {
                    return Ok(false);
                }
                ProjectEvidenceRepository::add_project_evidence(
                    repositories,
                    &ProjectId::new(id.as_str())?,
                    &binding,
                )?;
            }
            None => ProjectEvidenceRepository::remove_project_evidence(
                repositories,
                &ProjectId::new(id.as_str())?,
                &binding,
            )?,
            _ => {
                return Err(ApplicationError::InconsistentReference {
                    relation: "project_evidence_value",
                });
            }
        }
    }
    Ok(true)
}

fn decode_project_criterion(
    key: &str,
    encoded: &str,
) -> Result<ProjectSuccessCriterion, ApplicationError> {
    let mut parts = encoded.splitn(3, '|');
    let required = match parts.next() {
        Some("1") => true,
        Some("0") => false,
        _ => {
            return Err(ApplicationError::InconsistentReference {
                relation: "project_criterion_required",
            });
        }
    };
    let status = SuccessCriterionStatus::parse(parts.next().unwrap_or(""))?;
    let description = parts
        .next()
        .unwrap_or("")
        .replace("%0A", "\n")
        .replace("%7C", "|")
        .replace("%25", "%");
    let mut criterion = ProjectSuccessCriterion::new(key.to_owned(), description, required)?;
    criterion.set_status(status);
    Ok(criterion)
}

fn materialize_milestone(
    repositories: &mut impl UnitOfWork,
    id: &str,
    fields: &BTreeMap<String, Option<String>>,
) -> Result<bool, ApplicationError> {
    let id = MilestoneId::new(id.to_owned())?;
    let existing = MilestoneRepository::get(repositories, &id)?;
    let Some(project_id) = value_or(
        fields,
        "project_id",
        existing.as_ref().map(|v| v.project_id().to_string()),
    ) else {
        return Ok(false);
    };
    let project_id = ProjectId::new(project_id)?;
    if ProjectRepository::get(repositories, &project_id)?.is_none() {
        return Ok(false);
    }
    let Some(name) = value_or(
        fields,
        "name",
        existing.as_ref().map(|v| v.name().to_owned()),
    ) else {
        return Ok(false);
    };
    let status = value_or(
        fields,
        "status",
        existing.as_ref().map(|v| v.status().as_str().to_owned()),
    )
    .unwrap_or_else(|| "planned".into());
    let mut milestone = Milestone::new(id, project_id, name)?;
    restore_milestone_status(&mut milestone, &status)?;
    MilestoneRepository::save(repositories, milestone)?;
    Ok(true)
}

fn materialize_task(
    repositories: &mut impl UnitOfWork,
    id: &str,
    fields: &BTreeMap<String, Option<String>>,
) -> Result<bool, ApplicationError> {
    let id = TaskId::new(id.to_owned())?;
    let existing = TaskRepository::get(repositories, &id)?;
    let Some(title) = value_or(
        fields,
        "title",
        existing.as_ref().map(|v| v.title().to_owned()),
    ) else {
        return Ok(false);
    };
    let project_id = optional_id_value(
        fields,
        "project_id",
        existing
            .as_ref()
            .and_then(|v| v.scope().project_id().map(ToString::to_string)),
    )?;
    let milestone_id = optional_id_value(
        fields,
        "milestone_id",
        existing
            .as_ref()
            .and_then(|v| v.scope().milestone_id().map(ToString::to_string)),
    )?;
    let project_id = project_id.map(ProjectId::new).transpose()?;
    let milestone_id = milestone_id.map(MilestoneId::new).transpose()?;
    let scope = TaskScope::try_new(project_id.clone(), milestone_id.clone())?;
    if let Some(project_id) = &project_id
        && ProjectRepository::get(repositories, project_id)?.is_none()
    {
        return Ok(false);
    }
    if let Some(milestone_id) = &milestone_id {
        let Some(milestone) = MilestoneRepository::get(repositories, milestone_id)? else {
            return Ok(false);
        };
        if milestone.project_id() != project_id.as_ref().expect("scope guarantees project") {
            return Err(ApplicationError::InconsistentReference {
                relation: "task_milestone_project",
            });
        }
    }

    let estimated = optional_u32(
        fields,
        "estimated_minutes",
        existing
            .as_ref()
            .and_then(|v| v.planning().estimated_minutes()),
    )?;
    let earliest = optional_i64(
        fields,
        "earliest_start_at_ms",
        existing
            .as_ref()
            .and_then(|v| v.planning().earliest_start_at_ms()),
    )?;
    let deadline = optional_i64(
        fields,
        "deadline_at_ms",
        existing
            .as_ref()
            .and_then(|v| v.planning().deadline_at_ms()),
    )?;
    let planning = TaskPlanning::new(estimated, earliest, deadline)?;
    let status = value_or(
        fields,
        "status",
        existing.as_ref().map(|v| v.status().as_str().to_owned()),
    )
    .unwrap_or_else(|| "inbox".into());
    let mut task = Task::new(id, title, scope)?;
    task.set_planning(planning);
    restore_task_status(&mut task, &status)?;
    TaskRepository::save(repositories, task)?;
    Ok(true)
}

fn materialize_habit(
    repositories: &mut impl UnitOfWork,
    id: &str,
    fields: &BTreeMap<String, Option<String>>,
) -> Result<bool, ApplicationError> {
    let id = HabitId::new(id.to_owned())?;
    let existing = HabitRepository::get(repositories, &id)?;
    let Some(name) = value_or(
        fields,
        "name",
        existing.as_ref().map(|v| v.name().to_owned()),
    ) else {
        return Ok(false);
    };
    let cadence_raw = value_or(
        fields,
        "cadence",
        existing.as_ref().map(|v| match v.cadence() {
            HabitCadence::Daily => "daily".to_owned(),
            HabitCadence::Weekly { target_per_week } => format!("weekly:{target_per_week}"),
        }),
    )
    .unwrap_or_else(|| "daily".into());
    let cadence = parse_cadence(&cadence_raw)?;
    let active_raw = value_or(
        fields,
        "active",
        existing.as_ref().map(|v| v.is_active().to_string()),
    )
    .unwrap_or_else(|| "true".into());
    let active = parse_bool(&active_raw)?;
    let mut habit = Habit::new(id, name, cadence)?;
    if !active {
        habit.pause();
    }
    HabitRepository::save(repositories, habit)?;
    Ok(true)
}

fn materialize_habit_occurrence(
    repositories: &mut impl UnitOfWork,
    id: &str,
    fields: &BTreeMap<String, Option<String>>,
) -> Result<bool, ApplicationError> {
    let id = HabitOccurrenceId::new(id.to_owned())?;
    let existing = HabitOccurrenceRepository::get(repositories, &id)?;
    let Some(habit_id) = value_or(
        fields,
        "habit_id",
        existing.as_ref().map(|v| v.habit_id().to_string()),
    ) else {
        return Ok(false);
    };
    let habit_id = HabitId::new(habit_id)?;
    if HabitRepository::get(repositories, &habit_id)?.is_none() {
        return Ok(false);
    }
    let Some(occurred_at_ms) = required_i64(
        fields,
        "occurred_at_ms",
        existing.as_ref().map(HabitOccurrence::occurred_at_ms),
    )?
    else {
        return Ok(false);
    };
    let duration = optional_u32(
        fields,
        "duration_minutes",
        existing
            .as_ref()
            .and_then(HabitOccurrence::duration_minutes),
    )?;
    let occurrence = HabitOccurrence::new(id, habit_id, occurred_at_ms, duration)?;
    if let Some(existing) = existing {
        if existing == occurrence {
            return Ok(true);
        }
        return Err(ApplicationError::Repository(
            crate::RepositoryError::Conflict,
        ));
    }
    HabitOccurrenceRepository::append(repositories, occurrence)?;
    Ok(true)
}

fn materialize_routine(
    repositories: &mut impl UnitOfWork,
    id: &str,
    fields: &BTreeMap<String, Option<String>>,
) -> Result<bool, ApplicationError> {
    let id = RoutineId::new(id.to_owned())?;
    let existing = RoutineRepository::get(repositories, &id)?;
    let Some(name) = value_or(
        fields,
        "name",
        existing.as_ref().map(|v| v.name().to_owned()),
    ) else {
        return Ok(false);
    };
    let mut membership: BTreeMap<String, usize> = existing
        .as_ref()
        .map(|routine| {
            routine
                .habit_ids()
                .iter()
                .enumerate()
                .map(|(position, id)| (id.to_string(), position))
                .collect()
        })
        .unwrap_or_default();
    for (field, value) in fields {
        let Some(habit_id) = field.strip_prefix("habit/") else {
            continue;
        };
        match value {
            Some(position) => {
                let position = position.parse::<usize>().map_err(|_| {
                    ApplicationError::Repository(crate::RepositoryError::CorruptData {
                        detail: format!("invalid routine habit position for {habit_id}"),
                    })
                })?;
                membership.insert(habit_id.to_owned(), position);
            }
            None => {
                membership.remove(habit_id);
            }
        }
    }
    let mut ordered: Vec<_> = membership.into_iter().collect();
    ordered.sort_by(|(left_id, left_pos), (right_id, right_pos)| {
        left_pos.cmp(right_pos).then_with(|| left_id.cmp(right_id))
    });
    let mut routine = Routine::new(id, name)?;
    for (habit_id, _) in ordered {
        let habit_id = HabitId::new(habit_id)?;
        if HabitRepository::get(repositories, &habit_id)?.is_none() {
            return Ok(false);
        }
        routine.add_habit(habit_id)?;
    }
    RoutineRepository::save(repositories, routine)?;
    Ok(true)
}

// SQL knowledge materialization was retired in M19. The semantic transport
// treats legacy knowledge envelopes as deferred, never accepted projections.
// A Markdown-aware replica/import adapter belongs to M21.

fn materialize_schedule(
    repositories: &mut impl UnitOfWork,
    id: &str,
    fields: &BTreeMap<String, Option<String>>,
) -> Result<bool, ApplicationError> {
    let id = ScheduleBlockId::new(id.to_owned())?;
    let existing = ScheduleRepository::get(repositories, &id)?;
    let Some(title) = value_or(
        fields,
        "title",
        existing.as_ref().map(|v| v.title().to_owned()),
    ) else {
        return Ok(false);
    };
    let Some(starts_at_ms) = required_i64(
        fields,
        "starts_at_ms",
        existing.as_ref().map(ScheduleBlock::starts_at_ms),
    )?
    else {
        return Ok(false);
    };
    let Some(duration_minutes) = required_u32(
        fields,
        "duration_minutes",
        existing.as_ref().map(ScheduleBlock::duration_minutes),
    )?
    else {
        return Ok(false);
    };
    let flexibility_raw = value_or(
        fields,
        "flexibility",
        existing.as_ref().map(|v| match v.flexibility() {
            ScheduleFlexibility::Fixed => "fixed".to_owned(),
            ScheduleFlexibility::Movable => "movable".to_owned(),
        }),
    );
    let Some(flexibility_raw) = flexibility_raw else {
        return Ok(false);
    };
    let flexibility = match flexibility_raw.as_str() {
        "fixed" => ScheduleFlexibility::Fixed,
        "movable" => ScheduleFlexibility::Movable,
        _ => {
            return Err(corrupt(format!(
                "invalid schedule flexibility: {flexibility_raw}"
            )));
        }
    };
    let target_kind = value_or(
        fields,
        "target_kind",
        existing.as_ref().map(|v| match v.target() {
            ScheduleTarget::Standalone => "standalone".into(),
            ScheduleTarget::Task(_) => "task".into(),
            ScheduleTarget::Habit(_) => "habit".into(),
        }),
    );
    let Some(target_kind) = target_kind else {
        return Ok(false);
    };
    let existing_target_id = existing.as_ref().and_then(|v| match v.target() {
        ScheduleTarget::Standalone => None,
        ScheduleTarget::Task(id) => Some(id.to_string()),
        ScheduleTarget::Habit(id) => Some(id.to_string()),
    });
    let target_id = optional_id_value(fields, "target_id", existing_target_id)?;
    let target = match (target_kind.as_str(), target_id) {
        ("standalone", None) => ScheduleTarget::Standalone,
        ("task", Some(id)) => {
            let id = TaskId::new(id)?;
            if TaskRepository::get(repositories, &id)?.is_none() {
                return Ok(false);
            }
            ScheduleTarget::Task(id)
        }
        ("habit", Some(id)) => {
            let id = HabitId::new(id)?;
            if HabitRepository::get(repositories, &id)?.is_none() {
                return Ok(false);
            }
            ScheduleTarget::Habit(id)
        }
        _ => return Err(corrupt("invalid schedule target".into())),
    };
    let block = ScheduleBlock::new(
        id,
        title,
        starts_at_ms,
        duration_minutes,
        flexibility,
        target,
    )?;
    ScheduleRepository::save(repositories, block)?;
    Ok(true)
}

/// Replays a metric's independently merged target and time-stamped observation fields.
/// Observations defer until their target exists; snapshots are intentionally idempotent.
fn materialize_metric(
    repositories: &mut impl UnitOfWork,
    id: &str,
    fields: &BTreeMap<String, Option<String>>,
) -> Result<bool, ApplicationError> {
    if id.trim().is_empty() || id.trim() != id {
        return Err(corrupt("invalid metric id".into()));
    }
    if let Some(raw) = fields.get("target") {
        let raw = raw
            .as_ref()
            .ok_or_else(|| corrupt("metric target deletion is not supported".into()))?;
        let parts: Vec<_> = raw.split(':').collect();
        if parts.len() != 3 {
            return Err(corrupt("invalid metric target encoding".into()));
        }
        let baseline = parts[0]
            .parse::<i64>()
            .map_err(|_| corrupt("invalid metric baseline".into()))?;
        let target = parts[1]
            .parse::<i64>()
            .map_err(|_| corrupt("invalid metric goal".into()))?;
        let direction = match parts[2] {
            "at_least" => orbit_progress::MetricTargetDirection::AtLeast,
            "at_most" => orbit_progress::MetricTargetDirection::AtMost,
            _ => return Err(corrupt("invalid metric direction".into())),
        };
        MetricEvidenceRepository::put_metric_target(
            repositories,
            &MetricTargetRecord {
                metric_id: id.to_owned(),
                baseline,
                target,
                direction,
            },
        )?;
    }
    if MetricEvidenceRepository::get_metric_target(repositories, id)?.is_none() {
        return Ok(false);
    }
    for (field, value) in fields {
        if field == "target" {
            continue;
        }
        let Some(timestamp) = field.strip_prefix("observation:") else {
            return Err(corrupt(format!("unrecognized metric field: {field}")));
        };
        let timestamp = timestamp
            .parse::<i64>()
            .map_err(|_| corrupt("invalid observation timestamp".into()))?;
        let value = value
            .as_ref()
            .ok_or_else(|| corrupt("observation deletion is unsupported".into()))?
            .parse::<i64>()
            .map_err(|_| corrupt("invalid metric observation".into()))?;
        MetricEvidenceRepository::put_metric_observation(
            repositories,
            &MetricObservation {
                metric_id: id.to_owned(),
                observed_at_ms: timestamp,
                value,
            },
        )?;
    }
    Ok(true)
}

fn value_or(
    fields: &BTreeMap<String, Option<String>>,
    key: &str,
    fallback: Option<String>,
) -> Option<String> {
    fields.get(key).cloned().unwrap_or(fallback)
}

fn optional_id_value(
    fields: &BTreeMap<String, Option<String>>,
    key: &str,
    fallback: Option<String>,
) -> Result<Option<String>, ApplicationError> {
    Ok(fields.get(key).cloned().unwrap_or(fallback))
}

fn optional_u32(
    fields: &BTreeMap<String, Option<String>>,
    key: &str,
    fallback: Option<u32>,
) -> Result<Option<u32>, ApplicationError> {
    match fields.get(key) {
        None => Ok(fallback),
        Some(None) => Ok(None),
        Some(Some(value)) => value
            .parse::<u32>()
            .map(Some)
            .map_err(|_| corrupt(format!("invalid u32 field {key}: {value}"))),
    }
}

fn optional_i64(
    fields: &BTreeMap<String, Option<String>>,
    key: &str,
    fallback: Option<i64>,
) -> Result<Option<i64>, ApplicationError> {
    match fields.get(key) {
        None => Ok(fallback),
        Some(None) => Ok(None),
        Some(Some(value)) => value
            .parse::<i64>()
            .map(Some)
            .map_err(|_| corrupt(format!("invalid i64 field {key}: {value}"))),
    }
}

fn required_u32(
    fields: &BTreeMap<String, Option<String>>,
    key: &str,
    fallback: Option<u32>,
) -> Result<Option<u32>, ApplicationError> {
    optional_u32(fields, key, fallback)
}

fn required_i64(
    fields: &BTreeMap<String, Option<String>>,
    key: &str,
    fallback: Option<i64>,
) -> Result<Option<i64>, ApplicationError> {
    optional_i64(fields, key, fallback)
}

fn parse_bool(value: &str) -> Result<bool, ApplicationError> {
    match value {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(corrupt(format!("invalid boolean: {value}"))),
    }
}

fn parse_cadence(value: &str) -> Result<HabitCadence, ApplicationError> {
    if value == "daily" {
        return Ok(HabitCadence::Daily);
    }
    if let Some(target) = value.strip_prefix("weekly:") {
        let target_per_week = target
            .parse::<u8>()
            .map_err(|_| corrupt(format!("invalid habit cadence: {value}")))?;
        return Ok(HabitCadence::Weekly { target_per_week });
    }
    Err(corrupt(format!("invalid habit cadence: {value}")))
}

fn restore_project_status(project: &mut Project, status: &str) -> Result<(), ApplicationError> {
    match status {
        "proposed" => Ok(()),
        "active" => project
            .transition_to(ProjectStatus::Active)
            .map_err(Into::into),
        "on_hold" => {
            project.transition_to(ProjectStatus::Active)?;
            project
                .transition_to(ProjectStatus::OnHold)
                .map_err(Into::into)
        }
        "completed" => {
            project.transition_to(ProjectStatus::Active)?;
            project
                .transition_to(ProjectStatus::Completed)
                .map_err(Into::into)
        }
        "cancelled" => project
            .transition_to(ProjectStatus::Cancelled)
            .map_err(Into::into),
        "archived" => project
            .transition_to(ProjectStatus::Archived)
            .map_err(Into::into),
        _ => Err(corrupt(format!("invalid project status: {status}"))),
    }
}

fn restore_milestone_status(
    milestone: &mut Milestone,
    status: &str,
) -> Result<(), ApplicationError> {
    match status {
        "planned" => Ok(()),
        "in_progress" => milestone
            .transition_to(MilestoneStatus::InProgress)
            .map_err(Into::into),
        "blocked" => milestone
            .transition_to(MilestoneStatus::Blocked)
            .map_err(Into::into),
        "completed" => {
            milestone.transition_to(MilestoneStatus::InProgress)?;
            milestone
                .transition_to(MilestoneStatus::Completed)
                .map_err(Into::into)
        }
        "cancelled" => milestone
            .transition_to(MilestoneStatus::Cancelled)
            .map_err(Into::into),
        _ => Err(corrupt(format!("invalid milestone status: {status}"))),
    }
}

fn restore_task_status(task: &mut Task, status: &str) -> Result<(), ApplicationError> {
    match status {
        "inbox" => Ok(()),
        "planned" => task.transition_to(TaskStatus::Planned).map_err(Into::into),
        "ready" => task.transition_to(TaskStatus::Ready).map_err(Into::into),
        "in_progress" => {
            task.transition_to(TaskStatus::Ready)?;
            task.transition_to(TaskStatus::InProgress)
                .map_err(Into::into)
        }
        "waiting" => {
            task.transition_to(TaskStatus::Ready)?;
            task.transition_to(TaskStatus::InProgress)?;
            task.transition_to(TaskStatus::Waiting).map_err(Into::into)
        }
        "blocked" => {
            task.transition_to(TaskStatus::Ready)?;
            task.transition_to(TaskStatus::Blocked).map_err(Into::into)
        }
        "review" => {
            task.transition_to(TaskStatus::Ready)?;
            task.transition_to(TaskStatus::InProgress)?;
            task.transition_to(TaskStatus::Review).map_err(Into::into)
        }
        "deferred" => task.transition_to(TaskStatus::Deferred).map_err(Into::into),
        "done" => {
            task.transition_to(TaskStatus::Ready)?;
            task.transition_to(TaskStatus::InProgress)?;
            task.transition_to(TaskStatus::Done).map_err(Into::into)
        }
        "cancelled" => task
            .transition_to(TaskStatus::Cancelled)
            .map_err(Into::into),
        _ => Err(corrupt(format!("invalid task status: {status}"))),
    }
}

fn corrupt(detail: String) -> ApplicationError {
    ApplicationError::Repository(crate::RepositoryError::CorruptData { detail })
}
