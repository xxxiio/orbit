//! Rich local MCP tools: task details, acceptance criteria, workflow binding,
//! per-task activity and attachments.
//!
//! Every mutation is routed through the same canonical accepted commands the
//! CLI and TUI use. This module never opens its own store, never writes SQL
//! directly, never launches an agent job, and never returns attachment bytes in
//! a log line.
use std::fs;
use std::path::Path;

use orbit_application::{
    AcceptanceCriteriaChange, AcceptanceCriteriaCommand, AssociateAttachmentCommand,
    CreateProjectCommand, CreateProjectWithActivityCommand, ImportAttachmentCommand,
    RemoveAttachmentCommand, UpdateTaskDetailsCommand, parse_task_priority,
};
use orbit_domain::{
    AcceptanceCriterionId, ActivitySubject, Attachment, AttachmentId, AttachmentOwner,
    AttachmentOwnerKind, ProjectId, TaskId, content_type_for_filename,
    validate_attachment_filename,
};

use super::*;

/// Default and hard upper bounds for `attachment.read`.
const DEFAULT_ATTACHMENT_READ_BYTES: i64 = 1024 * 1024;
const MAX_ATTACHMENT_READ_BYTES: i64 = 4 * 1024 * 1024;

impl Server {
    /// Dispatch the rich local tools. Returns `None` for names owned by the
    /// main match so the existing surface stays unchanged.
    pub(super) fn dispatch_rich(
        &mut self,
        name: &str,
        args: &Value,
    ) -> Option<Result<Value, String>> {
        Some(match name {
            "task.get" => self.task_get(args),
            "task.update" => self.task_update(args),
            "task.acceptance.list" => self.task_acceptance_list(args),
            "task.acceptance.add" => self.task_acceptance_add(args),
            "task.acceptance.update" => self.task_acceptance_update(args),
            "task.acceptance.check" => self.task_acceptance_check(args),
            "task.acceptance.reorder" => self.task_acceptance_reorder(args),
            "task.acceptance.remove" => self.task_acceptance_remove(args),
            "task.activity" => self.task_activity(args),
            "project.create" => self.project_create(args),
            "workflow.list" => self.workflow_list(args),
            "workflow.apply" => self.workflow_apply(args),
            "attachment.list" => self.attachment_list(args),
            "attachment.inspect" => self.attachment_inspect(args),
            "attachment.add" => self.attachment_add(args),
            "attachment.read" => self.attachment_read(args),
            "attachment.remove" => self.attachment_remove(args),
            "attachment.associate" => self.attachment_associate(args),
            _ => return None,
        })
    }

