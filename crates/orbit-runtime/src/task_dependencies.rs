//! Deterministic, read-only task readiness over canonical task records.
//!
//! No AI, clock, network, or writable database is needed for this evaluation.
//! Readiness is intentionally different from a task's persisted workflow status:
//! an inbox task can be unblocked but not actionable; a `ready` task with an
//! unfinished prerequisite is blocked even if its workflow status is stale.
use crate::domain_commands::TaskRecord;
use crate::store::{AuthorityError, AuthorityStore};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskDependencyEdge {
    pub task_id: String,
    pub prerequisite_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReadinessReason {
    Ready,
    UnfinishedPrerequisites,
    ScheduledForLater,
    PastDeadline,
    Inbox,
    Planned,
    Waiting,
    Blocked,
    Deferred,
    InProgress,
    Review,
    Done,
    Cancelled,
    UnknownStatus,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskReadiness {
    pub task_id: String,
    pub title: String,
    pub status: String,
    pub actionable: bool,
    pub reason: ReadinessReason,
    /// Direct prerequisites not yet completed, sorted by ID.
    pub blocking_task_ids: Vec<String>,
    /// All direct prerequisites, including completed work.
    pub prerequisite_ids: Vec<String>,
    /// Scheduling metadata is preserved for explainable planning.
    pub earliest_start_at_ms: Option<i64>,
    pub deadline_at_ms: Option<i64>,
    pub estimated_minutes: Option<i64>,
    pub project_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskReadinessSnapshot {
    pub revision: i64,
    pub edges: Vec<TaskDependencyEdge>,
    pub tasks: Vec<TaskReadiness>,
    pub actionable_count: usize,
    pub blocked_count: usize,
}

/// The caller supplies its interpretation of `now` to guarantee deterministic
/// and reproducible offline/online results with identical input snapshots.
pub fn evaluate_readiness(
    revision: i64,
    tasks: &[TaskRecord],
    edges: &[TaskDependencyEdge],
    now_ms: i64,
) -> Result<TaskReadinessSnapshot, &'static str> {
    if now_ms < 0 || revision < 0 {
        return Err("invalid evaluation time or revision");
    }
    let by_id: BTreeMap<&str, &TaskRecord> = tasks.iter().map(|t| (t.id.as_str(), t)).collect();
    if by_id.len() != tasks.len() {
        return Err("duplicate task identifiers");
    }
    let mut dependencies: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for edge in edges {
        let dependent = edge.task_id.as_str();
        let prerequisite = edge.prerequisite_id.as_str();
        if dependent == prerequisite {
            return Err("self dependency");
        }
        if !by_id.contains_key(dependent) || !by_id.contains_key(prerequisite) {
            return Err("dangling task dependency");
        }
        if !dependencies
            .entry(dependent)
            .or_default()
            .insert(prerequisite)
        {
            return Err("duplicate task dependency");
        }
    }
    // Also fail closed for graph data from an untrusted or corrupted replica.
    fn visit<'a>(
        node: &'a str,
        graph: &BTreeMap<&'a str, BTreeSet<&'a str>>,
        visiting: &mut BTreeSet<&'a str>,
        done: &mut BTreeSet<&'a str>,
    ) -> Result<(), &'static str> {
        if done.contains(node) {
            return Ok(());
        }
        if !visiting.insert(node) {
            return Err("cyclic task dependencies");
        }
        if let Some(prerequisites) = graph.get(node) {
            for prerequisite in prerequisites {
                visit(prerequisite, graph, visiting, done)?;
            }
        }
        visiting.remove(node);
        done.insert(node);
        Ok(())
    }
    let mut visiting = BTreeSet::new();
    let mut done = BTreeSet::new();
    for task in tasks {
        visit(&task.id, &dependencies, &mut visiting, &mut done)?;
    }

    let mut rows = Vec::with_capacity(tasks.len());
    for task in tasks {
        let prerequisites = dependencies.get(task.id.as_str());
        let prerequisite_ids: Vec<String> = prerequisites
            .iter()
            .flat_map(|set| set.iter())
            .map(|id| (*id).to_owned())
            .collect();
        let blocking_task_ids: Vec<String> = prerequisite_ids
            .iter()
            .filter(|id| {
                by_id
                    .get(id.as_str())
                    .is_some_and(|prior| prior.status != "done")
            })
            .cloned()
            .collect();
        // A real dependency always wins over a user-selected workflow label.
        let reason = if matches!(task.status.as_str(), "done") {
            ReadinessReason::Done
        } else if task.status == "cancelled" {
            ReadinessReason::Cancelled
        } else if !blocking_task_ids.is_empty() {
            ReadinessReason::UnfinishedPrerequisites
        } else {
            match task.status.as_str() {
                "ready" if task.earliest_start_at_ms.is_some_and(|when| when > now_ms) => {
                    ReadinessReason::ScheduledForLater
                }
                "ready" if task.deadline_at_ms.is_some_and(|when| when < now_ms) => {
                    ReadinessReason::PastDeadline
                }
                "ready" => ReadinessReason::Ready,
                "inbox" => ReadinessReason::Inbox,
                "planned" => ReadinessReason::Planned,
                "waiting" => ReadinessReason::Waiting,
                "blocked" => ReadinessReason::Blocked,
                "deferred" => ReadinessReason::Deferred,
                "in_progress" => ReadinessReason::InProgress,
                "review" => ReadinessReason::Review,
                _ => ReadinessReason::UnknownStatus,
            }
        };
        // An overdue task is still actionable: missing its deadline must not
        // make it disappear from the user's work queue.
        let actionable = matches!(
            reason,
            ReadinessReason::Ready | ReadinessReason::PastDeadline
        );
        rows.push(TaskReadiness {
            task_id: task.id.clone(),
            title: task.title.clone(),
            status: task.status.clone(),
            actionable,
            reason,
            blocking_task_ids,
            prerequisite_ids,
            earliest_start_at_ms: task.earliest_start_at_ms,
            deadline_at_ms: task.deadline_at_ms,
            estimated_minutes: task.estimated_minutes,
            project_id: task.project_id.clone(),
        });
    }
    rows.sort_by(|a, b| a.task_id.cmp(&b.task_id));
    let actionable_count = rows.iter().filter(|r| r.actionable).count();
    let blocked_count = rows
        .iter()
        .filter(|r| !r.blocking_task_ids.is_empty())
        .count();
    let mut edges = edges.to_vec();
    edges.sort_by(|a, b| (&a.task_id, &a.prerequisite_id).cmp(&(&b.task_id, &b.prerequisite_id)));
    Ok(TaskReadinessSnapshot {
        revision,
        edges,
        tasks: rows,
        actionable_count,
        blocked_count,
    })
}

