//! Legacy connected-palette operations that the shared shell did not yet
//! offer, now expressed as one pure command grammar for the `Modal::Console`
//! handler.
//!
//! Nothing here performs I/O, queues a command or mutates accepted state:
//! [`parse`] turns one console line into a typed request and [`approve_request`]
//! re-validates a reviewed scenario against the currently accepted revision
//! before the shell performs the single typed client call. Read-only analyses
//! return data through the shell; they never reach the outbox.
use crate::planning_ui;
use orbit_runtime::planning_approval::{
    ApprovedScheduleReceipt, ApprovedScheduleRequest, PlanApproval,
};
use orbit_runtime::planning_focus::{FocusRequest, GroupFocusRequest};
use orbit_runtime::planning_scenarios::ScenarioCompareRequest;
use orbit_runtime::portfolio_actions::PortfolioActionRequest;
use orbit_runtime::scenario_execution::{ScenarioExecutionRequest, ScenarioExecutionReview};

/// One console line that is either a read-only analysis or an explicit,
/// separately confirmed approval of a reviewed scenario.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    /// Read-only project-aware next actions over the accepted snapshot.
    Portfolio(PortfolioActionRequest),
    /// Read-only what-if comparison over the accepted snapshot.
    Compare(ScenarioCompareRequest),
    /// Read-only, online reconstruction of a scenario as exact calendar slots.
    Review(ScenarioExecutionRequest),
    /// Apply the previously reviewed scenario; needs the exact `--confirm`.
    Approve,
    /// Read-only accepted task context.
    FocusTask(FocusRequest),
    /// Read-only accepted project/milestone context.
    FocusGroup(GroupFocusRequest),
    /// Read one accepted Markdown body by id or vault-relative path.
    NoteShow(String),
    /// Read-only accepted routine compositions.
    RoutineList,
    /// Read-only accepted project strategies.
    StrategyList,
    /// Read-only accepted milestones.
    MilestoneList,
    /// Read-only accepted schedule blocks.
    ScheduleList,
    /// Compare the cached replica report with a live authority observation.
    ObserveSync,
}

fn require_accepted(accepted: Option<i64>) -> Result<i64, String> {
    accepted.ok_or_else(|| "No accepted snapshot yet; refresh while online first".to_owned())
}

