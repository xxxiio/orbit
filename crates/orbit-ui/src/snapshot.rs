//! Convert an accepted authority snapshot into renderer-neutral view data.
//!
//! Connected/embedded clients already receive one accepted JSON projection.
//! This adapter lets them use the same `orbit-ui` contracts as the local TUI
//! without opening a second store or reinterpreting pending commands as
//! accepted state.
use crate::data::*;
use crate::model::format_epoch_date;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

fn array<'a>(snapshot: &'a Value, key: &str) -> &'a [Value] {
    snapshot
        .get(key)
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
}
fn string(row: &Value, key: &str) -> Option<String> {
    row.get(key).and_then(Value::as_str).map(str::to_owned)
}
fn number(row: &Value, key: &str) -> Option<i64> {
    row.get(key).and_then(Value::as_i64)
}

fn projects(snapshot: &Value) -> Vec<ProjectRow> {
    array(snapshot, "projects")
        .iter()
        .map(|row| ProjectRow {
            id: string(row, "id").unwrap_or_default(),
            name: string(row, "name").unwrap_or_else(|| "Unnamed project".into()),
            status: string(row, "status").unwrap_or_else(|| "unknown".into()),
            mode: string(row, "mode").unwrap_or_else(|| "standard".into()),
            objective: string(row, "objective"),
            completion_ready: row
                .get("completion_ready")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            criteria_satisfied: 0,
            criteria_total: 0,
            health: None,
            evidence: None,
        })
        .collect()
}

fn task_statuses(snapshot: &Value) -> BTreeMap<String, String> {
    array(snapshot, "tasks")
        .iter()
        .filter_map(|row| Some((string(row, "id")?, string(row, "status")?)))
        .collect()
}

fn dependencies(snapshot: &Value) -> BTreeMap<String, Vec<String>> {
    let mut result: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for row in array(snapshot, "task_dependencies") {
        if let (Some(task), Some(prerequisite)) =
            (string(row, "task_id"), string(row, "prerequisite_id"))
        {
            result.entry(task).or_default().push(prerequisite);
        }
    }
    for values in result.values_mut() {
        values.sort();
    }
    result
}

fn tasks(snapshot: &Value) -> Vec<TaskRow> {
    let statuses = task_statuses(snapshot);
    let dependencies = dependencies(snapshot);
    let mut rows: Vec<TaskRow> = array(snapshot, "tasks")
        .iter()
        .map(|row| {
            let id = string(row, "id").unwrap_or_default();
            let status = string(row, "status").unwrap_or_else(|| "inbox".into());
            let prerequisites = dependencies.get(&id).cloned().unwrap_or_default();
            let blocking: Vec<String> = prerequisites
                .iter()
                .filter(|prerequisite| {
                    statuses
                        .get(*prerequisite)
                        .is_some_and(|state| state != "done" && state != "cancelled")
                })
                .cloned()
                .collect();
            TaskRow {
                id: id.clone(),
                title: string(row, "title").unwrap_or_default(),
                status: status.clone(),
                project_id: string(row, "project_id"),
                milestone_id: string(row, "milestone_id"),
                estimated_minutes: number(row, "estimated_minutes").map(|value| value as u32),
                earliest_start_at_ms: number(row, "earliest_start_at_ms"),
                deadline_at_ms: number(row, "deadline_at_ms"),
                actionable: status == "ready" && blocking.is_empty(),
                readiness: status,
                prerequisite_ids: prerequisites,
                blocking_task_ids: blocking,
                allowed_statuses: Vec::new(),
                priority: string(row, "priority").unwrap_or_default(),
                due_date: string(row, "due_date").unwrap_or_else(|| {
                    number(row, "deadline_at_ms")
                        .filter(|deadline| *deadline != 0)
                        .map(format_epoch_date)
                        .unwrap_or_default()
                }),
                acceptance_total: number(row, "acceptance_total").unwrap_or(0).max(0) as usize,
                acceptance_completed: number(row, "acceptance_completed").unwrap_or(0).max(0)
                    as usize,
            }
        })
        .collect();
    rows.sort_by(|left, right| left.id.cmp(&right.id));
    rows
}

fn milestones(snapshot: &Value, tasks: &[TaskRow]) -> Vec<MilestoneRow> {
    array(snapshot, "milestones")
        .iter()
        .map(|row| {
            let id = string(row, "id").unwrap_or_default();
            let scoped: Vec<&TaskRow> = tasks
                .iter()
                .filter(|task| task.milestone_id.as_deref() == Some(&id))
                .collect();
            MilestoneRow {
                id,
                project_id: string(row, "project_id").unwrap_or_default(),
                name: string(row, "name").unwrap_or_default(),
                status: string(row, "status").unwrap_or_else(|| "planned".into()),
                task_total: scoped.len(),
                task_done: scoped.iter().filter(|task| task.status == "done").count(),
            }
        })
        .collect()
}