    fn mcp_actor(&self) -> String {
        format!("mcp_{}_{}", self.policy.agent_id, self.policy.session_id).replace(
            |c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '_'),
            "_",
        )
    }

    fn scoped_task(&self, args: &Value, key: &str) -> Result<TaskId, String> {
        let id = TaskId::new(required_str(args, key)?).map_err(debug_err)?;
        let task = self
            .authority
            .application_queries()
            .task(&id)
            .map_err(debug_err)?;
        self.ensure_task_visible(&task)
            .map_err(|error| format!("task {id} is outside agent project scope: {error}"))?;
        Ok(id)
    }

    /// Projection shared by `task.get` and `task.update`. Acceptance criteria
    /// are read from the same authority, never cached client-side.
    fn task_json(&self, id: &TaskId) -> Result<Value, String> {
        let task = self
            .authority
            .application_queries()
            .task(id)
            .map_err(debug_err)?;
        let details = task.details();
        let criteria = self.authority.acceptance_criteria(id).map_err(debug_err)?;
        Ok(json!({
            "id": task.id().as_str(),
            "title": task.title(),
            "status": task.status().as_str(),
            "project_id": task.scope().project_id().map(|p| p.as_str()),
            "milestone_id": task.scope().milestone_id().map(|m| m.as_str()),
            "planning": {
                "estimated_minutes": task.planning().estimated_minutes(),
                "earliest_start_at_ms": task.planning().earliest_start_at_ms(),
                "deadline_at_ms": task.planning().deadline_at_ms(),
            },
            "description": details.description(),
            "priority": details.priority().map(|p| p.as_str()),
            "labels": details.labels(),
            "acceptance_criteria": criteria_json(&criteria),
        }))
    }

    fn task_get(&self, args: &Value) -> Result<Value, String> {
        let id = self.scoped_task(args, "id")?;
        self.task_json(&id)
    }

    fn task_update(&mut self, args: &Value) -> Result<Value, String> {
        self.policy.write_allowed()?;
        if args.get("description").is_none()
            && args.get("priority").is_none()
            && args.get("labels").is_none()
        {
            return Err(
                "task.update requires at least one of description, priority or labels".into(),
            );
        }
        let id = self.scoped_task(args, "id")?;
        let task = self
            .authority
            .application_queries()
            .task(&id)
            .map_err(debug_err)?;
        let current = task.details();

        let description = match args.get("description") {
            Some(value) => value
                .as_str()
                .ok_or_else(|| "description must be a string".to_owned())?
                .to_owned(),
            None => current.description().to_owned(),
        };
        let priority = match args.get("priority") {
            Some(value) => parse_priority_arg(value)?,
            None => current.priority(),
        };
        let labels = match args.get("labels") {
            Some(value) => parse_labels_arg(value)?,
            None => current.labels().to_vec(),
        };

        let revision = self.authority.snapshot().map_err(debug_err)?.0;
        self.authority
            .update_task_details_accepted(
                &self.mcp_actor(),
                &mcp_command_id(args),
                revision,
                UpdateTaskDetailsCommand {
                    task_id: id.clone(),
                    description,
                    priority,
                    labels,
                    activity_id: mcp_activity_id(args)?,
                    origin: self.policy.origin(),
                },
            )
            .map_err(debug_err)?;
        self.task_json(&id)
    }

    fn task_acceptance_list(&self, args: &Value) -> Result<Value, String> {
        let id = self.scoped_task(args, "task_id")?;
        let criteria = self.authority.acceptance_criteria(&id).map_err(debug_err)?;
        Ok(json!({"task_id": id.as_str(), "criteria": criteria_json(&criteria)}))
    }

    fn task_acceptance_add(&mut self, args: &Value) -> Result<Value, String> {
        self.policy.write_allowed()?;
        let id = self.scoped_task(args, "task_id")?;
        let text = required_str(args, "text")?;
        let criterion_id = AcceptanceCriterionId::new(new_id("criterion")).map_err(debug_err)?;
        self.apply_criteria(
            args,
            id,
            AcceptanceCriteriaChange::Add { criterion_id, text },
        )
    }

    fn task_acceptance_update(&mut self, args: &Value) -> Result<Value, String> {
        self.policy.write_allowed()?;
        let id = self.scoped_task(args, "task_id")?;
        let criterion_id = self.criterion_id(args)?;
        let text = required_str(args, "text")?;
        self.apply_criteria(
            args,
            id,
            AcceptanceCriteriaChange::SetText { criterion_id, text },
        )
    }

    fn task_acceptance_check(&mut self, args: &Value) -> Result<Value, String> {
        self.policy.write_allowed()?;
        let id = self.scoped_task(args, "task_id")?;
        let criterion_id = self.criterion_id(args)?;
        let completed = optional_bool(args, "completed", true)?;
        self.apply_criteria(
            args,
            id,
            AcceptanceCriteriaChange::SetCompleted {
                criterion_id,
                completed,
            },
        )
    }

    fn task_acceptance_reorder(&mut self, args: &Value) -> Result<Value, String> {
        self.policy.write_allowed()?;
        let id = self.scoped_task(args, "task_id")?;
        let criterion_id = self.criterion_id(args)?;
        let position = required_i64(args, "position")?;
        if position < 0 {
            return Err("position must be a non-negative zero-based index".into());
        }
        self.apply_criteria(
            args,
            id,
            AcceptanceCriteriaChange::Reorder {
                criterion_id,
                position: position as usize,
            },
        )
    }

    fn task_acceptance_remove(&mut self, args: &Value) -> Result<Value, String> {
        self.policy.write_allowed()?;
        let id = self.scoped_task(args, "task_id")?;
        let criterion_id = self.criterion_id(args)?;
        self.apply_criteria(args, id, AcceptanceCriteriaChange::Remove { criterion_id })
    }

    fn criterion_id(&self, args: &Value) -> Result<AcceptanceCriterionId, String> {
        AcceptanceCriterionId::new(required_str(args, "criterion_id")?).map_err(debug_err)
    }

    fn apply_criteria(
        &mut self,
        args: &Value,
        task_id: TaskId,
        change: AcceptanceCriteriaChange,
    ) -> Result<Value, String> {
        let revision = self.authority.snapshot().map_err(debug_err)?.0;
        self.authority
            .apply_acceptance_criteria_accepted(
                &self.mcp_actor(),
                &mcp_command_id(args),
                revision,
                AcceptanceCriteriaCommand {
                    task_id: task_id.clone(),
                    change,
                    activity_id: mcp_activity_id(args)?,
                    origin: self.policy.origin(),
                },
            )
            .map_err(debug_err)?;
        let criteria = self
            .authority
            .acceptance_criteria(&task_id)
            .map_err(debug_err)?;
        Ok(json!({"task_id": task_id.as_str(), "criteria": criteria_json(&criteria)}))
    }

    fn task_activity(&self, args: &Value) -> Result<Value, String> {
        let task_id = self.scoped_task(args, "task_id")?;
        let limit = optional_i64(args, "limit", 50)?;
        if !(0..=200).contains(&limit) {
            return Err("limit must be between 0 and 200".into());
        }
        let activities = self
            .authority
            .application_queries()
            .activities()
            .map_err(debug_err)?;
        let mut rows = activities
            .iter()
            .filter(|activity| match activity.subject() {
                ActivitySubject::Task(subject) => subject == &task_id,
                _ => false,
            })
            .map(|activity| {
                let dto = orbit_client::ActivityDto::from(activity);
                json!({
                    "id": dto.id,
                    "kind": dto.kind,
                    "origin": dto.origin,
                    "occurred_at_ms": dto.occurred_at_ms,
                    "summary": dto.summary,
                })
            })
            .collect::<Vec<_>>();
        // Append order is oldest first; callers get newest first.
        rows.reverse();
        rows.truncate(limit as usize);
        Ok(json!(rows))
    }

    fn project_create(&mut self, args: &Value) -> Result<Value, String> {
        self.policy.write_allowed()?;
        let name = required_str(args, "name")?;
        let workflow = match args.get("workflow") {
            Some(value) => value
                .as_str()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| "workflow must be a non-empty workflow id".to_owned())?
                .to_owned(),
            None => "basic".to_owned(),
        };
        ensure_bundled_workflow(&workflow)?;

        let id = match args.get("id") {
            Some(value) => {
                let id = value
                    .as_str()
                    .filter(|value| !value.trim().is_empty())
                    .ok_or_else(|| "id must be a non-empty project id".to_owned())?;
                id.to_owned()
            }
            None if self.policy.projects.is_empty() => new_id("project"),
            None => {
                return Err(
                    "project.create requires an explicit id within the agent project allowlist"
                        .into(),
                );
            }
        };
        let project_id = ProjectId::new(id).map_err(debug_err)?;
        if !self.policy.project_allowed(&project_id) {
            return Err(format!(
                "project {} is outside agent project scope",
                project_id.as_str()
            ));
        }

        let revision = self.authority.snapshot().map_err(debug_err)?.0;
        let (_, _, project) = self
            .authority
            .create_project_accepted(
                &self.mcp_actor(),
                &mcp_command_id(args),
                revision,
                CreateProjectWithActivityCommand {
                    project: CreateProjectCommand {
                        id: project_id.clone(),
                        name,
                    },
                    activity_id: mcp_activity_id(args)?,
                    origin: self.policy.origin(),
                    summary: format!("Agent created project {}", project_id.as_str()),
                },
            )
            .map_err(debug_err)?;

        // Basic is bound atomically during creation. Any other bundled workflow
        // is an explicit follow-up binding through the same client path.
        if workflow != "basic" {
            let revision = self.authority.snapshot().map_err(debug_err)?.0;
            self.authority
                .set_project_workflow_accepted(
                    &self.mcp_actor(),
                    &format!("{}:workflow", mcp_command_id(args)),
                    revision,
                    project_id.clone(),
                    workflow.clone(),
                    None,
                )
                .map_err(debug_err)?;
        }

        let binding = self
            .authority
            .active_project_blueprint(project_id.as_str())
            .map_err(debug_err)?;
        Ok(json!({
            "id": project.id().as_str(),
            "name": project.name(),
            "status": project.status().as_str(),
            "workflow": binding.map(|binding| json!({
                "id": binding.blueprint_id,
                "version": binding.version,
            })),
        }))
    }

    fn workflow_list(&self, args: &Value) -> Result<Value, String> {
        let workflows = bundled_workflow_json();
        let project_id = match args.get("project_id") {
            Some(value) => Some(
                value
                    .as_str()
                    .filter(|value| !value.trim().is_empty())
                    .ok_or_else(|| "project_id must be a non-empty project id".to_owned())?
                    .to_owned(),
            ),
            None => None,
        };
        let binding = match &project_id {
            Some(id) => {
                let project = ProjectId::new(id.clone()).map_err(debug_err)?;
                if !self.policy.project_allowed(&project) {
                    return Err("project outside agent scope".into());
                }
                self.authority
                    .active_project_blueprint(id)
                    .map_err(debug_err)?
                    .map(|binding| {
                        json!({
                            "project_id": binding.project_id,
                            "workflow_id": binding.blueprint_id,
                            "version": binding.version,
                        })
                    })
            }
            None => None,
        };
        Ok(json!({
            "workflows": workflows,
            "project_id": project_id,
            "binding": binding,
        }))
    }

    fn workflow_apply(&mut self, args: &Value) -> Result<Value, String> {
        self.policy.write_allowed()?;
        let project_id = ProjectId::new(required_str(args, "project_id")?).map_err(debug_err)?;
        if !self.policy.project_allowed(&project_id) {
            return Err("project outside agent scope".into());
        }
        let workflow_id = required_str(args, "workflow_id")?;
        let version = match args.get("version") {
            Some(value) => {
                let version = value
                    .as_u64()
                    .ok_or_else(|| "version must be a non-negative integer".to_owned())?;
                Some(u32::try_from(version).map_err(|_| "version is out of range".to_owned())?)
            }
            None => None,
        };
        ensure_known_workflow(&self.authority, &workflow_id)?;
        let revision = self.authority.snapshot().map_err(debug_err)?.0;
        self.authority
            .set_project_workflow_accepted(
                &self.mcp_actor(),
                &mcp_command_id(args),
                revision,
                project_id.clone(),
                workflow_id,
                version,
            )
            .map_err(debug_err)?;
        let binding = self
            .authority
            .active_project_blueprint(project_id.as_str())
            .map_err(debug_err)?
            .ok_or("workflow binding was not projected after apply")?;
        Ok(json!({
            "project_id": binding.project_id,
            "workflow_id": binding.blueprint_id,
            "version": binding.version,
            "applied": true,
        }))
    }

    fn attachment_list(&self, args: &Value) -> Result<Value, String> {
        let owner = attachment_owner(args)?;
        self.ensure_attachment_owner_scope(&owner)?;
        let attachments = self.authority.list_attachments(&owner).map_err(debug_err)?;
        Ok(json!({
            "owner_kind": owner.kind().as_str(),
            "owner_id": owner.id(),
            "attachments": attachments.iter().map(attachment_json).collect::<Vec<_>>(),
        }))
    }

    fn attachment_inspect(&self, args: &Value) -> Result<Value, String> {
        let id = AttachmentId::new(required_str(args, "id")?).map_err(debug_err)?;
        let (attachment, content) = self.authority.attachment_content(&id).map_err(debug_err)?;
        self.ensure_attachment_owner_scope(attachment.owner())?;
        let mut value = attachment_json(&attachment);
        value["content_available"] = json!(true);
        value["content_size"] = json!(content.len());
        Ok(value)
    }

    fn attachment_add(&mut self, args: &Value) -> Result<Value, String> {
        self.policy.write_allowed()?;
        let owner = attachment_owner(args)?;
        self.ensure_attachment_owner_scope(&owner)?;
        let description = optional_string(args, "description")?;
        let path = required_str(args, "path")?;
        let source = Path::new(&path);
        let metadata = fs::symlink_metadata(source)
            .map_err(|error| format!("cannot read attachment source {path:?}: {error}"))?;
        if metadata.file_type().is_symlink() {
            return Err(format!(
                "attachment source {path:?} is a symbolic link; import the real file instead"
            ));
        }
        if !metadata.is_file() {
            return Err(format!(
                "attachment source {path:?} is not a regular file; directories and devices cannot be attached"
            ));
        }
        let filename = source
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| format!("attachment source {path:?} has no portable file name"))?
            .to_owned();
        validate_attachment_filename(&filename).map_err(debug_err)?;
        // Read the bytes but never log or echo them.
        let content = fs::read(source)
            .map_err(|error| format!("cannot read attachment source {path:?}: {error}"))?;
        let content_type = content_type_for_filename(&filename).to_owned();
        let id = AttachmentId::new(new_id("attachment")).map_err(debug_err)?;
        let revision = self.authority.snapshot().map_err(debug_err)?.0;
        self.authority
            .import_attachment_accepted(
                &self.mcp_actor(),
                &mcp_command_id(args),
                revision,
                ImportAttachmentCommand {
                    id: id.clone(),
                    owner,
                    filename,
                    content_type: Some(content_type),
                    content,
                    description,
                    activity_id: mcp_activity_id(args)?,
                    origin: self.policy.origin(),
                },
            )
            .map_err(debug_err)?;
        let (attachment, _) = self.authority.attachment_content(&id).map_err(debug_err)?;
        Ok(attachment_json(&attachment))
    }

    fn attachment_read(&self, args: &Value) -> Result<Value, String> {
        let id = AttachmentId::new(required_str(args, "id")?).map_err(debug_err)?;
        let max_bytes = optional_i64(args, "max_bytes", DEFAULT_ATTACHMENT_READ_BYTES)?;
        if max_bytes <= 0 {
            return Err("max_bytes must be a positive integer".into());
        }
        if max_bytes > MAX_ATTACHMENT_READ_BYTES {
            return Err(format!(
                "max_bytes must not exceed {MAX_ATTACHMENT_READ_BYTES} (4 MiB)"
            ));
        }
        let (attachment, content) = self.authority.attachment_content(&id).map_err(debug_err)?;
        self.ensure_attachment_owner_scope(attachment.owner())?;
        let mut value = attachment_json(&attachment);
        value["content_available"] = json!(true);
        value["content_size"] = json!(content.len());
        if content.len() as i64 > max_bytes {
            value["content_omitted"] = json!(true);
        } else {
            value["content_omitted"] = json!(false);
            value["content"] = json!(base64_encode(&content));
        }
        Ok(value)
    }

    fn attachment_remove(&mut self, args: &Value) -> Result<Value, String> {
        self.policy.write_allowed()?;
        let id = AttachmentId::new(required_str(args, "id")?).map_err(debug_err)?;
        let (attachment, _) = self.authority.attachment_content(&id).map_err(debug_err)?;
        self.ensure_attachment_owner_scope(attachment.owner())?;
        let revision = self.authority.snapshot().map_err(debug_err)?.0;
        let (_, _, content_removed) = self
            .authority
            .remove_attachment_accepted(
                &self.mcp_actor(),
                &mcp_command_id(args),
                revision,
                RemoveAttachmentCommand {
                    id: id.clone(),
                    activity_id: mcp_activity_id(args)?,
                    origin: self.policy.origin(),
                },
            )
            .map_err(debug_err)?;
        Ok(json!({
            "id": id.as_str(),
            "removed": true,
            "content_removed": content_removed,
        }))
    }

    fn attachment_associate(&mut self, args: &Value) -> Result<Value, String> {
        self.policy.write_allowed()?;
        let source_id = AttachmentId::new(required_str(args, "source_id")?).map_err(debug_err)?;
        let owner = attachment_owner(args)?;
        self.ensure_attachment_owner_scope(&owner)?;
        let (source, _) = self
            .authority
            .attachment_content(&source_id)
            .map_err(debug_err)?;
        self.ensure_attachment_owner_scope(source.owner())?;
        let new_id = AttachmentId::new(mcp_entity_id(args, "attachment")).map_err(debug_err)?;
        let revision = self.authority.snapshot().map_err(debug_err)?.0;
        self.authority
            .associate_attachment_accepted(
                &self.mcp_actor(),
                &mcp_command_id(args),
                revision,
                AssociateAttachmentCommand {
                    source_id,
                    new_id: new_id.clone(),
                    owner: owner.clone(),
                    activity_id: mcp_activity_id(args)?,
                    origin: self.policy.origin(),
                },
            )
            .map_err(debug_err)?;
        let attachment = self
            .authority
            .list_attachments(&owner)
            .map_err(debug_err)?
            .into_iter()
            .find(|attachment| attachment.id() == &new_id)
            .ok_or("associated attachment was not projected")?;
        Ok(attachment_json(&attachment))
    }

    /// Refuse attachment operations whose owner cannot be proven to live in an
    /// allowed project. Restricted policies fail closed instead of guessing.
    fn ensure_attachment_owner_scope(&self, owner: &AttachmentOwner) -> Result<(), String> {
        match owner.kind() {
            AttachmentOwnerKind::Task => {
                let task_id = TaskId::new(owner.id()).map_err(debug_err)?;
                let task = self
                    .authority
                    .application_queries()
                    .task(&task_id)
                    .map_err(|error| {
                        format!(
                            "cannot verify project scope for task {}: {error:?}",
                            owner.id()
                        )
                    })?;
                self.ensure_task_visible(&task)
                    .map_err(|_| format!("task {} is outside agent project scope", owner.id()))
            }
            AttachmentOwnerKind::Project => {
                let project = ProjectId::new(owner.id()).map_err(debug_err)?;
                if self.policy.project_allowed(&project) {
                    Ok(())
                } else {
                    Err(format!(
                        "attachment owner project {} is outside agent scope",
                        owner.id()
                    ))
                }
            }
            AttachmentOwnerKind::WorkItem => {
                let item = self
                    .authority
                    .project_work_item(owner.id())
                    .map_err(debug_err)?;
                match item {
                    Some(item) => {
                        let project = ProjectId::new(item.project_id).map_err(debug_err)?;
                        if self.policy.project_allowed(&project) {
                            Ok(())
                        } else {
                            Err("attachment work item is outside agent project scope".into())
                        }
                    }
                    None if self.policy.projects.is_empty()
                        && self.policy.allow_unscoped_execution =>
                    {
                        Ok(())
                    }
                    None => Err(format!(
                        "cannot verify project scope for work item {}; refusing under a scoped policy",
                        owner.id()
                    )),
                }
            }
            AttachmentOwnerKind::Knowledge => {
                if !self.policy.projects.is_empty() {
                    return Err(
                        "cannot verify project scope for knowledge attachments under a scoped policy"
                            .into(),
                    );
                }
                if self.policy.allow_unscoped_knowledge {
                    Ok(())
                } else {
                    Err("knowledge attachments require unscoped knowledge permission".into())
                }
            }
            AttachmentOwnerKind::AgentJob => {
                if !self.policy.projects.is_empty() {
                    return Err(
                        "cannot verify project scope for agent job attachments under a scoped policy"
                            .into(),
                    );
                }
                if self.policy.allow_unscoped_execution {
                    Ok(())
                } else {
                    Err("agent job attachments require unscoped execution permission".into())
                }
            }
        }
    }
}

