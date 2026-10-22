//! Typed rich-application commands accepted through the single authority ledger.
//!
//! Each adapter supplies a durable command id and an expected global revision.
//! These operations persist domain rows, human-readable activities, semantic
//! history, replay projections, and retry receipts in the same SQL transaction.
//! Unlike standalone ApplicationServices, they can be safely retried after an
//! uncertain network response without repeating the application mutation.
use crate::{AuthoritativeHost, store::AuthorityError};
use orbit_application::{
    AcceptanceCriteriaCommand, ApplyValidatedHabitPlanCommand, ApplyValidatedPlanCommand,
    AssociateAttachmentCommand, ChangeProjectEvidenceCommand,
    ConfigureProjectStrategyWithActivityCommand, CreateHabitWithActivityCommand,
    CreateProjectWithActivityCommand, CreateRoutineWithActivityCommand,
    CreateScheduleBlockWithActivityCommand, CreateTaskWithActivityCommand, ImportAttachmentCommand,
    MetricObservation, MetricTargetRecord, RecordHabitOccurrenceWithActivityCommand,
    RemoveAttachmentCommand, RescheduleBlockWithActivityCommand, SetHabitActiveWithActivityCommand,
    SetProjectCriterionStatusWithActivityCommand, SetRoutineHabitMembershipWithActivityCommand,
    SetTaskPlanningWithActivityCommand, StartTaskWithActivityCommand,
    TransitionMilestoneWithActivityCommand, TransitionProjectWithActivityCommand,
    TransitionTaskWithActivityCommand, UpdateTaskDetailsCommand,
};
use orbit_domain::{AttachmentId, Habit, Project, ProjectId, Routine, Task};
use orbit_sync::{ChangeEnvelope, ChangeId, EntityKind, EntityRef, FieldMutation, SemanticChange};

fn field(name: &str, value: Option<String>) -> Result<FieldMutation, AuthorityError> {
    FieldMutation::new(name, value, None)
        .map_err(|_| AuthorityError::Invalid("invalid rich semantic field"))
}
fn semantic(
    kind: EntityKind,
    id: &str,
    fields: Vec<FieldMutation>,
) -> Result<SemanticChange, AuthorityError> {
    SemanticChange::new(
        EntityRef::new(kind, id).map_err(|_| AuthorityError::Invalid("invalid rich entity id"))?,
        fields,
    )
    .map_err(|_| AuthorityError::Invalid("invalid rich semantic change"))
}
fn command_payload<T: std::fmt::Debug>(command: &T) -> Result<String, AuthorityError> {
    // Full typed request (including activity identity and origin) is bound to
    // the immutable receipt. A retry changing only the activity is not equal.
    serde_json::to_string(&format!("{command:?}"))
        .map_err(|_| AuthorityError::Invalid("invalid rich command payload"))
}

/// Locate the latest recorded version of one semantic field. Without this
/// optimistic base, a second status update conflicts with the accepted replay.
/// The later admission revision check fences races between this lookup and the
/// transaction; the projection engine validates the field base on commit.
fn latest_field_version(
    host: &AuthoritativeHost,
    kind: EntityKind,
    id: &str,
    field_name: &str,
) -> Result<Option<ChangeId>, AuthorityError> {
    let mut stmt = host
        .authority
        .conn
        .prepare("SELECT wire FROM authority_history WHERE kind='change' ORDER BY cursor DESC")?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
    for row in rows {
        let envelope =
            ChangeEnvelope::from_wire_json(&row?).map_err(|_| AuthorityError::MigrationConflict)?;
        if envelope.change.entity.kind == kind
            && envelope.change.entity.id == id
            && envelope
                .change
                .mutations
                .iter()
                .any(|mutation| mutation.field == field_name)
        {
            return Ok(Some(envelope.id));
        }
    }
    Ok(None)
}

struct RichStateInput<'a> {
    actor: &'a str,
    command_id: &'a str,
    expected_revision: i64,
    kind: EntityKind,
    entity_id: &'a str,
    payload: String,
}

impl AuthoritativeHost {
    /// Admit a rich mutation using a monotonic semantic state version. Repeated
    /// updates to the same entity are ordered through the single authority
    /// ledger, even when the underlying application object is stored outside
    /// the minimal project/task projection.
    fn accept_rich_state(
        &mut self,
        input: RichStateInput<'_>,
        apply: impl FnOnce(
            &mut crate::application_services::ApplicationServices<'_>,
        ) -> Result<String, orbit_application::ApplicationError>,
    ) -> Result<(i64, bool, String), AuthorityError> {
        self.accept_rich_state_result(input, apply)
    }

    fn accept_rich_state_result<T>(
        &mut self,
        input: RichStateInput<'_>,
        apply: impl FnOnce(
            &mut crate::application_services::ApplicationServices<'_>,
        ) -> Result<T, orbit_application::ApplicationError>,
    ) -> Result<(i64, bool, T), AuthorityError>
    where
        T: serde::Serialize + serde::de::DeserializeOwned,
    {
        let RichStateInput {
            actor,
            command_id,
            expected_revision,
            kind,
            entity_id,
            payload,
        } = input;
        let base = latest_field_version(self, kind, entity_id, "rich_state")?;
        let change = semantic(
            kind,
            entity_id,
            vec![
                FieldMutation::new("rich_state", Some(payload.clone()), base)
                    .map_err(|_| AuthorityError::Invalid("invalid rich state mutation"))?,
            ],
        )?;
        self.execute_application_command(
            actor,
            command_id,
            expected_revision,
            &payload,
            change,
            apply,
        )
    }

    /// Canonical project creation using the rich application use case, not a
    /// second minimal project implementation. Duplicate delivery never creates
    /// a second activity record or changes the original revision.
    pub fn create_project_accepted(
        &mut self,
        actor: &str,
        command_id: &str,
        expected_revision: i64,
        command: CreateProjectWithActivityCommand,
    ) -> Result<(i64, bool, Project), AuthorityError> {
        let id = command.project.id.clone();
        let name = command.project.name.clone();
        let change = semantic(
            EntityKind::Project,
            id.as_str(),
            vec![
                field("name", Some(name))?,
                field("status", Some("proposed".into()))?,
            ],
        )?;
        let payload = command_payload(&command)?;
        let (revision, duplicate, result_id) = self.execute_application_command(
            actor,
            command_id,
            expected_revision,
            &payload,
            change,
            |app| {
                let project = app.create_project(command)?;
                Ok(project.id().as_str().to_owned())
            },
        )?;
        if result_id != id.as_str() {
            return Err(AuthorityError::MigrationConflict);
        }
        let project = self
            .application_queries()
            .project(&id)
            .map_err(|_| AuthorityError::MigrationConflict)?;
        Ok((revision, duplicate, project))
    }

