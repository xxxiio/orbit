//! Explicit approval gate for validated, authoritative schedule-plan commands.
//!
//! Review never mutates the canonical workspace. Approval is bound to the exact
//! typed command, actor, command identity and accepted global revision. Actual
//! creation delegates to the existing atomic, idempotent application ledger.
use crate::{AuthoritativeHost, AuthorityError};
use orbit_application::ApplyValidatedPlanCommand;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewedScheduleSlot {
    pub task_id: String,
    pub schedule_block_id: String,
    pub starts_at_ms: i64,
    pub ends_at_ms: i64,
    pub duration_minutes: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanReview {
    pub expected_revision: i64,
    pub actor: String,
    pub command_id: String,
    pub fingerprint: String,
    pub scheduled_block_ids: Vec<String>,
    pub scheduled_tasks: Vec<String>,
    pub scheduled_slots: Vec<ReviewedScheduleSlot>,
    pub total_reserved_minutes: i64,
    pub requires_explicit_approval: bool,
    pub creates_calendar_blocks: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanApproval {
    pub review: PlanReview,
    /// Must be asserted by a human or explicitly authorized agent after the
    /// caller has presented the exact reviewed schedule and side effects.
    pub approved: bool,
}

fn valid_identity(value: &str) -> bool {
    !value.trim().is_empty()
        && value.len() <= 256
        && !value.contains('\0')
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b':'))
}

pub fn fingerprint(
    actor: &str,
    command_id: &str,
    expected_revision: i64,
    command: &ApplyValidatedPlanCommand,
) -> Result<String, AuthorityError> {
    if !valid_identity(actor) || !valid_identity(command_id) || expected_revision < 0 {
        return Err(AuthorityError::Invalid("invalid plan approval identity"));
    }
    // Domain command Debug is already used to bind command ledger receipts.
    // Use explicit lengths to avoid concatenation ambiguity between identities.
    let mut digest = Sha256::new();
    for part in ["orbit.plan-approval.v1", actor, command_id] {
        digest.update((part.len() as u64).to_be_bytes());
        digest.update(part.as_bytes());
    }
    digest.update(expected_revision.to_be_bytes());
    let payload = format!("{command:?}");
    digest.update((payload.len() as u64).to_be_bytes());
    digest.update(payload.as_bytes());
    Ok(format!("{:x}", digest.finalize()))
}

pub fn prepare_review(
    actor: &str,
    command_id: &str,
    expected_revision: i64,
    command: &ApplyValidatedPlanCommand,
) -> Result<PlanReview, AuthorityError> {
    let fingerprint = fingerprint(actor, command_id, expected_revision, command)?;
    let mut blocks = Vec::new();
    let mut tasks = Vec::new();
    let mut distinct_blocks = std::collections::BTreeSet::new();
    let mut distinct_tasks = std::collections::BTreeSet::new();
    for item in &command.identities {
        let block_id = item.schedule_block_id.to_string();
        let task_id = item.task_id.to_string();
        if !distinct_blocks.insert(block_id.clone()) || !distinct_tasks.insert(task_id.clone()) {
            return Err(AuthorityError::Invalid(
                "repeated schedule or task identity in reviewed plan",
            ));
        }
        blocks.push(block_id);
        tasks.push(task_id);
    }
    if blocks.is_empty() || blocks.len() != command.proposal.proposal().scheduled.len() {
        return Err(AuthorityError::Invalid(
            "review requires matching nonempty schedule identities",
        ));
    }
    let mut scheduled_slots = Vec::with_capacity(command.proposal.proposal().scheduled.len());
    for scheduled in &command.proposal.proposal().scheduled {
        let Some(identity) = command
            .identities
            .iter()
            .find(|identity| identity.task_id == scheduled.task_id)
        else {
            return Err(AuthorityError::Invalid(
                "plan identity missing for scheduled slot",
            ));
        };
        let duration_ms = scheduled
            .end_ms
            .checked_sub(scheduled.start_ms)
            .ok_or(AuthorityError::Invalid("schedule duration overflow"))?;
        if duration_ms <= 0 || duration_ms % 60_000 != 0 {
            return Err(AuthorityError::Invalid(
                "invalid reviewed schedule duration",
            ));
        }
        scheduled_slots.push(ReviewedScheduleSlot {
            task_id: scheduled.task_id.to_string(),
            schedule_block_id: identity.schedule_block_id.to_string(),
            starts_at_ms: scheduled.start_ms,
            ends_at_ms: scheduled.end_ms,
            duration_minutes: duration_ms / 60_000,
        });
    }
    let total_reserved_minutes = scheduled_slots.iter().try_fold(0i64, |total, slot| {
        total
            .checked_add(slot.duration_minutes)
            .ok_or(AuthorityError::Invalid(
                "approved plan total duration overflow",
            ))
    })?;
    Ok(PlanReview {
        expected_revision,
        actor: actor.to_owned(),
        command_id: command_id.to_owned(),
        fingerprint,
        scheduled_block_ids: blocks,
        scheduled_tasks: tasks,
        scheduled_slots,
        total_reserved_minutes,
        requires_explicit_approval: true,
        creates_calendar_blocks: true,
    })
}