fn criteria_json(criteria: &orbit_domain::AcceptanceCriteria) -> Vec<Value> {
    criteria
        .items()
        .iter()
        .enumerate()
        .map(|(position, criterion)| {
            json!({
                "id": criterion.id().as_str(),
                "text": criterion.text(),
                "completed": criterion.is_completed(),
                "completed_at_ms": criterion.completed_at_ms(),
                "position": position,
            })
        })
        .collect()
}

fn attachment_json(attachment: &Attachment) -> Value {
    json!({
        "id": attachment.id().as_str(),
        "owner_kind": attachment.owner().kind().as_str(),
        "owner_id": attachment.owner().id(),
        "filename": attachment.filename(),
        "content_type": attachment.content_type(),
        "size_bytes": attachment.size_bytes(),
        "checksum": attachment.checksum(),
        "created_at_ms": attachment.created_at_ms(),
        "description": attachment.description(),
        "is_image": attachment.is_image(),
    })
}

fn parse_priority_arg(value: &Value) -> Result<Option<orbit_domain::TaskPriority>, String> {
    match value {
        Value::Null => Ok(None),
        Value::String(text) => {
            let text = text.trim();
            if text.is_empty() || text.eq_ignore_ascii_case("none") {
                Ok(None)
            } else {
                parse_task_priority(text).map(Some).map_err(|_| {
                    format!(
                        "invalid task priority {text:?}; allowed values are low, medium, high, urgent, none"
                    )
                })
            }
        }
        _ => Err("priority must be a string: low, medium, high, urgent, none".into()),
    }
}