fn habits(snapshot: &Value, now_ms: i64) -> Vec<HabitRow> {
    let cutoff = now_ms.saturating_sub(28 * 24 * 60 * 60 * 1000);
    let occurrences = array(snapshot, "habit_occurrences");
    array(snapshot, "habits")
        .iter()
        .map(|row| {
            let id = string(row, "id").unwrap_or_default();
            let recent: Vec<i64> = occurrences
                .iter()
                .filter(|occurrence| string(occurrence, "habit_id").as_deref() == Some(&id))
                .filter_map(|occurrence| number(occurrence, "occurred_at_ms"))
                .filter(|at| *at >= cutoff)
                .collect();
            HabitRow {
                id,
                name: string(row, "name").unwrap_or_default(),
                cadence: string(row, "cadence").unwrap_or_else(|| "daily".into()),
                active: row.get("active").and_then(Value::as_bool).unwrap_or(true),
                occurrences_28d: recent.len(),
                last_occurred_at_ms: recent.into_iter().max(),
            }
        })
        .collect()
}

fn metrics(snapshot: &Value) -> Vec<MetricRow> {
    let observations = array(snapshot, "metric_observations");
    array(snapshot, "metrics")
        .iter()
        .map(|row| {
            let id = string(row, "id").unwrap_or_default();
            let mut rows: Vec<MetricObservationRow> = observations
                .iter()
                .filter(|observation| string(observation, "metric_id").as_deref() == Some(&id))
                .filter_map(|observation| {
                    Some(MetricObservationRow {
                        observed_at_ms: number(observation, "observed_at_ms")?,
                        value: number(observation, "value")?,
                    })
                })
                .collect();
            rows.sort_by_key(|observation| observation.observed_at_ms);
            MetricRow {
                id,
                baseline: number(row, "baseline").unwrap_or(0),
                target: number(row, "target").unwrap_or(0),
                direction: string(row, "direction").unwrap_or_else(|| "at_least".into()),
                latest: rows.last().map(|observation| observation.value),
                observations: rows,
            }
        })
        .collect()
}

fn schedule(snapshot: &Value) -> Vec<ScheduleRow> {
    array(snapshot, "schedule")
        .iter()
        .map(|row| ScheduleRow {
            id: string(row, "id").unwrap_or_default(),
            title: string(row, "title").unwrap_or_default(),
            starts_at_ms: number(row, "starts_at_ms").unwrap_or(0),
            ends_at_ms: number(row, "ends_at_ms").unwrap_or(0),
            flexibility: string(row, "flexibility").unwrap_or_else(|| "movable".into()),
            target_kind: string(row, "target_kind").unwrap_or_default(),
            target_id: string(row, "target_id"),
        })
        .collect()
}

fn knowledge(snapshot: &Value) -> Vec<KnowledgeRow> {
    array(snapshot, "knowledge")
        .iter()
        .map(|row| KnowledgeRow {
            id: string(row, "id").unwrap_or_default(),
            kind: string(row, "kind").unwrap_or_else(|| "note".into()),
            title: string(row, "title").unwrap_or_default(),
            status: string(row, "status").unwrap_or_else(|| "draft".into()),
            provenance: string(row, "provenance").unwrap_or_else(|| "human".into()),
            updated_at_ms: number(row, "updated_at_ms").unwrap_or(0),
        })
        .collect()
}

fn revision(snapshot: &Value) -> Option<i64> {
    number(snapshot, "revision")
}

/// Build the global workspace projection from accepted-only snapshot data.
pub fn workspace_from_snapshot(
    snapshot: Option<&Value>,
    now_ms: i64,
) -> Result<WorkspaceData, ViewDataError> {
    let Some(snapshot) = snapshot else {
        return Ok(WorkspaceData::default());
    };
    let tasks = tasks(snapshot);
    let inbox = tasks
        .iter()
        .filter(|task| task.status == "inbox")
        .cloned()
        .collect();
    Ok(WorkspaceData {
        projects: projects(snapshot),
        tasks,
        inbox,
        active_habits: habits(snapshot, now_ms)
            .into_iter()
            .filter(|habit| habit.active)
            .collect(),
        recent_knowledge: knowledge(snapshot),
        agent_jobs: Vec::new(),
        pending_commands: 0,
        revision: revision(snapshot),
    })
}