/// Resolve one console line before the write grammar runs.
///
/// Returns `Ok(None)` when the line belongs to another console command, so
/// ordinary writes keep their existing confirmation flow. `accepted_revision`
/// is the revision the shell last observed; planning commands fail closed
/// without one. `unique_id` supplies the review identity the caller also uses
/// for any later command receipt.
pub fn parse(
    source: &str,
    now_ms: i64,
    accepted_revision: Option<i64>,
    unique_id: &str,
) -> Result<Option<Command>, String> {
    let source = source.trim();
    let op = source.split_whitespace().next().unwrap_or_default();
    match op {
        // Approval is only ever the exact reviewed command; every prefix or
        // extra option is rejected rather than interpreted.
        "plan.approve" => {
            if source == "plan.approve --confirm" {
                Ok(Some(Command::Approve))
            } else {
                Err("Approval requires the exact command: plan.approve --confirm".into())
            }
        }
        "plan.portfolio" => {
            require_accepted(accepted_revision)?;
            match planning_ui::parse_portfolio(source, now_ms)? {
                Some(request) => Ok(Some(Command::Portfolio(request))),
                None => Ok(None),
            }
        }
        "plan.compare" => {
            let revision = require_accepted(accepted_revision)?;
            match planning_ui::parse_scenario(source, now_ms, revision)? {
                Some(request) => Ok(Some(Command::Compare(request))),
                None => Ok(None),
            }
        }
        "plan.review" => {
            let revision = require_accepted(accepted_revision)?;
            match planning_ui::parse_scenario_review(source, now_ms, revision, unique_id)? {
                Some(request) => Ok(Some(Command::Review(request))),
                None => Ok(None),
            }
        }
        "plan.focus" => {
            require_accepted(accepted_revision)?;
            let words: Vec<_> = source.split_whitespace().collect();
            if words.len() != 2 || words[1].trim().is_empty() {
                return Err("usage: plan.focus <task-id>".into());
            }
            if now_ms < 0 {
                return Err("invalid evaluation time".into());
            }
            Ok(Some(Command::FocusTask(FocusRequest {
                task_id: words[1].to_owned(),
                max_neighbors: 12,
            })))
        }
        "plan.group" => {
            require_accepted(accepted_revision)?;
            let words: Vec<_> = source.split_whitespace().collect();
            if (words.len() != 3 && words.len() != 5)
                || words.get(1) != Some(&"--project")
                || (words.len() == 5 && words.get(3) != Some(&"--milestone"))
                || words[2].trim().is_empty()
                || words.get(4).is_some_and(|value| value.trim().is_empty())
            {
                return Err(
                    "usage: plan.group --project <project-id> [--milestone <milestone-id>]".into(),
                );
            }
            Ok(Some(Command::FocusGroup(GroupFocusRequest {
                project_id: words[2].to_owned(),
                milestone_id: words.get(4).map(|value| (*value).to_owned()),
                max_tasks: 12,
            })))
        }
        "note.show" => {
            let target = source
                .strip_prefix("note.show")
                .unwrap_or_default()
                .trim()
                .to_owned();
            if target.is_empty() {
                return Err("usage: note.show <note-id|vault-relative-path.md>".into());
            }
            Ok(Some(Command::NoteShow(target)))
        }
        "sync.observe" => {
            if source != "sync.observe" {
                return Err("usage: sync.observe".into());
            }
            Ok(Some(Command::ObserveSync))
        }
        // Accepted-state listings. They read the accepted snapshot only; a
        // connected deployment without one fails inside the typed client, and
        // queued intentions never appear in these rows.
        "routine.list" => {
            if source != "routine.list" {
                return Err("usage: routine.list".into());
            }
            Ok(Some(Command::RoutineList))
        }
        "strategy.list" => {
            if source != "strategy.list" {
                return Err("usage: strategy.list".into());
            }
            Ok(Some(Command::StrategyList))
        }
        "milestone.list" => {
            if source != "milestone.list" {
                return Err("usage: milestone.list".into());
            }
            Ok(Some(Command::MilestoneList))
        }
        "schedule.list" => {
            if source != "schedule.list" {
                return Err("usage: schedule.list".into());
            }
            Ok(Some(Command::ScheduleList))
        }
        // These operations are covered by shared shell views; say where rather
        // than silently reporting an unknown command.
        "plan.today" => Err(
            "plan.today is the shared Home / Today view: press g h, then [ / ] for the horizon"
                .into(),
        ),
        "plan.impact" => Err(
            "plan.impact is the read-only completion-impact view: select a task and press d".into(),
        ),
        _ => Ok(None),
    }
}

/// Re-validate a reviewed scenario and build the exact approval request.
///
/// Fail closed: no review, a review fenced to another accepted revision, a
/// review that claims to have written, or a review whose plan does not match
/// its own reviewed slots all refuse to produce a request. This is the same
/// rule the legacy palette applied, with the revision check done locally before
/// the server sees anything.
pub fn approve_request(
    review: Option<&ScenarioExecutionReview>,
    accepted_revision: i64,
) -> Result<ApprovedScheduleRequest, String> {
    let review = review.ok_or_else(|| {
        "No reviewed scenario is awaiting approval; use plan.review first".to_owned()
    })?;
    if !review.read_only {
        return Err(
            "Reviewed scenario claims a write already happened; run plan.review again".into(),
        );
    }
    if review.review.expected_revision != accepted_revision
        || review.plan.expected_revision != accepted_revision
    {
        return Err(format!(
            "Accepted revision changed since the review (reviewed {}, now {}); run plan.review again and inspect every slot",
            review.review.expected_revision, accepted_revision
        ));
    }
    if review.review.command_id != review.plan.command_id {
        return Err("Reviewed scenario identity is inconsistent; run plan.review again".into());
    }
    if review.review.scheduled_slots.len() != review.plan.items.len() {
        return Err("Reviewed scenario is missing exact slots; run plan.review again".into());
    }
    if !review.review.requires_explicit_approval {
        return Err("Reviewed scenario does not require explicit approval".into());
    }
    Ok(ApprovedScheduleRequest {
        plan: review.plan.clone(),
        approval: PlanApproval {
            review: review.review.clone(),
            approved: true,
        },
    })
}