fn parse_labels_arg(value: &Value) -> Result<Vec<String>, String> {
    let labels = value
        .as_array()
        .ok_or_else(|| "labels must be an array of strings".to_owned())?;
    labels
        .iter()
        .map(|label| {
            label
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| "labels must be an array of strings".to_owned())
        })
        .collect()
}

fn attachment_owner(args: &Value) -> Result<AttachmentOwner, String> {
    let kind = required_str(args, "owner_kind")?;
    let id = required_str(args, "owner_id")?;
    let kind = AttachmentOwnerKind::parse(kind.trim()).ok_or_else(|| {
        format!(
            "unknown attachment owner kind {kind:?}; allowed values are task, project, work_item, knowledge, agent_job"
        )
    })?;
    AttachmentOwner::new(kind, id).map_err(debug_err)
}

fn optional_i64(args: &Value, key: &str, default: i64) -> Result<i64, String> {
    match args.get(key) {
        None => Ok(default),
        Some(value) => value
            .as_i64()
            .ok_or_else(|| format!("{key} must be an integer")),
    }
}

fn optional_bool(args: &Value, key: &str, default: bool) -> Result<bool, String> {
    match args.get(key) {
        None => Ok(default),
        Some(value) => value
            .as_bool()
            .ok_or_else(|| format!("{key} must be a boolean")),
    }
}