/// Build a project-scoped projection from accepted-only snapshot data.
pub fn project_from_snapshot(
    snapshot: Option<&Value>,
    project_id: &str,
    now_ms: i64,
) -> Result<ProjectData, ViewDataError> {
    let Some(snapshot) = snapshot else {
        return Err(ViewDataError::new(
            "no_accepted_snapshot",
            "No accepted snapshot is cached; connected views require an accepted projection",
        ));
    };
    let project = projects(snapshot)
        .into_iter()
        .find(|project| project.id == project_id)
        .ok_or_else(|| {
            ViewDataError::invalid(format!(
                "project {project_id} is not in the accepted snapshot"
            ))
        })?;
    let all_tasks = tasks(snapshot);
    let tasks: Vec<TaskRow> = all_tasks
        .iter()
        .filter(|task| task.project_id.as_deref() == Some(project_id))
        .cloned()
        .collect();
    let milestones = milestones(snapshot, &tasks)
        .into_iter()
        .filter(|milestone| milestone.project_id == project_id)
        .collect();
    let task_ids: BTreeSet<&str> = tasks.iter().map(|task| task.id.as_str()).collect();
    let schedule = schedule(snapshot)
        .into_iter()
        .filter(|block| {
            block.target_id.as_deref() == Some(project_id)
                || block
                    .target_id
                    .as_deref()
                    .is_some_and(|id| task_ids.contains(id))
        })
        .collect();
    Ok(ProjectData {
        revision: revision(snapshot),
        project,
        tasks,
        work_items: Vec::new(),
        milestones,
        knowledge: knowledge(snapshot),
        habits: habits(snapshot, now_ms),
        metrics: metrics(snapshot),
        agent_jobs: Vec::new(),
        schedule,
        pending_commands: 0,
        observed_at_ms: now_ms,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn snapshot() -> Value {
        json!({
            "revision": 7,
            "history_fingerprint": "f".repeat(64),
            "projects": [
                {"id":"p1","name":"Connected","status":"active"},
                {"id":"p2","name":"Other","status":"proposed"}
            ],
            "tasks": [
                {"id":"t1","title":"Ready task","status":"ready","project_id":"p1","estimated_minutes":30,"deadline_at_ms":1700000000000i64,"priority":"high"},
                {"id":"t2","title":"Blocked task","status":"ready","project_id":"p1"},
                {"id":"t3","title":"Inbox","status":"inbox"},
                {"id":"t4","title":"Done prerequisite","status":"done","project_id":"p1"}
            ],
            "task_dependencies": [
                {"task_id":"t1","prerequisite_id":"t4"},
                {"task_id":"t2","prerequisite_id":"t1"}
            ],
            "milestones": [{"id":"m1","project_id":"p1","name":"M1","status":"in_progress"}],
            "habits": [{"id":"h1","name":"Walk","cadence":"daily"}],
            "habit_occurrences": [
                {"id":"o1","habit_id":"h1","occurred_at_ms":1000}
            ],
            "metrics": [{"id":"metric1","baseline":0,"target":10,"direction":"at_least"}],
            "metric_observations": [
                {"metric_id":"metric1","observed_at_ms":1,"value":4},
                {"metric_id":"metric1","observed_at_ms":2,"value":6}
            ],
            "schedule": [{"id":"s1","title":"Block","starts_at_ms":10,"ends_at_ms":20,"flexibility":"fixed","target_kind":"project","target_id":"p1"}],
            "knowledge": [{"id":"k1","kind":"note","title":"Evidence","status":"active","provenance":"human","updated_at_ms":5}]
        })
    }

    #[test]
    fn accepted_snapshot_maps_to_shared_workspace_contract() {
        let workspace = workspace_from_snapshot(Some(&snapshot()), 31_536_000_000).unwrap();
        assert_eq!(workspace.projects.len(), 2);
        assert_eq!(workspace.tasks.len(), 4);
        assert_eq!(workspace.inbox.len(), 1);
        assert_eq!(workspace.active_habits.len(), 1);
        assert_eq!(workspace.recent_knowledge.len(), 1);
        assert!(
            workspace
                .tasks
                .iter()
                .any(|task| task.id == "t1" && task.actionable)
        );
        let t1 = workspace.tasks.iter().find(|task| task.id == "t1").unwrap();
        assert_eq!(t1.priority, "high");
        assert_eq!(t1.due_date, "2023-11-14");
        assert!(
            workspace
                .tasks
                .iter()
                .any(|task| task.id == "t2" && !task.actionable)
        );
    }

    #[test]
    fn project_projection_is_scoped_and_never_invents_work_items() {
        let data = project_from_snapshot(Some(&snapshot()), "p1", 31_536_000_000).unwrap();
        assert_eq!(data.tasks.len(), 3);
        assert_eq!(data.milestones.len(), 1);
        assert_eq!(data.schedule.len(), 1);
        assert!(data.work_items.is_empty());
        assert_eq!(data.metrics[0].latest, Some(6));
        assert!(project_from_snapshot(Some(&snapshot()), "missing", 0).is_err());
    }

    #[test]
    fn absent_connected_snapshot_is_not_accepted_state() {
        assert!(
            workspace_from_snapshot(None, 0)
                .unwrap()
                .projects
                .is_empty()
        );
        assert!(project_from_snapshot(None, "p1", 0).is_err());
    }
}