/// State the authoritative receipt and revision after an approval, so a user
/// never has to infer success from a cleared modal.
pub fn receipt_line(receipt: &ApprovedScheduleReceipt) -> String {
    format!(
        "Authority accepted {} calendar reservation(s) at revision {} ({}). Press r to refresh accepted state.",
        receipt.schedule_block_ids.len(),
        receipt.accepted_revision,
        if receipt.replayed {
            "replayed receipt, no duplicate blocks"
        } else {
            "new blocks created"
        }
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use orbit_runtime::planning_approval::{PlanReview, ScheduleApprovalItem};
    use orbit_runtime::planning_scenarios::{ScenarioComparison, ScenarioResult};
    use orbit_runtime::scenario_execution::ScenarioExecutionReview;

    fn comparison() -> ScenarioComparison {
        let result = ScenarioResult {
            name: "what-if".into(),
            capacity_minutes: 120,
            allocated_minutes: 30,
            remaining_minutes: 90,
            chosen: Vec::new(),
            excluded: Vec::new(),
            newly_selected: Vec::new(),
            displaced: Vec::new(),
            explanation: Vec::new(),
        };
        ScenarioComparison {
            accepted_revision: 7,
            evaluated_at_ms: 0,
            baseline: result.clone(),
            alternatives: vec![result],
            warnings: Vec::new(),
            read_only: true,
        }
    }

    fn review() -> ScenarioExecutionReview {
        ScenarioExecutionReview {
            selected_scenario: "what-if".into(),
            comparison: comparison(),
            plan: orbit_runtime::planning_approval::ScheduleApprovalRequest {
                command_id: "session-schedule".into(),
                expected_revision: 7,
                window_start_ms: 0,
                window_end_ms: 3_600_000,
                items: vec![ScheduleApprovalItem {
                    task_id: "t1".into(),
                    starts_at_ms: 0,
                    ends_at_ms: 1_800_000,
                    schedule_block_id: "session-slot".into(),
                    activity_id: "session-activity".into(),
                }],
            },
            review: PlanReview {
                expected_revision: 7,
                actor: "human".into(),
                command_id: "session-schedule".into(),
                fingerprint: "a".repeat(64),
                scheduled_block_ids: vec!["session-slot".into()],
                scheduled_tasks: vec!["t1".into()],
                scheduled_slots: vec![orbit_runtime::planning_approval::ReviewedScheduleSlot {
                    task_id: "t1".into(),
                    schedule_block_id: "session-slot".into(),
                    starts_at_ms: 0,
                    ends_at_ms: 1_800_000,
                    duration_minutes: 30,
                }],
                total_reserved_minutes: 30,
                requires_explicit_approval: true,
                creates_calendar_blocks: true,
            },
            warnings: Vec::new(),
            read_only: true,
        }
    }

    #[test]
    fn approval_requires_a_prior_review() {
        let error = approve_request(None, 7).unwrap_err();
        assert!(error.contains("plan.review"), "{error}");
    }

    #[test]
    fn approval_is_fenced_to_the_same_accepted_revision() {
        let error = approve_request(Some(&review()), 8).unwrap_err();
        assert!(error.contains("revision changed"), "{error}");
        let request = approve_request(Some(&review()), 7).expect("matching revision");
        assert!(request.approval.approved);
        assert_eq!(request.plan.items.len(), 1);
        assert_eq!(request.approval.review, review().review);
    }

    #[test]
    fn a_review_that_claims_a_write_or_omits_slots_is_refused() {
        let mut written = review();
        written.read_only = false;
        assert!(approve_request(Some(&written), 7).is_err());

        let mut missing = review();
        missing.plan.items.clear();
        assert!(approve_request(Some(&missing), 7).is_err());

        let mut other_command = review();
        other_command.plan.command_id = "other".into();
        assert!(approve_request(Some(&other_command), 7).is_err());
    }

    #[test]
    fn parsing_is_bounded_and_never_accepts_implicit_approval() {
        assert_eq!(
            parse("plan.approve --confirm", 0, None, "session").unwrap(),
            Some(Command::Approve)
        );
        for invalid in [
            "plan.approve",
            "plan.approve --confirm extra",
            "plan.approve now",
        ] {
            assert!(parse(invalid, 0, None, "session").is_err(), "{invalid}");
        }
    }

    #[test]
    fn planning_commands_fail_closed_without_accepted_state() {
        for command in [
            "plan.portfolio",
            "plan.compare",
            "plan.review",
            "plan.focus t1",
            "plan.group --project p",
        ] {
            let error = parse(command, 0, None, "session").unwrap_err();
            assert!(error.contains("No accepted snapshot"), "{command}: {error}");
        }
    }

    #[test]
    fn planning_commands_reuse_the_shared_revision_bound_parsers() {
        let portfolio = parse("plan.portfolio --limit 5 --project p", 100, Some(3), "s")
            .unwrap()
            .unwrap();
        let Command::Portfolio(request) = portfolio else {
            panic!("expected portfolio");
        };
        assert_eq!(request.limit, 5);
        assert_eq!(request.project_id.as_deref(), Some("p"));

        let compare = parse("plan.compare --minutes 90 --pin t1", 100, Some(3), "s")
            .unwrap()
            .unwrap();
        let Command::Compare(request) = compare else {
            panic!("expected comparison");
        };
        assert_eq!(request.expected_revision, 3);
        assert_eq!(request.alternatives[0].pinned_task_ids, vec!["t1"]);

        let review = parse("plan.review --hours 2", 65_000, Some(3), "session-1")
            .unwrap()
            .unwrap();
        let Command::Review(request) = review else {
            panic!("expected review");
        };
        assert_eq!(request.command_id, "session-1-schedule");
        assert_eq!(request.comparison.expected_revision, 3);
    }

    #[test]
    fn context_and_note_commands_have_exact_grammar() {
        let focus = parse("plan.focus t1", 0, Some(3), "s").unwrap().unwrap();
        assert_eq!(
            focus,
            Command::FocusTask(FocusRequest {
                task_id: "t1".into(),
                max_neighbors: 12
            })
        );
        let group = parse("plan.group --project p --milestone m", 0, Some(3), "s")
            .unwrap()
            .unwrap();
        assert_eq!(
            group,
            Command::FocusGroup(GroupFocusRequest {
                project_id: "p".into(),
                milestone_id: Some("m".into()),
                max_tasks: 12
            })
        );
        for invalid in [
            "plan.focus",
            "plan.focus t1 extra",
            "plan.group --milestone m",
            "plan.group --project p --bad m",
            "note.show",
            "sync.observe now",
        ] {
            assert!(parse(invalid, 0, Some(3), "s").is_err(), "{invalid}");
        }
        assert_eq!(
            parse("note.show docs/plan.md", 0, None, "s").unwrap(),
            Some(Command::NoteShow("docs/plan.md".into()))
        );
        assert_eq!(
            parse("sync.observe", 0, None, "s").unwrap(),
            Some(Command::ObserveSync)
        );
        assert_eq!(parse("task.create Something", 0, None, "s").unwrap(), None);
    }

    #[test]
    fn receipt_line_states_revision_and_block_count() {
        let receipt = ApprovedScheduleReceipt {
            accepted_revision: 9,
            replayed: false,
            schedule_block_ids: vec!["a".into(), "b".into()],
        };
        let line = receipt_line(&receipt);
        assert!(line.contains("2 calendar reservation"), "{line}");
        assert!(line.contains("revision 9"), "{line}");
        assert!(line.contains("Press r"), "{line}");
    }
}