pub fn validate_approval(
    approval: &PlanApproval,
    actor: &str,
    command_id: &str,
    expected_revision: i64,
    command: &ApplyValidatedPlanCommand,
) -> Result<(), AuthorityError> {
    if !approval.approved {
        return Err(AuthorityError::Invalid(
            "plan execution requires explicit approval",
        ));
    }
    let actual = prepare_review(actor, command_id, expected_revision, command)?;
    if actual != approval.review {
        return Err(AuthorityError::Invalid(
            "plan approval does not match reviewed command",
        ));
    }
    Ok(())
}

impl AuthoritativeHost {
    /// Review against the current canonical revision; no calendar changes.
    pub fn review_task_plan_approval(
        &self,
        actor: &str,
        command_id: &str,
        expected_revision: i64,
        command: &ApplyValidatedPlanCommand,
    ) -> Result<PlanReview, AuthorityError> {
        if self.snapshot()?.0 != expected_revision {
            return Err(AuthorityError::Invalid(
                "accepted revision changed before plan review",
            ));
        }
        prepare_review(actor, command_id, expected_revision, command)
    }

    /// Execute only a previously reviewed, explicitly approved command.
    /// The original authoritative transaction validates schedule conflicts,
    /// revision fencing, command replay and receipt identity atomically.
    pub fn apply_approved_task_plan_accepted(
        &mut self,
        approval: PlanApproval,
        actor: &str,
        command_id: &str,
        expected_revision: i64,
        command: ApplyValidatedPlanCommand,
    ) -> Result<(i64, bool, Vec<String>), AuthorityError> {
        validate_approval(&approval, actor, command_id, expected_revision, &command)?;
        self.apply_task_plan_accepted(actor, command_id, expected_revision, command)
    }
}