fn optional_string(args: &Value, key: &str) -> Result<Option<String>, String> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value
            .as_str()
            .map(|text| Some(text.to_owned()))
            .ok_or_else(|| format!("{key} must be a string")),
    }
}

/// Bundled workflow descriptors in stable compiled order.
fn bundled_workflow_json() -> Vec<Value> {
    orbit_blueprint::bundled::BUNDLED_BLUEPRINTS
        .iter()
        .map(|descriptor| {
            json!({
                "id": descriptor.id,
                "name": descriptor.name,
                "description": descriptor.description,
                "version": descriptor.version,
                "bundled": true,
                "default": descriptor.id == "basic",
            })
        })
        .collect()
}

fn bundled_workflow_ids() -> Vec<&'static str> {
    orbit_blueprint::bundled::BUNDLED_BLUEPRINTS
        .iter()
        .map(|descriptor| descriptor.id)
        .collect()
}

fn ensure_bundled_workflow(id: &str) -> Result<u32, String> {
    orbit_blueprint::bundled::BUNDLED_BLUEPRINTS
        .iter()
        .find(|descriptor| descriptor.id == id)
        .map(|descriptor| descriptor.version)
        .ok_or_else(|| {
            format!(
                "unknown workflow {id:?}; available bundled workflows: {}",
                bundled_workflow_ids().join(", ")
            )
        })
}