    /// Explicitly change a project's pinned workflow.
    ///
    /// Resolution order: an explicit version wins; otherwise the bundled
    /// descriptor version; otherwise the latest published revision. Bundled
    /// manifests are installed on demand (never overwriting a published
    /// revision) so a fresh offline workspace can switch workflows. Unknown
    /// projects and unresolvable workflows are rejected without creating a
    /// project or rebinding anything; a duplicate command id returns the
    /// original receipt and never applies the change twice.
    pub fn set_project_workflow_accepted(
        &mut self,
        actor: &str,
        command_id: &str,
        expected_revision: i64,
        project_id: ProjectId,
        blueprint_id: String,
        version: Option<u32>,
    ) -> Result<(i64, bool, String), AuthorityError> {
        let bundled_version = crate::default_workflow::bundled_version(&blueprint_id);
        let version = match version {
            Some(version) => version,
            None => match bundled_version {
                Some(version) => version,
                None => self
                    .authority
                    .conn
                    .latest_blueprint_version(&blueprint_id)
                    .map_err(|_| AuthorityError::MigrationConflict)?
                    .ok_or(AuthorityError::Invalid("unknown workflow"))?,
            },
        };
        if bundled_version.is_some() {
            crate::default_workflow::install_bundled_workflow(&self.authority.conn, &blueprint_id)?;
        }
        let project_exists: bool = self.authority.conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM projects WHERE id=?1)",
            [project_id.as_str()],
            |row| row.get(0),
        )?;
        if !project_exists {
            return Err(AuthorityError::Invalid("unknown project"));
        }
        if self
            .authority
            .conn
            .load_blueprint(&blueprint_id, version)
            .map_err(|_| AuthorityError::MigrationConflict)?
            .is_none()
        {
            return Err(AuthorityError::Invalid("unknown workflow"));
        }
        let entity_id = project_id.as_str().to_owned();
        let payload = serde_json::to_string(&(entity_id.as_str(), blueprint_id.as_str(), version))
            .map_err(|_| AuthorityError::Invalid("invalid workflow command"))?;
        self.accept_rich_state(
            RichStateInput {
                actor,
                command_id,
                expected_revision,
                kind: EntityKind::Project,
                entity_id: &entity_id,
                payload,
            },
            move |app| {
                app.replace_project_workflow(&project_id, &blueprint_id, version)?;
                Ok(project_id.as_str().to_owned())
            },
        )
    }

    /// Explicitly unpin a project's workflow so it falls back to generated
    /// default views. The project itself is never modified or deleted.
    pub fn remove_project_workflow_accepted(
        &mut self,
        actor: &str,
        command_id: &str,
        expected_revision: i64,
        project_id: ProjectId,
    ) -> Result<(i64, bool, bool), AuthorityError> {
        let entity_id = project_id.as_str().to_owned();
        let payload = serde_json::to_string(&("unpin", entity_id.as_str()))
            .map_err(|_| AuthorityError::Invalid("invalid workflow command"))?;
        self.accept_rich_state_result(
            RichStateInput {
                actor,
                command_id,
                expected_revision,
                kind: EntityKind::Project,
                entity_id: &entity_id,
                payload,
            },
            move |app| app.remove_project_workflow(&project_id),
        )
    }

    /// Canonical task creation, including optional project/milestone placement.
    /// Rejected scope checks or activity uniqueness violations roll back both
    /// the application rows and the authority command receipt.
    pub fn create_task_accepted(
        &mut self,
        actor: &str,
        command_id: &str,
        expected_revision: i64,
        command: CreateTaskWithActivityCommand,
    ) -> Result<(i64, bool, Task), AuthorityError> {
        let id = command.task.id.clone();
        let scope = command.task.scope.clone();
        let change = semantic(
            EntityKind::Task,
            id.as_str(),
            vec![
                field("title", Some(command.task.title.clone()))?,
                field("status", Some("inbox".into()))?,
                field("project_id", scope.project_id().map(ToString::to_string))?,
                field(
                    "milestone_id",
                    scope.milestone_id().map(ToString::to_string),
                )?,
            ],
        )?;
        let payload = command_payload(&command)?;
        let (revision, duplicate, result_id) = self.execute_application_command(
            actor,
            command_id,
            expected_revision,
            &payload,
            change,
            |app| {
                let task = app.create_task(command)?;
                Ok(task.id().as_str().to_owned())
            },
        )?;
        if result_id != id.as_str() {
            return Err(AuthorityError::MigrationConflict);
        }
        let task = self
            .application_queries()
            .task(&id)
            .map_err(|_| AuthorityError::MigrationConflict)?;
        Ok((revision, duplicate, task))
    }

    /// Versioned project status transition. The semantic event uses the
    /// previously accepted status field as its optimistic base version.
    pub fn transition_project_accepted(
        &mut self,
        actor: &str,
        command_id: &str,
        expected_revision: i64,
        command: TransitionProjectWithActivityCommand,
    ) -> Result<(i64, bool, Project), AuthorityError> {
        let id = command.project.id.clone();
        let base = latest_field_version(self, EntityKind::Project, id.as_str(), "status")?;
        let change = semantic(
            EntityKind::Project,
            id.as_str(),
            vec![
                FieldMutation::new("status", Some(command.project.status.as_str().into()), base)
                    .map_err(|_| AuthorityError::Invalid("invalid project status"))?,
            ],
        )?;
        let payload = command_payload(&command)?;
        let (revision, duplicate, result_id) = self.execute_application_command(
            actor,
            command_id,
            expected_revision,
            &payload,
            change,
            |app| {
                let project = app.transition_project(command)?;
                Ok(project.id().as_str().to_owned())
            },
        )?;
        if result_id != id.as_str() {
            return Err(AuthorityError::MigrationConflict);
        }
        let project = self
            .application_queries()
            .project(&id)
            .map_err(|_| AuthorityError::MigrationConflict)?;
        Ok((revision, duplicate, project))
    }

    /// Versioned task state transition using the same rich domain validation
    /// and activity creation as the local application service.
    pub fn transition_task_accepted(
        &mut self,
        actor: &str,
        command_id: &str,
        expected_revision: i64,
        command: TransitionTaskWithActivityCommand,
    ) -> Result<(i64, bool, Task), AuthorityError> {
        let id = command.task.id.clone();
        let base = latest_field_version(self, EntityKind::Task, id.as_str(), "status")?;
        let change = semantic(
            EntityKind::Task,
            id.as_str(),
            vec![
                FieldMutation::new("status", Some(command.task.status.as_str().into()), base)
                    .map_err(|_| AuthorityError::Invalid("invalid task status"))?,
            ],
        )?;
        let payload = command_payload(&command)?;
        let (revision, duplicate, result_id) = self.execute_application_command(
            actor,
            command_id,
            expected_revision,
            &payload,
            change,
            |app| {
                let task = app.transition_task(command)?;
                Ok(task.id().as_str().to_owned())
            },
        )?;
        if result_id != id.as_str() {
            return Err(AuthorityError::MigrationConflict);
        }
        let task = self
            .application_queries()
            .task(&id)
            .map_err(|_| AuthorityError::MigrationConflict)?;
        Ok((revision, duplicate, task))
    }

    /// Start-task operation preserves its distinct activity type while still
    /// updating the same versioned semantic status field.
    pub fn start_task_accepted(
        &mut self,
        actor: &str,
        command_id: &str,
        expected_revision: i64,
        command: StartTaskWithActivityCommand,
    ) -> Result<(i64, bool, Task), AuthorityError> {
        let id = command.id.clone();
        let base = latest_field_version(self, EntityKind::Task, id.as_str(), "status")?;
        let change = semantic(
            EntityKind::Task,
            id.as_str(),
            vec![
                FieldMutation::new("status", Some("in_progress".into()), base)
                    .map_err(|_| AuthorityError::Invalid("invalid task start status"))?,
            ],
        )?;
        let payload = command_payload(&command)?;
        let (revision, duplicate, result_id) = self.execute_application_command(
            actor,
            command_id,
            expected_revision,
            &payload,
            change,
            |app| {
                let task = app.start_task(command)?;
                Ok(task.id().as_str().to_owned())
            },
        )?;
        if result_id != id.as_str() {
            return Err(AuthorityError::MigrationConflict);
        }
        let task = self
            .application_queries()
            .task(&id)
            .map_err(|_| AuthorityError::MigrationConflict)?;
        Ok((revision, duplicate, task))
    }

    /// Canonical routine creation with exactly-once activity and a retriable
    /// command receipt. The local application service remains the only owner
    /// of the routine business validation.
    pub fn create_routine_accepted(
        &mut self,
        actor: &str,
        command_id: &str,
        expected_revision: i64,
        command: CreateRoutineWithActivityCommand,
    ) -> Result<(i64, bool, Routine), AuthorityError> {
        let id = command.id.clone();
        let change = semantic(
            EntityKind::Routine,
            id.as_str(),
            vec![field("name", Some(command.name.clone()))?],
        )?;
        let payload = command_payload(&command)?;
        let (revision, duplicate, result_id) = self.execute_application_command(
            actor,
            command_id,
            expected_revision,
            &payload,
            change,
            |app| {
                let routine = app.create_routine(command)?;
                Ok(routine.id().as_str().to_owned())
            },
        )?;
        if result_id != id.as_str() {
            return Err(AuthorityError::MigrationConflict);
        }
        let routine = self
            .application_queries()
            .routines()
            .map_err(|_| AuthorityError::MigrationConflict)?
            .into_iter()
            .find(|routine| routine.id() == &id)
            .ok_or(AuthorityError::MigrationConflict)?;
        Ok((revision, duplicate, routine))
    }

    /// Canonical habit creation. Habits are included in the same global
    /// revision sequence and semantic replay as projects and tasks.
    pub fn create_habit_accepted(
        &mut self,
        actor: &str,
        command_id: &str,
        expected_revision: i64,
        command: CreateHabitWithActivityCommand,
    ) -> Result<(i64, bool, Habit), AuthorityError> {
        let id = command.habit.id.clone();
        let change = semantic(
            EntityKind::Habit,
            id.as_str(),
            vec![
                field("name", Some(command.habit.name.clone()))?,
                field(
                    "cadence",
                    Some(match command.habit.cadence {
                        orbit_domain::HabitCadence::Daily => "daily".to_owned(),
                        orbit_domain::HabitCadence::Weekly { target_per_week } => {
                            format!("weekly:{target_per_week}")
                        }
                    }),
                )?,
            ],
        )?;
        let payload = command_payload(&command)?;
        let (revision, duplicate, result_id) = self.execute_application_command(
            actor,
            command_id,
            expected_revision,
            &payload,
            change,
            |app| {
                let habit = app.create_habit(command)?;
                Ok(habit.id().as_str().to_owned())
            },
        )?;
        if result_id != id.as_str() {
            return Err(AuthorityError::MigrationConflict);
        }
        let habit = self
            .application_queries()
            .habits()
            .map_err(|_| AuthorityError::MigrationConflict)?
            .into_iter()
            .find(|habit| habit.id() == &id)
            .ok_or(AuthorityError::MigrationConflict)?;
        Ok((revision, duplicate, habit))
    }
    /// Canonical, retriable set task planning preserving application activity.
    pub fn set_task_planning_accepted(
        &mut self,
        actor: &str,
        command_id: &str,
        expected_revision: i64,
        command: SetTaskPlanningWithActivityCommand,
    ) -> Result<(i64, bool, String), AuthorityError> {
        let id = command.task.id.as_str().to_owned();
        let payload = command_payload(&command)?;
        self.accept_rich_state(
            RichStateInput {
                actor,
                command_id,
                expected_revision,
                kind: EntityKind::Task,
                entity_id: &id,
                payload,
            },
            |app| {
                let result = app.set_task_planning(command)?;
                Ok(result.id().as_str().to_owned())
            },
        )
    }

    /// Canonical, retriable configure project strategy preserving application activity.
    pub fn configure_project_strategy_accepted(
        &mut self,
        actor: &str,
        command_id: &str,
        expected_revision: i64,
        command: ConfigureProjectStrategyWithActivityCommand,
    ) -> Result<(i64, bool, String), AuthorityError> {
        let id = command.project.id.as_str().to_owned();
        let payload = command_payload(&command)?;
        self.accept_rich_state(
            RichStateInput {
                actor,
                command_id,
                expected_revision,
                kind: EntityKind::Project,
                entity_id: &id,
                payload,
            },
            |app| {
                let result = app.configure_project_strategy(command)?;
                Ok(result.id().as_str().to_owned())
            },
        )
    }

    /// Canonical, retriable set project criterion status preserving application activity.
    pub fn set_project_criterion_status_accepted(
        &mut self,
        actor: &str,
        command_id: &str,
        expected_revision: i64,
        command: SetProjectCriterionStatusWithActivityCommand,
    ) -> Result<(i64, bool, String), AuthorityError> {
        let id = command.project_id.as_str().to_owned();
        let payload = command_payload(&command)?;
        self.accept_rich_state(
            RichStateInput {
                actor,
                command_id,
                expected_revision,
                kind: EntityKind::Project,
                entity_id: &id,
                payload,
            },
            |app| {
                let result = app.set_project_criterion_status(command)?;
                Ok(result.id().as_str().to_owned())
            },
        )
    }

    /// Canonical, retriable set habit active preserving application activity.
    pub fn set_habit_active_accepted(
        &mut self,
        actor: &str,
        command_id: &str,
        expected_revision: i64,
        command: SetHabitActiveWithActivityCommand,
    ) -> Result<(i64, bool, String), AuthorityError> {
        let id = command.id.as_str().to_owned();
        let payload = command_payload(&command)?;
        self.accept_rich_state(
            RichStateInput {
                actor,
                command_id,
                expected_revision,
                kind: EntityKind::Habit,
                entity_id: &id,
                payload,
            },
            |app| {
                let result = app.set_habit_active(command)?;
                Ok(result.id().as_str().to_owned())
            },
        )
    }

    /// Canonical, retriable record habit occurrence preserving application activity.
    pub fn record_habit_occurrence_accepted(
        &mut self,
        actor: &str,
        command_id: &str,
        expected_revision: i64,
        command: RecordHabitOccurrenceWithActivityCommand,
    ) -> Result<(i64, bool, String), AuthorityError> {
        let id = command.id.as_str().to_owned();
        let payload = command_payload(&command)?;
        self.accept_rich_state(
            RichStateInput {
                actor,
                command_id,
                expected_revision,
                kind: EntityKind::HabitOccurrence,
                entity_id: &id,
                payload,
            },
            |app| {
                let result = app.record_habit_occurrence(command)?;
                Ok(result.id().as_str().to_owned())
            },
        )
    }

    /// Canonical, retriable set routine habit membership preserving application activity.
    pub fn set_routine_habit_membership_accepted(
        &mut self,
        actor: &str,
        command_id: &str,
        expected_revision: i64,
        command: SetRoutineHabitMembershipWithActivityCommand,
    ) -> Result<(i64, bool, String), AuthorityError> {
        let id = command.routine_id.as_str().to_owned();
        let payload = command_payload(&command)?;
        self.accept_rich_state(
            RichStateInput {
                actor,
                command_id,
                expected_revision,
                kind: EntityKind::Routine,
                entity_id: &id,
                payload,
            },
            |app| {
                let result = app.set_routine_habit_membership(command)?;
                Ok(result.id().as_str().to_owned())
            },
        )
    }

    /// Atomically accept a multi-block task plan. The semantic record is a plan
    /// manifest containing *every* intended schedule ID. No fake schedule-block
    /// entity is inserted into the authority history. Domain validation runs in
    /// the same transaction and must create exactly these block identities.
    pub fn apply_task_plan_accepted(
        &mut self,
        actor: &str,
        command_id: &str,
        expected_revision: i64,
        command: ApplyValidatedPlanCommand,
    ) -> Result<(i64, bool, Vec<String>), AuthorityError> {
        let ids: Vec<String> = command
            .identities
            .iter()
            .map(|identity| identity.schedule_block_id.to_string())
            .collect();
        let manifest = serde_json::to_string(&ids)
            .map_err(|_| AuthorityError::Invalid("invalid task plan manifest"))?;
        let change = semantic(
            EntityKind::PlanApplication,
            command_id,
            vec![
                field("kind", Some("task".into()))?,
                field("schedule_block_ids", Some(manifest))?,
            ],
        )?;
        let payload = command_payload(&command)?;
        self.execute_application_command(
            actor,
            command_id,
            expected_revision,
            &payload,
            change,
            move |app| {
                let created = app.apply_task_plan(command)?;
                let actual: Vec<String> = created.iter().map(|b| b.id().to_string()).collect();
                if actual.len() != ids.len()
                    || actual.iter().collect::<std::collections::BTreeSet<_>>()
                        != ids.iter().collect::<std::collections::BTreeSet<_>>()
                {
                    return Err(orbit_application::ApplicationError::InconsistentReference {
                        relation: "task_plan_receipt_manifest",
                    });
                }
                Ok(actual)
            },
        )
    }

    /// Habit-plan counterpart, using the same global command revision, receipt,
    /// atomic schedule/activity transaction, and multi-block semantic manifest.
    pub fn apply_habit_plan_accepted(
        &mut self,
        actor: &str,
        command_id: &str,
        expected_revision: i64,
        command: ApplyValidatedHabitPlanCommand,
    ) -> Result<(i64, bool, Vec<String>), AuthorityError> {
        let ids: Vec<String> = command
            .identities
            .iter()
            .map(|identity| identity.schedule_block_id.to_string())
            .collect();
        let manifest = serde_json::to_string(&ids)
            .map_err(|_| AuthorityError::Invalid("invalid habit plan manifest"))?;
        let change = semantic(
            EntityKind::PlanApplication,
            command_id,
            vec![
                field("kind", Some("habit".into()))?,
                field("schedule_block_ids", Some(manifest))?,
            ],
        )?;
        let payload = command_payload(&command)?;
        self.execute_application_command(
            actor,
            command_id,
            expected_revision,
            &payload,
            change,
            move |app| {
                let created = app.apply_habit_plan(command)?;
                let actual: Vec<String> = created.iter().map(|b| b.id().to_string()).collect();
                if actual.len() != ids.len()
                    || actual.iter().collect::<std::collections::BTreeSet<_>>()
                        != ids.iter().collect::<std::collections::BTreeSet<_>>()
                {
                    return Err(orbit_application::ApplicationError::InconsistentReference {
                        relation: "habit_plan_receipt_manifest",
                    });
                }
                Ok(actual)
            },
        )
    }

    /// Canonical, retriable create schedule block preserving application activity.
    pub fn create_schedule_block_accepted(
        &mut self,
        actor: &str,
        command_id: &str,
        expected_revision: i64,
        command: CreateScheduleBlockWithActivityCommand,
    ) -> Result<(i64, bool, String), AuthorityError> {
        let id = command.block.id.as_str().to_owned();
        let payload = command_payload(&command)?;
        self.accept_rich_state(
            RichStateInput {
                actor,
                command_id,
                expected_revision,
                kind: EntityKind::ScheduleBlock,
                entity_id: &id,
                payload,
            },
            |app| {
                let result = app.create_schedule_block(command)?;
                Ok(result.id().as_str().to_owned())
            },
        )
    }

    /// Canonical, retriable reschedule block preserving application activity.
    pub fn reschedule_block_accepted(
        &mut self,
        actor: &str,
        command_id: &str,
        expected_revision: i64,
        command: RescheduleBlockWithActivityCommand,
    ) -> Result<(i64, bool, String), AuthorityError> {
        let id = command.id.as_str().to_owned();
        let payload = command_payload(&command)?;
        self.accept_rich_state(
            RichStateInput {
                actor,
                command_id,
                expected_revision,
                kind: EntityKind::ScheduleBlock,
                entity_id: &id,
                payload,
            },
            |app| {
                let result = app.reschedule_block(command)?;
                Ok(result.id().as_str().to_owned())
            },
        )
    }

    /// Canonical, retriable transition milestone preserving application activity.
    pub fn transition_milestone_accepted(
        &mut self,
        actor: &str,
        command_id: &str,
        expected_revision: i64,
        command: TransitionMilestoneWithActivityCommand,
    ) -> Result<(i64, bool, String), AuthorityError> {
        let id = command.milestone.id.as_str().to_owned();
        let payload = command_payload(&command)?;
        self.accept_rich_state(
            RichStateInput {
                actor,
                command_id,
                expected_revision,
                kind: EntityKind::Milestone,
                entity_id: &id,
                payload,
            },
            |app| {
                let result = app.transition_milestone(command)?;
                Ok(result.id().as_str().to_owned())
            },
        )
    }

    /// Record a metric target through the same durable command receipt as
    /// project/task changes; metric state is fenced by global revision.
    pub fn set_metric_target_accepted(
        &mut self,
        actor: &str,
        command_id: &str,
        expected_revision: i64,
        command: MetricTargetRecord,
    ) -> Result<(i64, bool, String), AuthorityError> {
        let id = command.metric_id.clone();
        let payload = command_payload(&command)?;
        self.accept_rich_state(
            RichStateInput {
                actor,
                command_id,
                expected_revision,
                kind: EntityKind::Metric,
                entity_id: &id,
                payload,
            },
            |app| {
                app.set_metric_target(command)?;
                Ok(id.clone())
            },
        )
    }

    /// Metric observations are append-only application records. Replaying the
    /// same command id never appends the same observation a second time.
    pub fn record_metric_observation_accepted(
        &mut self,
        actor: &str,
        command_id: &str,
        expected_revision: i64,
        command: MetricObservation,
    ) -> Result<(i64, bool, String), AuthorityError> {
        let id = command.metric_id.clone();
        let payload = command_payload(&command)?;
        self.accept_rich_state(
            RichStateInput {
                actor,
                command_id,
                expected_revision,
                kind: EntityKind::Metric,
                entity_id: &id,
                payload,
            },
            |app| {
                app.record_metric_observation(command)?;
                Ok(id.clone())
            },
        )
    }

    /// Project evidence binding/unbinding shares canonical command admission.
    /// The `bind` flag is part of the immutable payload identity.
    pub fn change_project_evidence_accepted(
        &mut self,
        actor: &str,
        command_id: &str,
        expected_revision: i64,
        bind: bool,
        command: ChangeProjectEvidenceCommand,
    ) -> Result<(i64, bool, String), AuthorityError> {
        let id = command.project_id.as_str().to_owned();
        let payload = command_payload(&(bind, &command))?;
        self.accept_rich_state(
            RichStateInput {
                actor,
                command_id,
                expected_revision,
                kind: EntityKind::Project,
                entity_id: &id,
                payload,
            },
            |app| {
                app.change_project_evidence(bind, command)?;
                Ok(id.clone())
            },
        )
    }

    /// Canonical, retriable rich task-detail update.
    ///
    /// The command carries its activity identity, so the immutable payload
    /// prevents a duplicate command id from appending a second activity even
    /// though the application layer would otherwise consider the update a
    /// no-op and skip writing.
    pub fn update_task_details_accepted(
        &mut self,
        actor: &str,
        command_id: &str,
        expected_revision: i64,
        command: UpdateTaskDetailsCommand,
    ) -> Result<(i64, bool, String), AuthorityError> {
        let id = command.task_id.as_str().to_owned();
        let payload = command_payload(&command)?;
        self.accept_rich_state(
            RichStateInput {
                actor,
                command_id,
                expected_revision,
                kind: EntityKind::Task,
                entity_id: &id,
                payload,
            },
            |app| {
                let task = app.update_task_details(command)?;
                Ok(task.id().as_str().to_owned())
            },
        )
    }

    /// Canonical, retriable ordered acceptance-criteria change. The whole
    /// ordered list is replaced atomically inside the command transaction.
    pub fn apply_acceptance_criteria_accepted(
        &mut self,
        actor: &str,
        command_id: &str,
        expected_revision: i64,
        command: AcceptanceCriteriaCommand,
    ) -> Result<(i64, bool, String), AuthorityError> {
        let id = command.task_id.as_str().to_owned();
        let payload = command_payload(&command)?;
        self.accept_rich_state(
            RichStateInput {
                actor,
                command_id,
                expected_revision,
                kind: EntityKind::Task,
                entity_id: &id,
                payload,
            },
            |app| {
                app.apply_acceptance_criteria(command)?;
                Ok(id.clone())
            },
        )
    }

    /// Canonical, retriable attachment import.
    ///
    /// Content bytes are written to the content-addressed object store before
    /// the metadata transaction runs. If that transaction rolls back the object
    /// remains as a harmless, deduplicated orphan that a later purge removes;
    /// a retried command returns its original receipt and never creates a
    /// second attachment row or activity.
    pub fn import_attachment_accepted(
        &mut self,
        actor: &str,
        command_id: &str,
        expected_revision: i64,
        command: ImportAttachmentCommand,
    ) -> Result<(i64, bool, AttachmentId), AuthorityError> {
        let id = command.id.clone();
        let payload = command_payload(&command)?;
        let (revision, duplicate, result_id) = self.accept_rich_state(
            RichStateInput {
                actor,
                command_id,
                expected_revision,
                kind: EntityKind::Attachment,
                entity_id: id.as_str(),
                payload,
            },
            |app| {
                let attachment = app.import_attachment(command)?;
                Ok(attachment.id().as_str().to_owned())
            },
        )?;
        if result_id != id.as_str() {
            return Err(AuthorityError::MigrationConflict);
        }
        Ok((revision, duplicate, id))
    }

    /// Canonical, retriable attachment removal. The returned boolean records
    /// whether the shared content object was also removed.
    pub fn remove_attachment_accepted(
        &mut self,
        actor: &str,
        command_id: &str,
        expected_revision: i64,
        command: RemoveAttachmentCommand,
    ) -> Result<(i64, bool, bool), AuthorityError> {
        let id = command.id.as_str().to_owned();
        let payload = command_payload(&command)?;
        self.accept_rich_state_result(
            RichStateInput {
                actor,
                command_id,
                expected_revision,
                kind: EntityKind::Attachment,
                entity_id: &id,
                payload,
            },
            |app| {
                let removal = app.remove_attachment(command)?;
                Ok(removal.content_removed)
            },
        )
    }

    /// Canonical, retriable association of existing deduplicated content with
    /// another owner. Both associations share one immutable object.
    pub fn associate_attachment_accepted(
        &mut self,
        actor: &str,
        command_id: &str,
        expected_revision: i64,
        command: AssociateAttachmentCommand,
    ) -> Result<(i64, bool, AttachmentId), AuthorityError> {
        let id = command.new_id.clone();
        let payload = command_payload(&command)?;
        let (revision, duplicate, result_id) = self.accept_rich_state(
            RichStateInput {
                actor,
                command_id,
                expected_revision,
                kind: EntityKind::Attachment,
                entity_id: id.as_str(),
                payload,
            },
            |app| {
                let attachment = app.associate_attachment(command)?;
                Ok(attachment.id().as_str().to_owned())
            },
        )?;
        if result_id != id.as_str() {
            return Err(AuthorityError::MigrationConflict);
        }
        Ok((revision, duplicate, id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use orbit_application::{CreateHabitCommand, CreateProjectCommand, CreateTaskCommand};
    use orbit_domain::{
        ActivityId, ActivityOrigin, HabitCadence, HabitId, ProjectId, RoutineId, TaskId, TaskScope,
    };

    fn project(id: &str, activity: &str) -> CreateProjectWithActivityCommand {
        CreateProjectWithActivityCommand {
            project: CreateProjectCommand {
                id: ProjectId::new(id).unwrap(),
                name: format!("Project {id}"),
            },
            activity_id: ActivityId::new(activity).unwrap(),
            origin: ActivityOrigin::Human,
            summary: format!("Created project {id}"),
        }
    }
    fn task(id: &str, project_id: &str, activity: &str) -> CreateTaskWithActivityCommand {
        CreateTaskWithActivityCommand {
            task: CreateTaskCommand {
                id: TaskId::new(id).unwrap(),
                title: format!("Task {id}"),
                scope: TaskScope::in_project(ProjectId::new(project_id).unwrap()),
            },
            activity_id: ActivityId::new(activity).unwrap(),
            origin: ActivityOrigin::Human,
            summary: format!("Created task {id}"),
        }
    }
    fn habit(id: &str, activity: &str) -> CreateHabitWithActivityCommand {
        CreateHabitWithActivityCommand {
            habit: CreateHabitCommand {
                id: HabitId::new(id).unwrap(),
                name: format!("Habit {id}"),
                cadence: HabitCadence::Daily,
            },
            activity_id: ActivityId::new(activity).unwrap(),
            origin: ActivityOrigin::Human,
            summary: format!("Created habit {id}"),
        }
    }

    fn routine(id: &str, activity: &str) -> CreateRoutineWithActivityCommand {
        CreateRoutineWithActivityCommand {
            id: RoutineId::new(id).unwrap(),
            name: format!("Routine {id}"),
            activity_id: ActivityId::new(activity).unwrap(),
            origin: ActivityOrigin::Human,
            summary: format!("Created routine {id}"),
        }
    }

    #[test]
    fn mixed_minimal_and_rich_commands_use_one_sequence_and_conflict_policy() {
        use crate::domain_commands::ProjectCommand;
        let mut host = AuthoritativeHost::open(":memory:").unwrap();
        // A canonical minimal command comes first, followed by rich use cases.
        let minimal = host
            .project_command(
                "device",
                ProjectCommand {
                    workflow_id: None,
                    command_id: "minimal-p".into(),
                    expected_revision: 0,
                    project_id: "minimal".into(),
                    name: "Minimal project".into(),
                },
            )
            .unwrap();
        assert_eq!(minimal.revision, 1);
        let (revision, _, _) = host
            .create_project_accepted("user", "rich-p", 1, project("rich", "activity-rich"))
            .unwrap();
        assert_eq!(revision, 2);
        let (revision, _, _) = host
            .create_task_accepted(
                "user",
                "rich-t",
                2,
                task("rich-task", "rich", "activity-task"),
            )
            .unwrap();
        assert_eq!(revision, 3);
        // A canonical command cannot reuse a rich command's immutable ID.
        let reused = host.project_command(
            "user",
            ProjectCommand {
                workflow_id: None,
                command_id: "rich-p".into(),
                expected_revision: 3,
                project_id: "other".into(),
                name: "Not permitted".into(),
            },
        );
        assert!(matches!(reused, Err(AuthorityError::ImmutableConflict)));
        assert_eq!(host.snapshot().unwrap().0, 3);
        assert_eq!(host.application_queries().projects().unwrap().len(), 2);
        assert_eq!(host.application_queries().activities().unwrap().len(), 2);
    }

    #[test]
    fn stale_rich_create_does_not_commit_activity_or_advance_history() {
        let mut host = AuthoritativeHost::open(":memory:").unwrap();
        host.create_project_accepted("user", "p", 0, project("p", "activity-p"))
            .unwrap();
        let stale = host.create_routine_accepted(
            "user",
            "stale",
            0,
            routine("morning", "activity-morning"),
        );
        assert!(matches!(stale, Err(AuthorityError::StaleRevision)));
        assert!(host.application_queries().routines().unwrap().is_empty());
        assert_eq!(host.application_queries().activities().unwrap().len(), 1);
        assert_eq!(host.snapshot().unwrap().0, 1);
    }

    #[test]
    fn rich_routine_create_and_retry_participates_in_global_revision() {
        let mut host = AuthoritativeHost::open(":memory:").unwrap();
        host.create_project_accepted("user", "p", 0, project("p", "activity-p"))
            .unwrap();
        let (revision, duplicate, _) = host
            .create_routine_accepted("user", "routine", 1, routine("morning", "activity-r"))
            .unwrap();
        assert_eq!(revision, 2);
        assert!(!duplicate);
        let (_, duplicate, _) = host
            .create_routine_accepted("user", "routine", 1, routine("morning", "activity-r"))
            .unwrap();
        assert!(duplicate);
        assert_eq!(host.snapshot().unwrap().0, 2);
        assert_eq!(host.application_queries().routines().unwrap().len(), 1);
        assert_eq!(host.application_queries().activities().unwrap().len(), 2);
    }

    #[test]
    fn repeated_project_status_changes_follow_semantic_field_versions() {
        use orbit_application::{TransitionProjectCommand, TransitionProjectWithActivityCommand};
        use orbit_domain::ProjectStatus;
        let mut host = AuthoritativeHost::open(":memory:").unwrap();
        host.create_project_accepted("user", "p", 0, project("p", "activity-p"))
            .unwrap();
        for (revision, next_status) in [
            (1, ProjectStatus::Active),
            (2, ProjectStatus::OnHold),
            (3, ProjectStatus::Active),
            (4, ProjectStatus::Completed),
        ] {
            let command = TransitionProjectWithActivityCommand {
                project: TransitionProjectCommand {
                    id: ProjectId::new("p").unwrap(),
                    status: next_status,
                },
                activity_id: ActivityId::new(format!("status-{revision}")).unwrap(),
                origin: ActivityOrigin::Human,
                summary: format!("Transition to {}", next_status.as_str()),
            };
            let (accepted, duplicate, project) = host
                .transition_project_accepted(
                    "user",
                    &format!("transition-{revision}"),
                    revision,
                    command,
                )
                .unwrap();
            assert_eq!(accepted, revision + 1);
            assert!(!duplicate);
            assert_eq!(project.status(), next_status);
        }
        assert_eq!(host.snapshot().unwrap().0, 5);
        assert_eq!(host.application_queries().activities().unwrap().len(), 5);
    }

    #[test]
    fn task_start_and_status_transitions_share_canonical_revision() {
        use orbit_application::{
            StartTaskWithActivityCommand, TransitionTaskCommand, TransitionTaskWithActivityCommand,
        };
        use orbit_domain::TaskStatus;
        let mut host = AuthoritativeHost::open(":memory:").unwrap();
        host.create_project_accepted("user", "p", 0, project("p", "activity-p"))
            .unwrap();
        host.create_task_accepted("user", "t", 1, task("t", "p", "activity-t"))
            .unwrap();
        // Inbox -> Planned -> InProgress -> Done must preserve semantic field versions.
        let planned = TransitionTaskWithActivityCommand {
            task: TransitionTaskCommand {
                id: TaskId::new("t").unwrap(),
                status: TaskStatus::Planned,
            },
            activity_id: ActivityId::new("activity-planned").unwrap(),
            origin: ActivityOrigin::Human,
            summary: "planned".into(),
        };
        let (r3, _, _) = host
            .transition_task_accepted("user", "plan", 2, planned)
            .unwrap();
        assert_eq!(r3, 3);
        let (r4, _, started) = host
            .start_task_accepted(
                "user",
                "start",
                3,
                StartTaskWithActivityCommand {
                    id: TaskId::new("t").unwrap(),
                    activity_id: ActivityId::new("activity-start").unwrap(),
                    origin: ActivityOrigin::Human,
                    summary: "started".into(),
                },
            )
            .unwrap();
        assert_eq!(r4, 4);
        assert_eq!(started.status(), TaskStatus::InProgress);
        let (r5, _, completed) = host
            .transition_task_accepted(
                "user",
                "complete",
                4,
                TransitionTaskWithActivityCommand {
                    task: TransitionTaskCommand {
                        id: TaskId::new("t").unwrap(),
                        status: TaskStatus::Done,
                    },
                    activity_id: ActivityId::new("activity-done").unwrap(),
                    origin: ActivityOrigin::Human,
                    summary: "completed".into(),
                },
            )
            .unwrap();
        assert_eq!(r5, 5);
        assert_eq!(completed.status(), TaskStatus::Done);
        assert_eq!(host.application_queries().activities().unwrap().len(), 5);
        assert_eq!(host.snapshot().unwrap().0, 5);
    }

    #[test]
    fn invalid_project_status_does_not_commit_activity_receipt_or_revision() {
        use orbit_application::{TransitionProjectCommand, TransitionProjectWithActivityCommand};
        use orbit_domain::ProjectStatus;
        let mut host = AuthoritativeHost::open(":memory:").unwrap();
        host.create_project_accepted("user", "p", 0, project("p", "activity-p"))
            .unwrap();
        // Proposed -> Completed is not a permitted lifecycle transition.
        let rejected = host.transition_project_accepted(
            "user",
            "invalid",
            1,
            TransitionProjectWithActivityCommand {
                project: TransitionProjectCommand {
                    id: ProjectId::new("p").unwrap(),
                    status: ProjectStatus::Completed,
                },
                activity_id: ActivityId::new("activity-invalid").unwrap(),
                origin: ActivityOrigin::Human,
                summary: "invalid".into(),
            },
        );
        assert!(rejected.is_err());
        assert_eq!(host.snapshot().unwrap().0, 1);
        assert_eq!(host.application_queries().activities().unwrap().len(), 1);
        assert_eq!(
            host.application_queries()
                .project(&ProjectId::new("p").unwrap())
                .unwrap()
                .status(),
            ProjectStatus::Proposed
        );
    }

    #[test]
    fn rich_project_task_habit_share_revision_history_and_activity_commit() {
        let mut host = AuthoritativeHost::open(":memory:").unwrap();
        let (r1, dup1, _) = host
            .create_project_accepted("user", "rich-project", 0, project("p", "activity-p"))
            .unwrap();
        let (r2, dup2, _) = host
            .create_task_accepted("user", "rich-task", 1, task("t", "p", "activity-t"))
            .unwrap();
        let (r3, dup3, _) = host
            .create_habit_accepted("user", "rich-habit", 2, habit("h", "activity-h"))
            .unwrap();
        assert_eq!((r1, r2, r3), (1, 2, 3));
        assert!(!dup1 && !dup2 && !dup3);
        assert_eq!(host.snapshot().unwrap().0, 3);
        assert_eq!(host.application_queries().activities().unwrap().len(), 3);
        assert_eq!(host.application_queries().projects().unwrap().len(), 1);
        assert_eq!(host.application_queries().tasks().unwrap().len(), 1);
        assert_eq!(host.application_queries().habits().unwrap().len(), 1);
    }

    #[test]
    fn rich_command_retries_return_original_receipts_without_duplicate_activities() {
        let mut host = AuthoritativeHost::open(":memory:").unwrap();
        host.create_project_accepted("user", "p", 0, project("p", "activity-p"))
            .unwrap();
        host.create_task_accepted("user", "t", 1, task("t", "p", "activity-t"))
            .unwrap();
        let (revision, duplicate, _) = host
            .create_project_accepted("user", "p", 0, project("p", "activity-p"))
            .unwrap();
        assert_eq!(revision, 1);
        assert!(duplicate);
        assert_eq!(host.application_queries().activities().unwrap().len(), 2);
        assert_eq!(host.snapshot().unwrap().0, 2);
    }

    #[test]
    fn failed_rich_task_does_not_advance_global_revision_or_activity_history() {
        let mut host = AuthoritativeHost::open(":memory:").unwrap();
        host.create_project_accepted("user", "p", 0, project("p", "activity-p"))
            .unwrap();
        let failed =
            host.create_task_accepted("user", "bad", 1, task("t", "missing", "activity-t"));
        assert!(failed.is_err());
        assert_eq!(host.snapshot().unwrap().0, 1);
        assert_eq!(host.application_queries().activities().unwrap().len(), 1);
        assert!(host.application_queries().tasks().unwrap().is_empty());
        let (revision, _, _) = host
            .create_task_accepted("user", "good", 1, task("t", "p", "activity-t"))
            .unwrap();
        assert_eq!(revision, 2);
    }

    #[test]
    fn immutable_rich_payload_includes_activity_identity() {
        let mut host = AuthoritativeHost::open(":memory:").unwrap();
        host.create_project_accepted("user", "p", 0, project("p", "activity-one"))
            .unwrap();
        let changed_activity =
            host.create_project_accepted("user", "p", 0, project("p", "activity-two"));
        assert!(matches!(
            changed_activity,
            Err(AuthorityError::ImmutableConflict)
        ));
        assert_eq!(host.application_queries().activities().unwrap().len(), 1);
    }
    #[test]
    fn rich_habit_state_commands_share_receipts_and_global_revision() {
        use orbit_application::SetHabitActiveWithActivityCommand;
        let mut host = AuthoritativeHost::open(":memory:").unwrap();
        host.create_habit_accepted("user", "habit-create", 0, habit("h", "create-activity"))
            .unwrap();
        let pause = || SetHabitActiveWithActivityCommand {
            id: HabitId::new("h").unwrap(),
            active: false,
            activity_id: ActivityId::new("pause-activity").unwrap(),
            origin: ActivityOrigin::Human,
            summary: "Pause habit".into(),
        };
        let (r1, duplicate, id) = host
            .set_habit_active_accepted("user", "pause", 1, pause())
            .unwrap();
        assert_eq!(r1, 2);
        assert!(!duplicate);
        assert_eq!(id, "h");
        let (r2, duplicate, _) = host
            .set_habit_active_accepted("user", "pause", 1, pause())
            .unwrap();
        assert_eq!(r2, r1);
        assert!(duplicate);
        assert_eq!(host.snapshot().unwrap().0, 2);
        assert_eq!(host.application_queries().activities().unwrap().len(), 2);
        let changed = SetHabitActiveWithActivityCommand {
            id: HabitId::new("h").unwrap(),
            active: true,
            activity_id: ActivityId::new("pause-activity").unwrap(),
            origin: ActivityOrigin::Human,
            summary: "Pause habit".into(),
        };
        assert!(matches!(
            host.set_habit_active_accepted("user", "pause", 1, changed),
            Err(AuthorityError::ImmutableConflict)
        ));
        let resume = SetHabitActiveWithActivityCommand {
            id: HabitId::new("h").unwrap(),
            active: true,
            activity_id: ActivityId::new("resume-activity").unwrap(),
            origin: ActivityOrigin::Human,
            summary: "Resume habit".into(),
        };
        assert_eq!(
            host.set_habit_active_accepted("user", "resume", 2, resume)
                .unwrap()
                .0,
            3
        );
        assert_eq!(host.snapshot().unwrap().0, 3);
    }

    #[test]
    fn failed_rich_habit_update_rolls_back_receipt_and_history() {
        use orbit_application::SetHabitActiveWithActivityCommand;
        let mut host = AuthoritativeHost::open(":memory:").unwrap();
        let bad = SetHabitActiveWithActivityCommand {
            id: HabitId::new("missing").unwrap(),
            active: false,
            activity_id: ActivityId::new("activity-failed").unwrap(),
            origin: ActivityOrigin::Human,
            summary: "Invalid pause".into(),
        };
        assert!(
            host.set_habit_active_accepted("user", "bad", 0, bad)
                .is_err()
        );
        assert_eq!(host.snapshot().unwrap().0, 0);
        assert!(host.application_queries().activities().unwrap().is_empty());
        assert_eq!(
            host.create_habit_accepted("user", "good", 0, habit("h", "activity-good"))
                .unwrap()
                .0,
            1
        );
    }

    #[test]
    fn rich_task_planning_preserves_application_rows_and_revision() {
        use orbit_application::{SetTaskPlanningCommand, SetTaskPlanningWithActivityCommand};
        use orbit_domain::TaskPlanning;
        let mut host = AuthoritativeHost::open(":memory:").unwrap();
        host.create_project_accepted("user", "p", 0, project("p", "activity-p"))
            .unwrap();
        host.create_task_accepted("user", "t", 1, task("t", "p", "activity-t"))
            .unwrap();
        let command = SetTaskPlanningWithActivityCommand {
            task: SetTaskPlanningCommand {
                id: TaskId::new("t").unwrap(),
                planning: TaskPlanning::new(Some(30), None, None).unwrap(),
            },
            activity_id: ActivityId::new("activity-plan").unwrap(),
            origin: ActivityOrigin::Human,
            summary: "Plan task".into(),
        };
        let (revision, duplicate, id) = host
            .set_task_planning_accepted("user", "plan", 2, command.clone())
            .unwrap();
        assert_eq!((revision, duplicate, id.as_str()), (3, false, "t"));
        assert!(
            host.set_task_planning_accepted("user", "plan", 2, command)
                .unwrap()
                .1
        );
        assert_eq!(host.snapshot().unwrap().0, 3);
        assert_eq!(host.application_queries().activities().unwrap().len(), 3);
    }
}