impl AuthorityStore {
    pub fn task_dependency_edges(&self) -> Result<Vec<TaskDependencyEdge>, AuthorityError> {
        let mut stmt=self.conn.prepare("SELECT task_id,prerequisite_id FROM task_prerequisites ORDER BY task_id,prerequisite_id")?;
        let rows = stmt.query_map([], |row| {
            Ok(TaskDependencyEdge {
                task_id: row.get(0)?,
                prerequisite_id: row.get(1)?,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }
    pub fn task_readiness(&self, now_ms: i64) -> Result<TaskReadinessSnapshot, AuthorityError> {
        // One read transaction ensures dependency and task snapshots are coherent.
        let tx = self.conn.unchecked_transaction()?;
        let revision: i64 = tx.query_row(
            "SELECT COALESCE(MAX(cursor),0) FROM authority_history",
            [],
            |r| r.get(0),
        )?;
        let tasks = {
            let mut stmt=tx.prepare("SELECT id,title,status,project_id,milestone_id,estimated_minutes,earliest_start_at_ms,deadline_at_ms,description,priority FROM tasks ORDER BY id")?;
            stmt.query_map([], |r| {
                Ok(TaskRecord {
                    id: r.get(0)?,
                    title: r.get(1)?,
                    status: r.get(2)?,
                    project_id: r.get(3)?,
                    milestone_id: r.get(4)?,
                    estimated_minutes: r.get(5)?,
                    earliest_start_at_ms: r.get(6)?,
                    deadline_at_ms: r.get(7)?,
                    description: r.get(8)?,
                    priority: r.get(9)?,
                    labels: Vec::new(),
                })
            })?
            .collect::<Result<Vec<_>, _>>()?
        };
        let edges = {
            let mut stmt=tx.prepare("SELECT task_id,prerequisite_id FROM task_prerequisites ORDER BY task_id,prerequisite_id")?;
            stmt.query_map([], |r| {
                Ok(TaskDependencyEdge {
                    task_id: r.get(0)?,
                    prerequisite_id: r.get(1)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?
        };
        evaluate_readiness(revision, &tasks, &edges, now_ms).map_err(AuthorityError::Invalid)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn task(id: &str, status: &str) -> TaskRecord {
        TaskRecord {
            id: id.into(),
            title: id.into(),
            status: status.into(),
            project_id: None,
            milestone_id: None,
            estimated_minutes: Some(30),
            earliest_start_at_ms: None,
            deadline_at_ms: None,
            description: String::new(),
            priority: None,
            labels: Vec::new(),
        }
    }
    fn edge(task: &str, prerequisite: &str) -> TaskDependencyEdge {
        TaskDependencyEdge {
            task_id: task.into(),
            prerequisite_id: prerequisite.into(),
        }
    }
    #[test]
    fn readiness_follows_prerequisite_completion_not_only_status() {
        let tasks = [task("build", "ready"), task("design", "ready")];
        let snapshot = evaluate_readiness(2, &tasks, &[edge("build", "design")], 500).unwrap();
        assert_eq!(snapshot.actionable_count, 1);
        assert_eq!(snapshot.blocked_count, 1);
        assert_eq!(
            snapshot.tasks[0].reason,
            ReadinessReason::UnfinishedPrerequisites
        );
        assert_eq!(snapshot.tasks[0].blocking_task_ids, ["design"]);
        let completed = [task("build", "ready"), task("design", "done")];
        let snapshot = evaluate_readiness(3, &completed, &[edge("build", "design")], 500).unwrap();
        assert!(snapshot.tasks[0].actionable);
    }
    #[test]
    fn readiness_fails_closed_for_corrupt_graphs() {
        let tasks = [task("a", "ready"), task("b", "ready")];
        for bad in [
            vec![edge("a", "a")],
            vec![edge("a", "missing")],
            vec![edge("a", "b"), edge("b", "a")],
            vec![edge("a", "b"), edge("a", "b")],
        ] {
            assert!(evaluate_readiness(0, &tasks, &bad, 0).is_err());
        }
    }
    #[test]
    fn scheduled_and_overdue_tasks_are_explained() {
        let mut future = task("a", "ready");
        future.earliest_start_at_ms = Some(200);
        let mut overdue = task("b", "ready");
        overdue.deadline_at_ms = Some(10);
        let result = evaluate_readiness(0, &[future, overdue], &[], 100).unwrap();
        assert_eq!(result.tasks[0].reason, ReadinessReason::ScheduledForLater);
        assert_eq!(result.tasks[1].reason, ReadinessReason::PastDeadline);
        assert!(result.tasks[1].actionable);
    }
}

#[cfg(test)]
mod authority_contract_tests {
    use super::*;
    use crate::{
        AuthoritativeHost,
        domain_commands::{TaskAction, TaskCommand},
    };
    use std::path::Path;
    fn edge(task: &str, prerequisite: &str) -> TaskDependencyEdge {
        TaskDependencyEdge {
            task_id: task.into(),
            prerequisite_id: prerequisite.into(),
        }
    }
    fn command(id: &str, revision: i64, action: TaskAction) -> TaskCommand {
        TaskCommand {
            command_id: id.into(),
            expected_revision: revision,
            action,
        }
    }
    fn create(host: &mut AuthoritativeHost, id: &str, revision: i64) {
        host.task_command(
            "owner",
            command(
                &format!("create-{id}"),
                revision,
                TaskAction::Create {
                    task_id: id.into(),
                    title: format!("Task {id}"),
                    project_id: None,
                },
            ),
        )
        .unwrap();
    }
    fn add(
        host: &mut AuthoritativeHost,
        command_id: &str,
        revision: i64,
        dependent: &str,
        prerequisite: &str,
    ) -> Result<crate::domain_commands::TaskResult, AuthorityError> {
        host.task_command(
            "owner",
            command(
                command_id,
                revision,
                TaskAction::AddPrerequisite {
                    task_id: dependent.into(),
                    prerequisite_id: prerequisite.into(),
                },
            ),
        )
    }
    fn remove(
        host: &mut AuthoritativeHost,
        command_id: &str,
        revision: i64,
        dependent: &str,
        prerequisite: &str,
    ) -> Result<crate::domain_commands::TaskResult, AuthorityError> {
        host.task_command(
            "owner",
            command(
                command_id,
                revision,
                TaskAction::RemovePrerequisite {
                    task_id: dependent.into(),
                    prerequisite_id: prerequisite.into(),
                },
            ),
        )
    }
    #[test]
    fn edge_add_is_revisioned_atomic_and_retry_safe() {
        let mut host = AuthoritativeHost::open(Path::new(":memory:")).unwrap();
        create(&mut host, "a", 0);
        create(&mut host, "b", 1);
        let receipt = add(&mut host, "edge", 2, "b", "a").unwrap();
        assert_eq!(receipt.revision, 3);
        assert!(!receipt.duplicate);
        let duplicate = add(&mut host, "edge", 2, "b", "a").unwrap();
        assert_eq!(duplicate.revision, 3);
        assert!(duplicate.duplicate);
        assert_eq!(host.snapshot().unwrap().0, 3);
        assert_eq!(host.task_dependency_edges().unwrap(), [edge("b", "a")]);
        assert!(matches!(
            add(&mut host, "edge-again", 3, "b", "a"),
            Err(AuthorityError::ImmutableConflict)
        ));
        assert_eq!(host.snapshot().unwrap().0, 3);
    }
    #[test]
    fn cyclic_mutations_are_rejected_without_receipts_or_partial_edges() {
        let mut host = AuthoritativeHost::open(":memory:").unwrap();
        for (index, id) in ["a", "b", "c", "d"].into_iter().enumerate() {
            create(&mut host, id, index as i64);
        }
        add(&mut host, "first", 4, "c", "b").unwrap();
        add(&mut host, "second", 5, "b", "a").unwrap();
        add(&mut host, "third", 6, "d", "c").unwrap();
        let before = host.task_dependency_edges().unwrap();
        let revision = host.snapshot().unwrap().0;
        assert!(matches!(
            add(&mut host, "cycle", revision, "a", "d"),
            Err(AuthorityError::Invalid(_))
        ));
        assert!(matches!(
            add(&mut host, "self", revision, "a", "a"),
            Err(AuthorityError::Invalid(_))
        ));
        assert!(matches!(
            add(&mut host, "missing", revision, "a", "missing"),
            Err(AuthorityError::Invalid(_))
        ));
        assert_eq!(host.snapshot().unwrap().0, revision);
        assert_eq!(host.task_dependency_edges().unwrap(), before);
        // The ID rejected for a cycle can still be admitted on a valid command:
        // rejection did not persist a receipt or consume a global revision.
        remove(&mut host, "cycle", revision, "d", "c").unwrap();
        assert_eq!(host.snapshot().unwrap().0, revision + 1);
    }
    #[test]
    fn stale_concurrent_writes_require_a_fresh_revision() {
        let mut host = AuthoritativeHost::open(":memory:").unwrap();
        create(&mut host, "a", 0);
        create(&mut host, "b", 1);
        create(&mut host, "c", 2);
        add(&mut host, "device-a", 3, "c", "b").unwrap();
        assert!(matches!(
            add(&mut host, "device-b", 3, "c", "a"),
            Err(AuthorityError::StaleRevision)
        ));
        assert_eq!(host.task_dependency_edges().unwrap(), [edge("c", "b")]);
        add(&mut host, "device-b", 4, "c", "a").unwrap();
        assert_eq!(
            host.task_dependency_edges().unwrap(),
            [edge("c", "a"), edge("c", "b")]
        );
    }
    #[test]
    fn removal_is_revisioned_and_does_not_change_other_edges() {
        let mut host = AuthoritativeHost::open(":memory:").unwrap();
        create(&mut host, "a", 0);
        create(&mut host, "b", 1);
        create(&mut host, "c", 2);
        add(&mut host, "ab", 3, "a", "b").unwrap();
        add(&mut host, "ac", 4, "a", "c").unwrap();
        let receipt = remove(&mut host, "remove", 5, "a", "b").unwrap();
        assert_eq!(receipt.revision, 6);
        assert_eq!(host.task_dependency_edges().unwrap(), [edge("a", "c")]);
        assert!(remove(&mut host, "missing-removal", 6, "a", "b").is_err());
        assert_eq!(host.task_dependency_edges().unwrap(), [edge("a", "c")]);
        assert_eq!(host.snapshot().unwrap().0, 6);
    }
    #[test]
    fn readiness_considers_current_accepted_status_not_pending_intentions() {
        let mut host = AuthoritativeHost::open(":memory:").unwrap();
        create(&mut host, "a", 0);
        create(&mut host, "b", 1);
        add(&mut host, "edge", 2, "b", "a").unwrap();
        let prior = host.task_readiness(123).unwrap();
        assert_eq!(prior.tasks[1].blocking_task_ids, ["a"]);
        assert_eq!(prior.revision, 3);
        assert!(!prior.tasks[1].actionable);
        assert_eq!(prior.edges, [edge("b", "a")]);
        assert!(host.task_readiness(-1).is_err());
    }
    #[test]
    fn malformed_dependency_commands_never_consume_revisions() {
        let mut host = AuthoritativeHost::open(":memory:").unwrap();
        create(&mut host, "a", 0);
        create(&mut host, "b", 1);
        let cases = [
            (
                "self",
                TaskAction::AddPrerequisite {
                    task_id: "a".into(),
                    prerequisite_id: "a".into(),
                },
            ),
            (
                "absent-prerequisite",
                TaskAction::AddPrerequisite {
                    task_id: "a".into(),
                    prerequisite_id: "nope".into(),
                },
            ),
            (
                "absent-dependent",
                TaskAction::AddPrerequisite {
                    task_id: "nope".into(),
                    prerequisite_id: "a".into(),
                },
            ),
            (
                "missing-removal",
                TaskAction::RemovePrerequisite {
                    task_id: "a".into(),
                    prerequisite_id: "b".into(),
                },
            ),
        ];
        for (id, action) in cases {
            assert!(host.task_command("owner", command(id, 2, action)).is_err());
            assert_eq!(host.snapshot().unwrap().0, 2);
            assert!(host.task_dependency_edges().unwrap().is_empty());
        }
    }
}