/// Accept a bundled workflow or an already-published one; reject anything the
/// authority could not resolve, with the bundled ids in the message.
fn ensure_known_workflow(authority: &AuthoritativeHost, id: &str) -> Result<(), String> {
    if ensure_bundled_workflow(id).is_ok() {
        return Ok(());
    }
    let published = authority
        .available_blueprint_versions(Some(id))
        .map_err(debug_err)?;
    if published.is_empty() {
        return Err(format!(
            "unknown workflow {id:?}; available bundled workflows: {}",
            bundled_workflow_ids().join(", ")
        ));
    }
    Ok(())
}

/// Small dependency-free RFC 4648 base64 encoder for attachment reads.
pub(super) fn base64_encode(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut output = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let first = chunk[0];
        let second = chunk.get(1).copied().unwrap_or(0);
        let third = chunk.get(2).copied().unwrap_or(0);
        output.push(TABLE[(first >> 2) as usize] as char);
        output.push(TABLE[(((first & 0b11) << 4) | (second >> 4)) as usize] as char);
        if chunk.len() > 1 {
            output.push(TABLE[(((second & 0b1111) << 2) | (third >> 6)) as usize] as char);
        } else {
            output.push('=');
        }
        if chunk.len() > 2 {
            output.push(TABLE[(third & 0b111111) as usize] as char);
        } else {
            output.push('=');
        }
    }
    output
}