/// Wire-safe exact schedule command submitted by a human or agent.
/// All identities are client-generated and stable across retries. The server
/// derives the actor from the authenticated device, never from request JSON.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScheduleApprovalItem {
    pub task_id: String,
    pub starts_at_ms: i64,
    pub ends_at_ms: i64,
    pub schedule_block_id: String,
    pub activity_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScheduleApprovalRequest {
    pub command_id: String,
    pub expected_revision: i64,
    pub window_start_ms: i64,
    pub window_end_ms: i64,
    pub items: Vec<ScheduleApprovalItem>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApprovedScheduleRequest {
    pub plan: ScheduleApprovalRequest,
    pub approval: PlanApproval,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApprovedScheduleReceipt {
    pub accepted_revision: i64,
    pub replayed: bool,
    pub schedule_block_ids: Vec<String>,
}

/// Why a submitted schedule failed validation, in the first failing rule's terms.
///
/// The approval gate's refusal is read by a person deciding whether to change the
/// window or a task's deadline, so it says which rule failed instead of only that
/// validation failed. The refusal text is fixed human copy (`AuthorityError::
/// Invalid` carries `&'static str`); the offending task is named by the review
/// surface from the plan it submitted.
fn violation_message(violations: &[orbit_planner::PlanViolation]) -> &'static str {
    use orbit_planner::PlanViolation;
    match violations.first() {
        Some(PlanViolation::UnknownTask(_)) => {
            "approved schedule failed domain validation: a task is no longer in this workspace"
        }
        Some(PlanViolation::DuplicateTask(_)) => {
            "approved schedule failed domain validation: a task appears twice"
        }
        Some(PlanViolation::TaskNotEligible(_)) => {
            "approved schedule failed domain validation: a task is no longer in a status that can be planned"
        }
        Some(PlanViolation::MissingEstimate(_)) => {
            "approved schedule failed domain validation: a task has no duration estimate"
        }
        Some(PlanViolation::DurationMismatch(_)) => {
            "approved schedule failed domain validation: a block's length does not match its task's estimate"
        }
        Some(PlanViolation::OutsidePlanningWindow(_)) => {
            "approved schedule failed domain validation: a block falls outside the planning window"
        }
        Some(PlanViolation::BeforeEarliestStart(_)) => {
            "approved schedule failed domain validation: a block starts before its task's earliest start"
        }
        Some(PlanViolation::AfterDeadline(_)) => {
            "approved schedule failed domain validation: a block would end after its task's deadline"
        }
        Some(PlanViolation::OverlapsExistingSchedule(_)) => {
            "approved schedule failed domain validation: a block overlaps time already on the calendar"
        }
        Some(PlanViolation::ViolatesFixedBuffer(_)) => {
            "approved schedule failed domain validation: a block uses the buffer around a fixed commitment"
        }
        Some(PlanViolation::OverlapsProposedTask(_)) => {
            "approved schedule failed domain validation: two blocks in the plan overlap"
        }
        Some(PlanViolation::CapacityExceeded) => {
            "approved schedule failed domain validation: the plan needs more time than the window has free"
        }
        None => "approved schedule failed domain validation",
    }
}

fn build_command(
    host: &AuthoritativeHost,
    request: &ScheduleApprovalRequest,
    permit_existing_receipt_blocks: bool,
) -> Result<ApplyValidatedPlanCommand, AuthorityError> {
    use orbit_application::PlanApplicationIdentity;
    use orbit_domain::{ActivityId, ActivityOrigin, ScheduleBlockId, TaskId};
    use orbit_planner::{
        CapacityPolicy, FixedBlockBufferPolicy, PlanProposal, PlanningWindow, ProposedTaskBlock,
        validate_proposal_with_buffers,
    };
    if !valid_identity(&request.command_id) || request.expected_revision < 0 {
        return Err(AuthorityError::Invalid(
            "invalid approved plan command identity",
        ));
    }
    if request.items.is_empty() || request.items.len() > 100 {
        return Err(AuthorityError::Invalid(
            "plan requires 1 to 100 scheduled tasks",
        ));
    }
    let window = PlanningWindow::new(request.window_start_ms, request.window_end_ms)
        .map_err(|_| AuthorityError::Invalid("invalid approved plan window"))?;
    let mut scheduled = Vec::new();
    let mut identities = Vec::new();
    let mut seen_tasks = std::collections::BTreeSet::new();
    let mut seen_blocks = std::collections::BTreeSet::new();
    let mut seen_activities = std::collections::BTreeSet::new();
    for item in &request.items {
        if !seen_tasks.insert(item.task_id.as_str())
            || !seen_blocks.insert(item.schedule_block_id.as_str())
            || !seen_activities.insert(item.activity_id.as_str())
        {
            return Err(AuthorityError::Invalid(
                "duplicate approved plan task or identity",
            ));
        }
        let task_id = TaskId::new(item.task_id.clone())
            .map_err(|_| AuthorityError::Invalid("invalid task identity"))?;
        let block_id = ScheduleBlockId::new(item.schedule_block_id.clone())
            .map_err(|_| AuthorityError::Invalid("invalid schedule identity"))?;
        let activity_id = ActivityId::new(item.activity_id.clone())
            .map_err(|_| AuthorityError::Invalid("invalid activity identity"))?;
        if item
            .ends_at_ms
            .checked_sub(item.starts_at_ms)
            .is_none_or(|minutes_ms| minutes_ms <= 0 || minutes_ms % 60_000 != 0)
        {
            return Err(AuthorityError::Invalid(
                "approved schedule requires whole-minute nonempty blocks",
            ));
        }
        scheduled.push(ProposedTaskBlock {
            task_id: task_id.clone(),
            start_ms: item.starts_at_ms,
            end_ms: item.ends_at_ms,
        });
        identities.push(PlanApplicationIdentity {
            task_id,
            schedule_block_id: block_id,
            activity_id,
        });
    }
    let queries = host.application_queries();
    let tasks = queries
        .tasks()
        .map_err(|_| AuthorityError::Invalid("cannot load accepted tasks"))?;
    let mut blocks = queries
        .schedule_between(request.window_start_ms, request.window_end_ms)
        .map_err(|_| AuthorityError::Invalid("cannot load accepted schedule"))?;
    // On a successful network retry, the original blocks already exist.
    // Reconstruct the *same* validated command so the authority receipt can
    // distinguish a legitimate retry from an ID collision. The ledger still
    // owns immutable payload equality and transaction-time safety checks.
    if permit_existing_receipt_blocks {
        blocks.retain(|block| !seen_blocks.contains(block.id().as_str()));
    }
    let policy = CapacityPolicy::default();
    let fixed_buffers = FixedBlockBufferPolicy::default();
    let validated = validate_proposal_with_buffers(
        window,
        &blocks,
        &tasks,
        policy,
        fixed_buffers,
        PlanProposal {
            scheduled,
            unscheduled: vec![],
        },
    )
    .map_err(|violations| AuthorityError::Invalid(violation_message(&violations)))?;
    Ok(ApplyValidatedPlanCommand {
        window,
        policy,
        fixed_buffers,
        proposal: validated,
        identities,
        origin: ActivityOrigin::Human,
    })
}

impl AuthoritativeHost {
    /// Evaluate and fingerprint the exact schedule submitted over HTTP. The
    /// actor is the verified transport device, not a client-controlled field.
    pub fn review_schedule_request(
        &self,
        actor: &str,
        request: &ScheduleApprovalRequest,
    ) -> Result<PlanReview, AuthorityError> {
        if self.snapshot()?.0 != request.expected_revision {
            return Err(AuthorityError::StaleRevision);
        }
        let command = build_command(self, request, false)?;
        self.review_task_plan_approval(
            actor,
            &request.command_id,
            request.expected_revision,
            &command,
        )
    }

    /// Reconstruct and revalidate the submitted schedule, then atomically
    /// delegate to the ledger. A replay returns the original receipt.
    pub fn execute_schedule_request(
        &mut self,
        actor: &str,
        request: ApprovedScheduleRequest,
    ) -> Result<ApprovedScheduleReceipt, AuthorityError> {
        let command = build_command(self, &request.plan, true)?;
        let (revision, replayed, ids) = self.apply_approved_task_plan_accepted(
            request.approval,
            actor,
            &request.plan.command_id,
            request.plan.expected_revision,
            command,
        )?;
        Ok(ApprovedScheduleReceipt {
            accepted_revision: revision,
            replayed,
            schedule_block_ids: ids,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn authority_with_ready_task() -> AuthoritativeHost {
        use crate::domain_commands::{TaskAction, TaskCommand};
        let mut host = AuthoritativeHost::open(":memory:").expect("authority");
        host.task_command(
            "device-a",
            TaskCommand {
                command_id: "task-create".into(),
                expected_revision: 0,
                action: TaskAction::Create {
                    task_id: "approved-task".into(),
                    title: "Execute reviewed plan".into(),
                    project_id: None,
                },
            },
        )
        .expect("create");
        host.task_command(
            "device-a",
            TaskCommand {
                command_id: "task-estimate".into(),
                expected_revision: 1,
                action: TaskAction::SetPlanning {
                    task_id: "approved-task".into(),
                    estimated_minutes: Some(30),
                    earliest_start_at_ms: None,
                    deadline_at_ms: None,
                },
            },
        )
        .expect("planning");
        host.task_command(
            "device-a",
            TaskCommand {
                command_id: "task-ready".into(),
                expected_revision: 2,
                action: TaskAction::ChangeStatus {
                    task_id: "approved-task".into(),
                    status: "ready".into(),
                },
            },
        )
        .expect("ready");
        host
    }
    fn wire_plan() -> ScheduleApprovalRequest {
        ScheduleApprovalRequest {
            command_id: "plan-cmd".into(),
            expected_revision: 3,
            window_start_ms: 0,
            window_end_ms: 7_200_000,
            items: vec![ScheduleApprovalItem {
                task_id: "approved-task".into(),
                starts_at_ms: 0,
                ends_at_ms: 1_800_000,
                schedule_block_id: "scheduled-approved-task".into(),
                activity_id: "activity-approved-task".into(),
            }],
        }
    }
    #[test]
    fn a_task_whose_deadline_already_passed_can_still_be_reviewed() {
        use crate::domain_commands::{TaskAction, TaskCommand};
        let mut host = AuthoritativeHost::open(":memory:").expect("authority");
        host.task_command(
            "device-a",
            TaskCommand {
                command_id: "overdue-create".into(),
                expected_revision: 0,
                action: TaskAction::Create {
                    task_id: "overdue-task".into(),
                    title: "Overdue work".into(),
                    project_id: None,
                },
            },
        )
        .expect("create");
        // The deadline is already past when the window opens: no block inside
        // the window can meet it, so it must not decide whether the window's own
        // proposal is reviewable (D-01).
        host.task_command(
            "device-a",
            TaskCommand {
                command_id: "overdue-planning".into(),
                expected_revision: 1,
                action: TaskAction::SetPlanning {
                    task_id: "overdue-task".into(),
                    estimated_minutes: Some(30),
                    earliest_start_at_ms: None,
                    deadline_at_ms: Some(20_000_000),
                },
            },
        )
        .expect("planning");
        host.task_command(
            "device-a",
            TaskCommand {
                command_id: "overdue-ready".into(),
                expected_revision: 2,
                action: TaskAction::ChangeStatus {
                    task_id: "overdue-task".into(),
                    status: "ready".into(),
                },
            },
        )
        .expect("ready");

        let plan = ScheduleApprovalRequest {
            command_id: "overdue-review".into(),
            expected_revision: 3,
            window_start_ms: 200_000_000,
            window_end_ms: 207_200_000,
            items: vec![ScheduleApprovalItem {
                task_id: "overdue-task".into(),
                starts_at_ms: 200_000_000,
                ends_at_ms: 201_800_000,
                schedule_block_id: "scheduled-overdue-task".into(),
                activity_id: "activity-overdue-task".into(),
            }],
        };

        let review = host
            .review_schedule_request("device-a", &plan)
            .expect("a plan the server proposed must be reviewable");
        assert_eq!(review.scheduled_tasks, vec!["overdue-task"]);
    }

    #[test]
    fn a_deadline_still_ahead_still_refuses_a_late_block() {
        use crate::domain_commands::{TaskAction, TaskCommand};
        let mut host = AuthoritativeHost::open(":memory:").expect("authority");
        host.task_command(
            "device-a",
            TaskCommand {
                command_id: "ahead-create".into(),
                expected_revision: 0,
                action: TaskAction::Create {
                    task_id: "ahead-task".into(),
                    title: "Due soon".into(),
                    project_id: None,
                },
            },
        )
        .expect("create");
        // Ahead of the window start, so a block can still end in time.
        host.task_command(
            "device-a",
            TaskCommand {
                command_id: "ahead-planning".into(),
                expected_revision: 1,
                action: TaskAction::SetPlanning {
                    task_id: "ahead-task".into(),
                    estimated_minutes: Some(30),
                    earliest_start_at_ms: None,
                    deadline_at_ms: Some(200_500_000),
                },
            },
        )
        .expect("planning");
        host.task_command(
            "device-a",
            TaskCommand {
                command_id: "ahead-ready".into(),
                expected_revision: 2,
                action: TaskAction::ChangeStatus {
                    task_id: "ahead-task".into(),
                    status: "ready".into(),
                },
            },
        )
        .expect("ready");

        let plan = ScheduleApprovalRequest {
            command_id: "ahead-review".into(),
            expected_revision: 3,
            window_start_ms: 200_000_000,
            window_end_ms: 207_200_000,
            items: vec![ScheduleApprovalItem {
                task_id: "ahead-task".into(),
                starts_at_ms: 200_000_000,
                ends_at_ms: 201_800_000,
                schedule_block_id: "scheduled-ahead-task".into(),
                activity_id: "activity-ahead-task".into(),
            }],
        };

        assert!(host.review_schedule_request("device-a", &plan).is_err());
        assert_eq!(host.snapshot().unwrap().0, 3);
    }

    #[test]
    fn wire_review_has_no_calendar_side_effect() {
        let host = authority_with_ready_task();
        let review = host
            .review_schedule_request("device-a", &wire_plan())
            .unwrap();
        assert_eq!(review.expected_revision, 3);
        assert_eq!(review.scheduled_tasks, vec!["approved-task"]);
        assert_eq!(review.scheduled_block_ids, vec!["scheduled-approved-task"]);
        assert_eq!(host.snapshot().unwrap().0, 3);
        assert!(
            host.application_queries()
                .schedule_between(0, 7_200_000)
                .unwrap()
                .is_empty()
        );
    }
    #[test]
    fn wire_rejection_does_not_consume_revision() {
        let mut host = authority_with_ready_task();
        let request = wire_plan();
        let review = host.review_schedule_request("device-a", &request).unwrap();
        assert!(
            host.execute_schedule_request(
                "device-a",
                ApprovedScheduleRequest {
                    plan: request,
                    approval: PlanApproval {
                        review,
                        approved: false
                    }
                }
            )
            .is_err()
        );
        assert_eq!(host.snapshot().unwrap().0, 3);
        assert!(
            host.application_queries()
                .schedule_between(0, 7_200_000)
                .unwrap()
                .is_empty()
        );
    }
    #[test]
    fn wire_approval_commits_atomically_and_retries_idempotently() {
        let mut host = authority_with_ready_task();
        let plan = wire_plan();
        let review = host.review_schedule_request("device-a", &plan).unwrap();
        let request = ApprovedScheduleRequest {
            plan,
            approval: PlanApproval {
                review,
                approved: true,
            },
        };
        let receipt = host
            .execute_schedule_request("device-a", request.clone())
            .unwrap();
        assert_eq!(receipt.schedule_block_ids, vec!["scheduled-approved-task"]);
        assert_eq!(receipt.accepted_revision, 4);
        assert!(!receipt.replayed);
        assert_eq!(
            host.application_queries()
                .schedule_between(0, 7_200_000)
                .unwrap()
                .len(),
            1
        );
        let retry = host.execute_schedule_request("device-a", request).unwrap();
        assert!(retry.replayed);
        assert_eq!(retry.accepted_revision, 4);
        assert_eq!(
            host.application_queries()
                .schedule_between(0, 7_200_000)
                .unwrap()
                .len(),
            1
        );
    }
    #[test]
    fn wire_approval_rejects_tampering_and_foreign_actor() {
        let mut host = authority_with_ready_task();
        let plan = wire_plan();
        let review = host.review_schedule_request("device-a", &plan).unwrap();
        let approved = PlanApproval {
            review,
            approved: true,
        };
        assert!(
            host.execute_schedule_request(
                "device-b",
                ApprovedScheduleRequest {
                    plan: plan.clone(),
                    approval: approved.clone(),
                }
            )
            .is_err()
        );
        let mut modified = plan;
        modified.items[0].ends_at_ms = 3_600_000;
        assert!(
            host.execute_schedule_request(
                "device-a",
                ApprovedScheduleRequest {
                    plan: modified,
                    approval: approved,
                }
            )
            .is_err()
        );
        assert_eq!(host.snapshot().unwrap().0, 3);
        assert!(
            host.application_queries()
                .schedule_between(0, 7_200_000)
                .unwrap()
                .is_empty()
        );
    }
    #[test]
    fn wire_stale_review_preserves_accepted_schedule() {
        use crate::domain_commands::{TaskAction, TaskCommand};
        let mut host = authority_with_ready_task();
        let plan = wire_plan();
        let review = host.review_schedule_request("device-a", &plan).unwrap();
        host.task_command(
            "device-a",
            TaskCommand {
                command_id: "concurrent-task".into(),
                expected_revision: 3,
                action: TaskAction::Create {
                    task_id: "new-task".into(),
                    title: "Concurrent edit".into(),
                    project_id: None,
                },
            },
        )
        .unwrap();
        assert!(
            host.execute_schedule_request(
                "device-a",
                ApprovedScheduleRequest {
                    plan,
                    approval: PlanApproval {
                        review,
                        approved: true
                    }
                }
            )
            .is_err()
        );
        assert_eq!(host.snapshot().unwrap().0, 4);
        assert!(
            host.application_queries()
                .schedule_between(0, 7_200_000)
                .unwrap()
                .is_empty()
        );
    }
    #[test]
    fn exact_slot_timestamps_are_visible_during_review() {
        let host = authority_with_ready_task();
        let review = host
            .review_schedule_request("device-a", &wire_plan())
            .unwrap();
        assert_eq!(review.scheduled_slots.len(), 1);
        assert_eq!(review.total_reserved_minutes, 30);
        let slot = &review.scheduled_slots[0];
        assert_eq!(slot.task_id, "approved-task");
        assert_eq!(slot.schedule_block_id, "scheduled-approved-task");
        assert_eq!(slot.starts_at_ms, 0);
        assert_eq!(slot.ends_at_ms, 1_800_000);
        assert_eq!(slot.duration_minutes, 30);
        let serialized = serde_json::to_string(&review).unwrap();
        let roundtrip: PlanReview = serde_json::from_str(&serialized).unwrap();
        assert_eq!(review, roundtrip);
    }
    #[test]
    fn wrong_actor_cannot_replay_committed_plan() {
        let mut host = authority_with_ready_task();
        let plan = wire_plan();
        let review = host.review_schedule_request("device-a", &plan).unwrap();
        let original = ApprovedScheduleRequest {
            plan,
            approval: PlanApproval {
                review,
                approved: true,
            },
        };
        host.execute_schedule_request("device-a", original.clone())
            .unwrap();
        assert!(
            host.execute_schedule_request("device-b", original.clone())
                .is_err()
        );
        assert_eq!(host.snapshot().unwrap().0, 4);
        assert_eq!(
            host.application_queries()
                .schedule_between(0, 7_200_000)
                .unwrap()
                .len(),
            1
        );
        let retry = host.execute_schedule_request("device-a", original).unwrap();
        assert!(retry.replayed);
    }
    #[test]
    fn non_minute_and_empty_schedule_slots_are_rejected() {
        let host = authority_with_ready_task();
        let mut plan = wire_plan();
        plan.items[0].ends_at_ms = 1;
        assert!(host.review_schedule_request("device-a", &plan).is_err());
        plan.items[0].ends_at_ms = 0;
        assert!(host.review_schedule_request("device-a", &plan).is_err());
        plan.items[0].ends_at_ms = 1_800_000;
        plan.items[0].starts_at_ms = 1_800_000;
        assert!(host.review_schedule_request("device-a", &plan).is_err());
    }
    #[test]
    fn reviewed_plan_rejects_changed_command_id_even_on_same_slot() {
        let mut host = authority_with_ready_task();
        let plan = wire_plan();
        let review = host.review_schedule_request("device-a", &plan).unwrap();
        let mut modified = plan;
        modified.command_id = "replaced-command".into();
        assert!(
            host.execute_schedule_request(
                "device-a",
                ApprovedScheduleRequest {
                    plan: modified,
                    approval: PlanApproval {
                        review,
                        approved: true
                    },
                }
            )
            .is_err()
        );
        assert_eq!(host.snapshot().unwrap().0, 3);
    }
    #[test]
    fn unknown_json_fields_fail_closed_for_reviews_and_approvals() {
        let plan = wire_plan();
        let mut document = serde_json::to_value(&plan).unwrap();
        document["force"] = serde_json::json!(true);
        assert!(serde_json::from_value::<ScheduleApprovalRequest>(document).is_err());
        let host = authority_with_ready_task();
        let review = host.review_schedule_request("device-a", &plan).unwrap();
        let mut approval = serde_json::to_value(PlanApproval {
            review,
            approved: true,
        })
        .unwrap();
        approval["override_revision"] = serde_json::json!(4);
        assert!(serde_json::from_value::<PlanApproval>(approval).is_err());
        assert_eq!(host.snapshot().unwrap().0, 3);
        assert!(
            host.application_queries()
                .schedule_between(0, 7_200_000)
                .unwrap()
                .is_empty()
        );
    }
    #[test]
    fn multi_task_approval_creates_one_atomic_revision() {
        use crate::domain_commands::{TaskAction, TaskCommand};
        let mut host = authority_with_ready_task();
        host.task_command(
            "device-a",
            TaskCommand {
                command_id: "second-create".into(),
                expected_revision: 3,
                action: TaskAction::Create {
                    task_id: "second-task".into(),
                    title: "Second approved task".into(),
                    project_id: None,
                },
            },
        )
        .unwrap();
        host.task_command(
            "device-a",
            TaskCommand {
                command_id: "second-planning".into(),
                expected_revision: 4,
                action: TaskAction::SetPlanning {
                    task_id: "second-task".into(),
                    estimated_minutes: Some(30),
                    earliest_start_at_ms: None,
                    deadline_at_ms: None,
                },
            },
        )
        .unwrap();
        host.task_command(
            "device-a",
            TaskCommand {
                command_id: "second-status".into(),
                expected_revision: 5,
                action: TaskAction::ChangeStatus {
                    task_id: "second-task".into(),
                    status: "ready".into(),
                },
            },
        )
        .unwrap();
        let mut plan = wire_plan();
        plan.expected_revision = 6;
        plan.items.push(ScheduleApprovalItem {
            task_id: "second-task".into(),
            starts_at_ms: 1_800_000,
            ends_at_ms: 3_600_000,
            schedule_block_id: "second-block".into(),
            activity_id: "second-activity".into(),
        });
        let review = host.review_schedule_request("device-a", &plan).unwrap();
        assert_eq!(review.total_reserved_minutes, 60);
        assert_eq!(review.scheduled_slots.len(), 2);
        assert_eq!(
            host.application_queries()
                .schedule_between(0, 7_200_000)
                .unwrap()
                .len(),
            0
        );
        let receipt = host
            .execute_schedule_request(
                "device-a",
                ApprovedScheduleRequest {
                    plan,
                    approval: PlanApproval {
                        review,
                        approved: true,
                    },
                },
            )
            .unwrap();
        assert_eq!(receipt.accepted_revision, 7);
        assert_eq!(receipt.schedule_block_ids.len(), 2);
        assert_eq!(host.snapshot().unwrap().0, 7);
        let blocks = host
            .application_queries()
            .schedule_between(0, 7_200_000)
            .unwrap();
        assert_eq!(blocks.len(), 2);
        assert_ne!(blocks[0].id(), blocks[1].id());
    }
    #[test]
    fn review_rejects_nonexistent_task_without_calendar_side_effect() {
        let host = authority_with_ready_task();
        let mut plan = wire_plan();
        plan.items[0].task_id = "nonexistent-task".into();
        assert!(host.review_schedule_request("device-a", &plan).is_err());
        assert_eq!(host.snapshot().unwrap().0, 3);
        assert!(
            host.application_queries()
                .schedule_between(0, 7_200_000)
                .unwrap()
                .is_empty()
        );
    }
    #[test]
    fn malformed_approval_json_cannot_bypass_confirmation() {
        let host = authority_with_ready_task();
        let plan = wire_plan();
        let review = host.review_schedule_request("device-a", &plan).unwrap();
        let payload = serde_json::json!({"plan":plan,"approval":{"review":review}});
        assert!(serde_json::from_value::<ApprovedScheduleRequest>(payload).is_err());
        assert_eq!(host.snapshot().unwrap().0, 3);
        assert!(
            host.application_queries()
                .schedule_between(0, 7_200_000)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn a_review_binds_calendar_time_not_just_task_identity() {
        let mut host = authority_with_ready_task();
        let plan = wire_plan();
        let review = host.review_schedule_request("device-a", &plan).unwrap();
        let mut retimed = plan;
        retimed.items[0].starts_at_ms = 600_000;
        retimed.items[0].ends_at_ms = 2_400_000;
        assert!(
            host.execute_schedule_request(
                "device-a",
                ApprovedScheduleRequest {
                    plan: retimed,
                    approval: PlanApproval {
                        review,
                        approved: true
                    },
                }
            )
            .is_err()
        );
        assert_eq!(host.snapshot().unwrap().0, 3);
    }

    #[test]
    fn existing_reservation_prevents_new_review_for_same_identity() {
        let mut host = authority_with_ready_task();
        let first = wire_plan();
        let review = host.review_schedule_request("device-a", &first).unwrap();
        host.execute_schedule_request(
            "device-a",
            ApprovedScheduleRequest {
                plan: first,
                approval: PlanApproval {
                    review,
                    approved: true,
                },
            },
        )
        .unwrap();
        let mut proposed = wire_plan();
        proposed.command_id = "different-command".into();
        proposed.expected_revision = 4;
        assert!(host.review_schedule_request("device-a", &proposed).is_err());
        assert_eq!(host.snapshot().unwrap().0, 4);
        assert_eq!(
            host.application_queries()
                .schedule_between(0, 7_200_000)
                .unwrap()
                .len(),
            1
        );
    }
    #[test]
    fn empty_calendar_review_does_not_generate_receipt() {
        let mut host = authority_with_ready_task();
        let mut request = wire_plan();
        request.items.clear();
        assert!(host.review_schedule_request("device-a", &request).is_err());
        let approval = PlanApproval {
            review: PlanReview {
                actor: "device-a".into(),
                command_id: request.command_id.clone(),
                expected_revision: 3,
                fingerprint: "fake".into(),
                scheduled_block_ids: vec![],
                scheduled_tasks: vec![],
                scheduled_slots: vec![],
                total_reserved_minutes: 0,
                requires_explicit_approval: true,
                creates_calendar_blocks: true,
            },
            approved: true,
        };
        assert!(
            host.execute_schedule_request(
                "device-a",
                ApprovedScheduleRequest {
                    plan: request,
                    approval,
                }
            )
            .is_err()
        );
        assert_eq!(host.snapshot().unwrap().0, 3);
    }

    #[test]
    fn duplicate_schedule_and_activity_identities_are_rejected() {
        use crate::domain_commands::{TaskAction, TaskCommand};
        let mut host = authority_with_ready_task();
        host.task_command(
            "device-a",
            TaskCommand {
                command_id: "second-task".into(),
                expected_revision: 3,
                action: TaskAction::Create {
                    task_id: "second-approved-task".into(),
                    title: "Another task".into(),
                    project_id: None,
                },
            },
        )
        .unwrap();
        host.task_command(
            "device-a",
            TaskCommand {
                command_id: "second-estimate".into(),
                expected_revision: 4,
                action: TaskAction::SetPlanning {
                    task_id: "second-approved-task".into(),
                    estimated_minutes: Some(30),
                    earliest_start_at_ms: None,
                    deadline_at_ms: None,
                },
            },
        )
        .unwrap();
        host.task_command(
            "device-a",
            TaskCommand {
                command_id: "second-ready".into(),
                expected_revision: 5,
                action: TaskAction::ChangeStatus {
                    task_id: "second-approved-task".into(),
                    status: "ready".into(),
                },
            },
        )
        .unwrap();
        let mut plan = wire_plan();
        plan.expected_revision = 6;
        let mut second = plan.items[0].clone();
        second.task_id = "second-approved-task".into();
        second.starts_at_ms = 1_800_000;
        second.ends_at_ms = 3_600_000;
        plan.items.push(second);
        assert!(host.review_schedule_request("device-a", &plan).is_err());
        plan.items[1].schedule_block_id = "second-block".into();
        assert!(host.review_schedule_request("device-a", &plan).is_err());
        plan.items[1].activity_id = "second-activity".into();
        assert!(host.review_schedule_request("device-a", &plan).is_ok());
    }

    #[test]
    fn wire_request_rejects_duplicate_task_and_overflow() {
        let host = authority_with_ready_task();
        let mut plan = wire_plan();
        plan.items.push(plan.items[0].clone());
        assert!(host.review_schedule_request("device-a", &plan).is_err());
        plan.items.pop();
        plan.items[0].starts_at_ms = i64::MIN;
        plan.items[0].ends_at_ms = i64::MAX;
        assert!(host.review_schedule_request("device-a", &plan).is_err());
    }

    fn plan() -> ApplyValidatedPlanCommand {
        use orbit_application::PlanApplicationIdentity;
        use orbit_domain::{
            ActivityId, ActivityOrigin, ScheduleBlockId, Task, TaskId, TaskPlanning, TaskScope,
            TaskStatus,
        };
        use orbit_planner::{
            CapacityPolicy, FixedBlockBufferPolicy, PlanningWindow, propose_tasks,
            validate_proposal,
        };
        let mut task = Task::new(
            TaskId::new("task-1").unwrap(),
            "Write spec",
            TaskScope::standalone(),
        )
        .unwrap();
        task.transition_to(TaskStatus::Ready).unwrap();
        task.set_planning(TaskPlanning::new(Some(30), None, None).unwrap());
        let window = PlanningWindow::new(0, 120 * 60_000).unwrap();
        let proposal = propose_tasks(window, &[], &[task.clone()], CapacityPolicy::default());
        let validated =
            validate_proposal(window, &[], &[task], CapacityPolicy::default(), proposal).unwrap();
        ApplyValidatedPlanCommand {
            window,
            policy: CapacityPolicy::default(),
            fixed_buffers: FixedBlockBufferPolicy::default(),
            proposal: validated,
            identities: vec![PlanApplicationIdentity {
                task_id: TaskId::new("task-1").unwrap(),
                schedule_block_id: ScheduleBlockId::new("reviewed-block").unwrap(),
                activity_id: ActivityId::new("reviewed-activity").unwrap(),
            }],
            origin: ActivityOrigin::Human,
        }
    }
    #[test]
    fn explicit_approval_is_mandatory() {
        let command = plan();
        let review = prepare_review("human", "cmd-1", 4, &command).unwrap();
        assert!(review.creates_calendar_blocks);
        assert_eq!(review.scheduled_tasks, vec!["task-1"]);
        let denied = PlanApproval {
            review: review.clone(),
            approved: false,
        };
        assert!(validate_approval(&denied, "human", "cmd-1", 4, &command).is_err());
        let allowed = PlanApproval {
            review,
            approved: true,
        };
        assert!(validate_approval(&allowed, "human", "cmd-1", 4, &command).is_ok());
    }
    #[test]
    fn approval_cannot_cross_actor_command_or_revision() {
        let command = plan();
        let approval = PlanApproval {
            review: prepare_review("alice", "c1", 3, &command).unwrap(),
            approved: true,
        };
        assert!(validate_approval(&approval, "bob", "c1", 3, &command).is_err());
        assert!(validate_approval(&approval, "alice", "c2", 3, &command).is_err());
        assert!(validate_approval(&approval, "alice", "c1", 4, &command).is_err());
    }
    #[test]
    fn review_detects_mutated_identities() {
        use orbit_domain::{ActivityId, ScheduleBlockId};
        let mut command = plan();
        let approval = PlanApproval {
            review: prepare_review("user", "cmd", 5, &command).unwrap(),
            approved: true,
        };
        command.identities[0].schedule_block_id = ScheduleBlockId::new("changed-block").unwrap();
        assert!(validate_approval(&approval, "user", "cmd", 5, &command).is_err());
        let mut command = plan();
        command.identities[0].activity_id = ActivityId::new("changed-activity").unwrap();
        assert!(validate_approval(&approval, "user", "cmd", 5, &command).is_err());
    }
    #[test]
    fn review_rejects_empty_or_repeated_plan() {
        let mut command = plan();
        command.identities.push(command.identities[0].clone());
        assert!(prepare_review("a", "b", 0, &command).is_err());
        let mut command = plan();
        command.identities.clear();
        assert!(prepare_review("a", "b", 0, &command).is_err());
        assert!(prepare_review(" ", "b", 0, &plan()).is_err());
        assert!(prepare_review("a", "", 0, &plan()).is_err());
        assert!(prepare_review("a", "b", -1, &plan()).is_err());
    }
    #[test]
    fn digest_is_stable_and_domain_separated() {
        let command = plan();
        let a = fingerprint("a", "bc", 3, &command).unwrap();
        assert_eq!(a, fingerprint("a", "bc", 3, &command).unwrap());
        assert_ne!(a, fingerprint("ab", "c", 3, &command).unwrap());
        assert_ne!(a, fingerprint("a", "bc", 4, &command).unwrap());
        assert_eq!(a.len(), 64);
    }
    #[test]
    fn fingerprint_separates_actor_command_and_revision() {
        // The approval hash uses domain-separated, length-delimited inputs.
        // A domain-command instance is supplied by the application-layer tests.
        assert!(valid_identity("human"));
        assert!(!valid_identity("  "));
        assert!(!valid_identity("bad\0id"));
    }
    #[test]
    fn approval_document_round_trip_preserves_deny_by_default() {
        let review = PlanReview {
            expected_revision: 7,
            actor: "human".into(),
            command_id: "cmd".into(),
            fingerprint: "fingerprint".into(),
            scheduled_block_ids: vec!["block".into()],
            scheduled_tasks: vec!["task".into()],
            scheduled_slots: vec![ReviewedScheduleSlot {
                task_id: "task".into(),
                schedule_block_id: "block".into(),
                starts_at_ms: 0,
                ends_at_ms: 60_000,
                duration_minutes: 1,
            }],
            total_reserved_minutes: 1,
            requires_explicit_approval: true,
            creates_calendar_blocks: true,
        };
        let document = PlanApproval {
            review,
            approved: false,
        };
        let serialized = serde_json::to_string(&document).unwrap();
        let parsed: PlanApproval = serde_json::from_str(&serialized).unwrap();
        assert_eq!(parsed, document);
        assert!(!parsed.approved);
    }
}
