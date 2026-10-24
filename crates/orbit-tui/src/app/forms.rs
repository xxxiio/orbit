//! Form field builders, form construction and per-domain submitters.
use super::*;

impl App {
    fn text_field(
        key: &str,
        label: &str,
        value: impl Into<String>,
        required: bool,
        hint: &str,
    ) -> FormField {
        let value = value.into();
        let cursor = value.chars().count();
        FormField {
            key: key.into(),
            label: label.into(),
            initial: value.clone(),
            value,
            cursor,
            kind: FormFieldKind::Text,
            required,
            hint: hint.into(),
        }
    }

    fn integer_field(
        key: &str,
        label: &str,
        value: impl Into<String>,
        required: bool,
        hint: &str,
    ) -> FormField {
        let value = value.into();
        let cursor = value.chars().count();
        FormField {
            key: key.into(),
            label: label.into(),
            initial: value.clone(),
            value,
            cursor,
            kind: FormFieldKind::Integer,
            required,
            hint: hint.into(),
        }
    }

    fn boolean_field(key: &str, label: &str, value: bool) -> FormField {
        let value = if value { "true" } else { "false" }.to_owned();
        FormField {
            key: key.into(),
            label: label.into(),
            initial: value.clone(),
            value,
            cursor: 0,
            kind: FormFieldKind::Boolean,
            required: false,
            hint: String::new(),
        }
    }

    fn choice_field(
        key: &str,
        label: &str,
        value: impl Into<String>,
        choices: Vec<FormChoice>,
        required: bool,
    ) -> FormField {
        let value = value.into();
        FormField {
            key: key.into(),
            label: label.into(),
            initial: value.clone(),
            value,
            cursor: 0,
            kind: FormFieldKind::Choice(choices),
            required,
            hint: String::new(),
        }
    }

    fn form_choice(value: impl Into<String>, label: impl Into<String>) -> FormChoice {
        FormChoice {
            value: value.into(),
            label: label.into(),
        }
    }

    fn multiline_field(
        key: &str,
        label: &str,
        value: impl Into<String>,
        required: bool,
        hint: &str,
    ) -> FormField {
        let value = value.into();
        let cursor = value.chars().count();
        FormField {
            key: key.into(),
            label: label.into(),
            initial: value.clone(),
            value,
            cursor,
            kind: FormFieldKind::Multiline,
            required,
            hint: hint.into(),
        }
    }

    /// Workflows available to apply, with Basic first and clearly marked as the
    /// default. Works offline because the descriptors are compiled in.
    fn workflow_choices(&self) -> Vec<FormChoice> {
        let mut descriptors = self.client.bundled_workflows();
        descriptors.sort_by_key(|descriptor| (!descriptor.is_default, descriptor.name.clone()));
        descriptors
            .into_iter()
            .map(|descriptor| {
                let label = if descriptor.is_default {
                    format!("{} (default)", descriptor.name)
                } else {
                    descriptor.name.clone()
                };
                Self::form_choice(descriptor.id, label)
            })
            .collect()
    }

    fn project_choices(&self, include_none: bool) -> Vec<FormChoice> {
        let mut choices = Vec::new();
        if include_none {
            choices.push(Self::form_choice("", "(none)"));
        }
        choices.extend(
            self.projects
                .iter()
                .map(|project| Self::form_choice(project.id.clone(), project.name.clone())),
        );
        choices
    }

    fn metric_choices(&self, include_none: bool) -> Vec<FormChoice> {
        let mut choices = Vec::new();
        if include_none {
            choices.push(Self::form_choice("", "(select)"));
        }
        if let Some(project) = &self.project {
            choices.extend(
                project
                    .data
                    .metrics
                    .iter()
                    .map(|metric| Self::form_choice(metric.id.clone(), metric.id.clone())),
            );
        }
        choices
    }

    fn blueprint_version_choices(&self) -> Vec<FormChoice> {
        let mut choices = Vec::new();
        if let Ok(installed) = self.client.list_blueprint_versions() {
            choices.extend(
                installed
                    .into_iter()
                    .map(|(id, version)| {
                        Self::form_choice(format!("{id}@{version}"), format!("{id}@{version}"))
                    })
                    .collect::<Vec<_>>(),
            );
        }
        choices
    }

    fn work_item_kind_choices(blueprint: &Blueprint) -> Vec<FormChoice> {
        blueprint
            .work_items
            .iter()
            .map(|kind| Self::form_choice(kind.id.clone(), kind.name.clone()))
            .collect()
    }

    fn work_item_fields(blueprint: &Blueprint, kind: &str) -> Option<Vec<FormField>> {
        let definition = blueprint.item_kind(kind).ok()?;
        Some(
            definition
                .fields
                .iter()
                .map(|field| {
                    let value = field
                        .default
                        .as_ref()
                        .map(|value| match value {
                            serde_json::Value::String(text) => text.clone(),
                            other => other.to_string(),
                        })
                        .unwrap_or_default();
                    match &field.kind {
                        FieldKind::Integer => Self::integer_field(
                            &field.id,
                            &field.id,
                            value,
                            field.required,
                            "integer",
                        ),
                        FieldKind::Boolean => Self::boolean_field(
                            &field.id,
                            &field.id,
                            field
                                .default
                                .as_ref()
                                .and_then(|value| value.as_bool())
                                .unwrap_or(false),
                        ),
                        FieldKind::Choice { options } => Self::choice_field(
                            &field.id,
                            &field.id,
                            value,
                            options
                                .iter()
                                .map(|option| Self::form_choice(option.clone(), option.clone()))
                                .collect(),
                            field.required,
                        ),
                        FieldKind::Text => {
                            Self::text_field(&field.id, &field.id, value, field.required, "text")
                        }
                    }
                })
                .collect(),
        )
    }

    pub fn quick_edit_selected(&mut self) {
        let Some(item) = self.selected_item() else {
            self.notify("Select a task to quick edit", StatusKind::Warning);
            return;
        };
        if item.kind != ItemKind::Task {
            self.notify("Quick edit applies to tasks", StatusKind::Warning);
            return;
        }
        let task_id = item.id.clone();
        self.start_form(FormMode::QuickEditTask { task_id });
    }

    pub fn start_form(&mut self, mode: FormMode) {
        let Some(form) = self.build_form(mode) else {
            return;
        };
        self.modal = Some(Modal::Form(form));
    }

    fn project_status_choices(current: &str) -> Vec<FormChoice> {
        use orbit_domain::ProjectStatus;
        let current_status = match current {
            "proposed" => ProjectStatus::Proposed,
            "active" => ProjectStatus::Active,
            "on_hold" => ProjectStatus::OnHold,
            "completed" => ProjectStatus::Completed,
            "cancelled" => ProjectStatus::Cancelled,
            "archived" => ProjectStatus::Archived,
            _ => return Vec::new(),
        };
        [
            ProjectStatus::Proposed,
            ProjectStatus::Active,
            ProjectStatus::OnHold,
            ProjectStatus::Completed,
            ProjectStatus::Cancelled,
            ProjectStatus::Archived,
        ]
        .into_iter()
        .filter(|status| current_status.can_transition_to(*status))
        .map(|status| Self::form_choice(status.as_str(), status.as_str()))
        .collect()
    }

    fn build_form(&mut self, mode: FormMode) -> Option<FormState> {
        match mode {
            FormMode::NewProject => Some(FormState {
                title: "New project".into(),
                hint: "Create a project without leaving the TUI".into(),
                mode: FormMode::NewProject,
                fields: vec![
                    Self::text_field("name", "Name", "", true, "required"),
                    Self::text_field("objective", "Objective", "", false, "optional"),
                    Self::choice_field(
                        "workflow",
                        "Workflow",
                        "basic",
                        self.workflow_choices(),
                        true,
                    ),
                ],
                focused: 0,
                invalid: None,
                error: None,
                dirty: false,
            }),
            FormMode::NewTask { project_id } => Some(FormState {
                title: "New task".into(),
                hint: "Create a task in the selected project or inbox".into(),
                mode: FormMode::NewTask {
                    project_id: project_id.clone(),
                },
                fields: vec![
                    Self::text_field("title", "Title", "", true, "required"),
                    Self::choice_field(
                        "project",
                        "Project",
                        project_id.unwrap_or_default(),
                        self.project_choices(true),
                        false,
                    ),
                    Self::text_field("milestone", "Milestone", "", false, "optional ID"),
                    Self::integer_field("estimate", "Estimate", "", false, "minutes"),
                    Self::integer_field("earliest", "Earliest start", "", false, "epoch ms"),
                    Self::integer_field("deadline", "Deadline", "", false, "epoch ms"),
                ],
                focused: 0,
                invalid: None,
                error: None,
                dirty: false,
            }),
            FormMode::EditTask { task_id } => {
                let tasks = self.client.list_tasks().ok()?;
                let task = tasks.iter().find(|task| task.id == task_id)?;
                Some(FormState {
                    title: format!("Edit task · {}", task.title),
                    hint: "Everyday fields first; planning and scope are further down".into(),
                    mode: FormMode::EditTask {
                        task_id: task_id.clone(),
                    },
                    fields: vec![
                        Self::text_field("title", "Title", task.title.clone(), true, "required"),
                        Self::choice_field(
                            "priority",
                            "Priority",
                            task.priority.clone().unwrap_or_default(),
                            vec![
                                Self::form_choice("", "(unset)"),
                                Self::form_choice("low", "low"),
                                Self::form_choice("medium", "medium"),
                                Self::form_choice("high", "high"),
                                Self::form_choice("urgent", "urgent"),
                            ],
                            false,
                        ),
                        Self::multiline_field(
                            "description",
                            "Description",
                            task.description.clone(),
                            false,
                            "Markdown text",
                        ),
                        Self::text_field(
                            "labels",
                            "Labels",
                            task.labels.join(", "),
                            false,
                            "comma separated",
                        ),
                        Self::choice_field(
                            "project",
                            "Project",
                            task.project_id.clone().unwrap_or_default(),
                            self.project_choices(true),
                            false,
                        ),
                        Self::text_field(
                            "milestone",
                            "Milestone",
                            task.milestone_id.clone().unwrap_or_default(),
                            false,
                            "optional ID",
                        ),
                        Self::integer_field(
                            "estimate",
                            "Estimate",
                            task.estimated_minutes
                                .map(|value| value.to_string())
                                .unwrap_or_default(),
                            false,
                            "minutes",
                        ),
                        Self::integer_field(
                            "earliest",
                            "Earliest start",
                            task.earliest_start_at_ms
                                .map(|value| value.to_string())
                                .unwrap_or_default(),
                            false,
                            "epoch ms",
                        ),
                        Self::integer_field(
                            "deadline",
                            "Deadline",
                            task.deadline_at_ms
                                .map(|value| value.to_string())
                                .unwrap_or_default(),
                            false,
                            "epoch ms",
                        ),
                    ],
                    focused: 0,
                    invalid: None,
                    error: None,
                    dirty: false,
                })
            }
            FormMode::QuickEditTask { task_id } => {
                let task = self.task_dto(&task_id)?;
                let fields = vec![
                    Self::text_field("title", "Title", task.title.clone(), true, "required"),
                    Self::choice_field(
                        "priority",
                        "Priority",
                        task.priority.clone().unwrap_or_default(),
                        vec![
                            Self::form_choice("", "(unset)"),
                            Self::form_choice("low", "low"),
                            Self::form_choice("medium", "medium"),
                            Self::form_choice("high", "high"),
                            Self::form_choice("urgent", "urgent"),
                        ],
                        false,
                    ),
                ];
                Some(FormState {
                    title: format!("Quick edit · {}", task.title),
                    hint: "Title and priority; status changes stay in the S menu".into(),
                    mode: FormMode::QuickEditTask {
                        task_id: task_id.clone(),
                    },
                    fields,
                    focused: 0,
                    invalid: None,
                    error: None,
                    dirty: false,
                })
            }
            FormMode::EditTaskDetails { task_id } => {
                let task = self.task_dto(&task_id)?;
                let priority = task.priority.clone().unwrap_or_default();
                Some(FormState {
                    title: format!("Edit details · {}", task.title),
                    hint: "Markdown description, priority and labels. Ctrl+S saves; Esc cancels."
                        .into(),
                    mode: FormMode::EditTaskDetails {
                        task_id: task_id.clone(),
                    },
                    fields: vec![
                        Self::multiline_field(
                            "description",
                            "Description",
                            task.description.clone(),
                            false,
                            "Markdown text",
                        ),
                        Self::choice_field(
                            "priority",
                            "Priority",
                            priority,
                            vec![
                                Self::form_choice("", "(unset)"),
                                Self::form_choice("low", "low"),
                                Self::form_choice("medium", "medium"),
                                Self::form_choice("high", "high"),
                                Self::form_choice("urgent", "urgent"),
                            ],
                            false,
                        ),
                        Self::text_field(
                            "labels",
                            "Labels",
                            task.labels.join(", "),
                            false,
                            "comma separated",
                        ),
                    ],
                    focused: 0,
                    invalid: None,
                    error: None,
                    dirty: false,
                })
            }
            FormMode::AddCriterion { task_id } => {
                let task = self.task_dto(&task_id)?;
                Some(FormState {
                    title: format!("Add acceptance criterion · {}", task.title),
                    hint: "Ordered and independently checkable".into(),
                    mode: FormMode::AddCriterion {
                        task_id: task_id.clone(),
                    },
                    fields: vec![Self::text_field("text", "Criterion", "", true, "required")],
                    focused: 0,
                    invalid: None,
                    error: None,
                    dirty: false,
                })
            }
            FormMode::EditCriterion {
                task_id,
                criterion_id,
            } => {
                let items = self.client.acceptance_criteria(&task_id).ok()?;
                let item = items.iter().find(|item| item.id == criterion_id)?;
                Some(FormState {
                    title: "Edit acceptance criterion".into(),
                    hint: "Update the criterion text".into(),
                    mode: FormMode::EditCriterion {
                        task_id: task_id.clone(),
                        criterion_id: criterion_id.clone(),
                    },
                    fields: vec![Self::text_field(
                        "text",
                        "Criterion",
                        item.text.clone(),
                        true,
                        "required",
                    )],
                    focused: 0,
                    invalid: None,
                    error: None,
                    dirty: false,
                })
            }
            FormMode::AttachFile {
                owner_kind,
                owner_id,
            } => Some(FormState {
                title: "Attach file".into(),
                hint: "Enter an existing file path; content is copied into managed storage".into(),
                mode: FormMode::AttachFile {
                    owner_kind: owner_kind.clone(),
                    owner_id: owner_id.clone(),
                },
                fields: vec![
                    Self::text_field("path", "File path", "", true, "required"),
                    Self::text_field("description", "Caption", "", false, "optional"),
                ],
                focused: 0,
                invalid: None,
                error: None,
                dirty: false,
            }),
            FormMode::ExportAttachment { attachment_id } => Some(FormState {
                title: "Export attachment".into(),
                hint: "The destination must not already exist".into(),
                mode: FormMode::ExportAttachment {
                    attachment_id: attachment_id.clone(),
                },
                fields: vec![Self::text_field(
                    "destination",
                    "Destination path",
                    "",
                    true,
                    "required",
                )],
                focused: 0,
                invalid: None,
                error: None,
                dirty: false,
            }),
            FormMode::ChangeWorkflow { project_id } => Some(FormState {
                title: "Change workflow".into(),
                hint: "Basic is the default; the pinned version is preserved for existing bindings"
                    .into(),
                mode: FormMode::ChangeWorkflow {
                    project_id: project_id.clone(),
                },
                fields: vec![
                    Self::choice_field(
                        "workflow",
                        "Workflow",
                        "basic",
                        self.workflow_choices(),
                        true,
                    ),
                    Self::integer_field("version", "Version", "", false, "optional"),
                ],
                focused: 0,
                invalid: None,
                error: None,
                dirty: false,
            }),
            FormMode::AddDependency { task_id } => {
                let tasks = self
                    .workspace
                    .as_ref()
                    .map(|workspace| workspace.tasks.clone())
                    .filter(|tasks| !tasks.is_empty())
                    .or_else(|| {
                        self.project
                            .as_ref()
                            .map(|project| project.data.tasks.clone())
                    })
                    .unwrap_or_default();
                let choices: Vec<FormChoice> = tasks
                    .iter()
                    .filter(|task| task.id != task_id)
                    .map(|task| Self::form_choice(task.id.clone(), task.title.clone()))
                    .collect();
                if choices.is_empty() {
                    self.notify(
                        "No other task is available as a prerequisite",
                        StatusKind::Warning,
                    );
                    return None;
                }
                Some(FormState {
                    title: "Add task prerequisite".into(),
                    hint: "The authority rejects self, missing and cyclic edges".into(),
                    mode: FormMode::AddDependency {
                        task_id: task_id.clone(),
                    },
                    fields: vec![Self::choice_field(
                        "prerequisite",
                        "Prerequisite",
                        choices[0].value.clone(),
                        choices,
                        true,
                    )],
                    focused: 0,
                    invalid: None,
                    error: None,
                    dirty: false,
                })
            }
            FormMode::RemoveDependency { task_id } => {
                let task = self
                    .workspace
                    .as_ref()
                    .and_then(|workspace| workspace.tasks.iter().find(|task| task.id == task_id))
                    .cloned()
                    .or_else(|| {
                        self.project.as_ref().and_then(|project| {
                            project
                                .data
                                .tasks
                                .iter()
                                .find(|task| task.id == task_id)
                                .cloned()
                        })
                    })?;
                if task.prerequisite_ids.is_empty() {
                    self.notify("This task has no prerequisites", StatusKind::Warning);
                    return None;
                }
                let choices = task
                    .prerequisite_ids
                    .iter()
                    .map(|id| Self::form_choice(id.clone(), id.clone()))
                    .collect();
                Some(FormState {
                    title: "Remove task prerequisite".into(),
                    hint: "Removal is revision-checked by the authority".into(),
                    mode: FormMode::RemoveDependency { task_id },
                    fields: vec![Self::choice_field(
                        "prerequisite",
                        "Prerequisite",
                        "",
                        choices,
                        true,
                    )],
                    focused: 0,
                    invalid: None,
                    error: None,
                    dirty: false,
                })
            }
            FormMode::EditProject { project_id } => {
                let project = self
                    .project_details
                    .iter()
                    .find(|project| project.id == project_id)?;
                Some(FormState {
                    title: format!("Project settings · {}", project.name),
                    hint: "Change supported project settings; criteria and signals are preserved"
                        .into(),
                    mode: FormMode::EditProject {
                        project_id: project_id.clone(),
                    },
                    fields: vec![
                        Self::choice_field(
                            "status",
                            "Status",
                            project.status.clone(),
                            Self::project_status_choices(&project.status),
                            true,
                        ),
                        Self::choice_field(
                            "mode",
                            "Mode",
                            project.mode.clone(),
                            [
                                "standard",
                                "learning",
                                "habit_driven",
                                "metric_target",
                                "maintenance",
                                "custom",
                            ]
                            .into_iter()
                            .map(|mode| Self::form_choice(mode, mode))
                            .collect(),
                            true,
                        ),
                        Self::text_field(
                            "objective",
                            "Objective",
                            project.objective.clone().unwrap_or_default(),
                            false,
                            "optional",
                        ),
                    ],
                    focused: 0,
                    invalid: None,
                    error: None,
                    dirty: false,
                })
            }
            FormMode::NewWorkItemKind { project_id } => {
                if self.client.is_connected() {
                    self.notify(
                        "Work-item creation requires the connected console (`:`, then Connected commands)",
                        StatusKind::Warning,
                    );
                    return None;
                }
                let blueprint = match self.client.project_blueprint_definition(&project_id) {
                    Ok(Some(blueprint)) => blueprint,
                    Ok(None) => {
                        self.notify("This project has no pinned blueprint", StatusKind::Warning);
                        return None;
                    }
                    Err(error) => {
                        self.notify(
                            format!("Blueprint unavailable: {}", error.message),
                            StatusKind::Warning,
                        );
                        return None;
                    }
                };
                let choices = Self::work_item_kind_choices(&blueprint);
                if choices.is_empty() {
                    self.notify(
                        "This blueprint declares no work-item kinds",
                        StatusKind::Warning,
                    );
                    return None;
                }
                Some(FormState {
                    title: "New work item · choose kind".into(),
                    hint: "The next form shows the type's declared fields".into(),
                    mode: FormMode::NewWorkItemKind {
                        project_id: project_id.clone(),
                    },
                    fields: vec![Self::choice_field(
                        "kind",
                        "Kind",
                        choices[0].value.clone(),
                        choices,
                        true,
                    )],
                    focused: 0,
                    invalid: None,
                    error: None,
                    dirty: false,
                })
            }
            FormMode::NewWorkItem {
                project_id,
                blueprint,
                kind,
            } => Some(FormState {
                title: format!("New {kind}"),
                hint: "Fields are generated from the pinned blueprint".into(),
                mode: FormMode::NewWorkItem {
                    project_id: project_id.clone(),
                    blueprint: blueprint.clone(),
                    kind: kind.clone(),
                },
                fields: Self::work_item_fields(&blueprint, &kind)?,
                focused: 0,
                invalid: None,
                error: None,
                dirty: false,
            }),
            FormMode::EditWorkItem {
                item_id,
                revision,
                blueprint,
                kind,
            } => {
                let item = self
                    .client
                    .list_blueprint_work_items(
                        self.project
                            .as_ref()
                            .map(|project| project.id.as_str())
                            .unwrap_or(""),
                    )
                    .ok()?
                    .into_iter()
                    .find(|item| item.id == item_id)?;
                let mut fields = Self::work_item_fields(&blueprint, &kind)?;
                for field in &mut fields {
                    if let Some(value) = item.properties.get(&field.key) {
                        field.value = match value {
                            serde_json::Value::String(text) => text.clone(),
                            other => other.to_string(),
                        };
                    }
                    field.initial = field.value.clone();
                }
                Some(FormState {
                    title: format!("Edit work item · {}", item_id),
                    hint: "Only declared properties are editable".into(),
                    mode: FormMode::EditWorkItem {
                        item_id,
                        revision,
                        blueprint,
                        kind,
                    },
                    fields,
                    focused: 0,
                    invalid: None,
                    error: None,
                    dirty: false,
                })
            }
            FormMode::NewKnowledge => Some(FormState {
                title: "New knowledge note".into(),
                hint: "Capture a Markdown note".into(),
                mode: FormMode::NewKnowledge,
                fields: vec![
                    Self::text_field("title", "Title", "", true, "required"),
                    Self::text_field("body", "Body", "", false, "Markdown"),
                ],
                focused: 0,
                invalid: None,
                error: None,
                dirty: false,
            }),
            FormMode::InstallBlueprint => Some(FormState {
                title: "Install bundled blueprint".into(),
                hint: "Installation publishes a version; it does not bind existing projects".into(),
                mode: FormMode::InstallBlueprint,
                fields: vec![Self::choice_field(
                    "blueprint",
                    "Blueprint",
                    orbit_blueprint::bundled::BUNDLED_BLUEPRINTS[0].id,
                    orbit_blueprint::bundled::BUNDLED_BLUEPRINTS
                        .iter()
                        .map(|descriptor| Self::form_choice(descriptor.id, descriptor.name))
                        .collect(),
                    true,
                )],
                focused: 0,
                invalid: None,
                error: None,
                dirty: false,
            }),
            FormMode::BindBlueprint { project_id } => {
                let choices = self.blueprint_version_choices();
                if choices.is_empty() {
                    self.notify(
                        "No published blueprint versions are available",
                        StatusKind::Warning,
                    );
                    return None;
                }
                Some(FormState {
                    title: "Bind blueprint version".into(),
                    hint: "Binding is explicit and pins the selected version".into(),
                    mode: FormMode::BindBlueprint {
                        project_id: project_id.clone(),
                    },
                    fields: vec![Self::choice_field(
                        "version",
                        "Blueprint",
                        choices[0].value.clone(),
                        choices,
                        true,
                    )],
                    focused: 0,
                    invalid: None,
                    error: None,
                    dirty: false,
                })
            }
            FormMode::MetricTarget { metric_id } => {
                let choices = self.metric_choices(true);
                let selected = metric_id.unwrap_or_else(|| {
                    choices
                        .get(1)
                        .map(|choice| choice.value.clone())
                        .unwrap_or_default()
                });
                let metric_field = if choices.len() <= 1 {
                    Self::text_field("metric", "Metric ID", selected.clone(), true, "metric ID")
                } else {
                    Self::choice_field("metric", "Metric", selected.clone(), choices, true)
                };
                Some(FormState {
                    title: "Metric target".into(),
                    hint: "Baseline and target are integers".into(),
                    mode: FormMode::MetricTarget {
                        metric_id: Some(selected),
                    },
                    fields: vec![
                        metric_field,
                        Self::integer_field("baseline", "Baseline", "", true, "integer"),
                        Self::integer_field("target", "Target", "", true, "integer"),
                        Self::choice_field(
                            "direction",
                            "Direction",
                            "at_least",
                            ["at_least", "at_most"]
                                .into_iter()
                                .map(|direction| Self::form_choice(direction, direction))
                                .collect(),
                            true,
                        ),
                    ],
                    focused: 0,
                    invalid: None,
                    error: None,
                    dirty: false,
                })
            }
            FormMode::MetricObservation { metric_id } => {
                let choices = self.metric_choices(true);
                let selected = metric_id.unwrap_or_else(|| {
                    choices
                        .get(1)
                        .map(|choice| choice.value.clone())
                        .unwrap_or_default()
                });
                let metric_field = if choices.len() <= 1 {
                    Self::text_field("metric", "Metric ID", selected.clone(), true, "metric ID")
                } else {
                    Self::choice_field("metric", "Metric", selected.clone(), choices, true)
                };
                Some(FormState {
                    title: "Record metric observation".into(),
                    hint: "The observation is appended through the authority".into(),
                    mode: FormMode::MetricObservation {
                        metric_id: Some(selected),
                    },
                    fields: vec![
                        metric_field,
                        Self::integer_field("value", "Value", "", true, "integer"),
                    ],
                    focused: 0,
                    invalid: None,
                    error: None,
                    dirty: false,
                })
            }
            FormMode::BindEvidence { project_id, unbind } => Some(FormState {
                title: if unbind {
                    "Unbind project evidence"
                } else {
                    "Bind project evidence"
                }
                .into(),
                hint: "Use an existing habit or metric ID".into(),
                mode: FormMode::BindEvidence { project_id, unbind },
                fields: vec![
                    Self::choice_field(
                        "kind",
                        "Kind",
                        "habit",
                        ["habit", "metric"]
                            .into_iter()
                            .map(|kind| Self::form_choice(kind, kind))
                            .collect(),
                        true,
                    ),
                    Self::text_field("id", "Evidence ID", "", true, "habit or metric ID"),
                ],
                focused: 0,
                invalid: None,
                error: None,
                dirty: false,
            }),
        }
    }

    pub fn form_value<'a>(form: &'a FormState, key: &str) -> &'a str {
        form.fields
            .iter()
            .find(|field| field.key == key)
            .map(|field| field.value.as_str())
            .unwrap_or("")
    }

    fn form_int(form: &FormState, key: &str, required: bool) -> Result<Option<i64>, String> {
        let value = Self::form_value(form, key).trim();
        if value.is_empty() {
            return if required {
                Err(format!("{key} is required"))
            } else {
                Ok(None)
            };
        }
        value
            .parse::<i64>()
            .map(Some)
            .map_err(|_| format!("{key} must be an integer"))
    }

    fn form_u32(form: &FormState, key: &str) -> Result<Option<u32>, String> {
        match Self::form_int(form, key, false)? {
            Some(value) if (1..=525_600).contains(&value) => Ok(Some(value as u32)),
            Some(_) => Err(format!("{key} must be between 1 and 525600")),
            None => Ok(None),
        }
    }

    fn form_error(&mut self, mut form: FormState, error: impl Into<String>) {
        form.invalid = None;
        form.error = Some(error.into());
        self.modal = Some(Modal::Form(form));
    }

    /// Show a submission failure on the field it belongs to and move focus
    /// there, preserving every typed value.
    fn form_error_with_field(&mut self, mut form: FormState, error: FormError) {
        let field = error
            .field
            .and_then(|key| form.fields.iter().position(|field| field.key == key));
        form.invalid = field;
        if let Some(index) = field {
            form.focused = index;
        }
        form.error = Some(error.message);
        self.modal = Some(Modal::Form(form));
    }

    pub fn submit_form(&mut self, form: FormState) {
        let key = format!("form:{:?}", form.mode);
        if let Some((last, at)) = &self.last_submission
            && last == &key
            && at.elapsed() < Duration::from_millis(250)
        {
            self.notify("Duplicate form submission ignored", StatusKind::Warning);
            self.modal = Some(Modal::Form(form));
            return;
        }
        self.last_submission = Some((key, Instant::now()));
        let mut validation_error: Option<(usize, String)> = None;
        for (index, field) in form.fields.iter().enumerate() {
            if field.required && field.value.trim().is_empty() {
                validation_error = Some((index, format!("{} is required", field.label)));
                break;
            }
            if matches!(field.kind, FormFieldKind::Integer)
                && !field.value.trim().is_empty()
                && field.value.trim().parse::<i64>().is_err()
            {
                validation_error = Some((index, format!("{} must be an integer", field.label)));
                break;
            }
            if let FormFieldKind::Choice(choices) = &field.kind
                && (!field.value.trim().is_empty() || field.required)
                && !choices.iter().any(|choice| choice.value == field.value)
            {
                validation_error = Some((index, format!("{} is not a valid choice", field.label)));
                break;
            }
        }
        // Cross-field planning check stays next to the field that has to change.
        if validation_error.is_none()
            && let (Some(earliest), Some(deadline)) = (
                form.fields
                    .iter()
                    .position(|field| field.key == "earliest")
                    .and_then(|index| {
                        Self::form_value(&form, "earliest")
                            .trim()
                            .parse::<i64>()
                            .ok()
                            .map(|value| (index, value))
                    }),
                form.fields
                    .iter()
                    .position(|field| field.key == "deadline")
                    .and_then(|index| {
                        Self::form_value(&form, "deadline")
                            .trim()
                            .parse::<i64>()
                            .ok()
                            .map(|value| (index, value))
                    }),
            )
            && earliest.1 > deadline.1
        {
            validation_error = Some((
                deadline.0,
                "Earliest start must not be after the deadline".to_owned(),
            ));
        }
        if let Some((index, error)) = validation_error {
            let mut form = form;
            form.invalid = Some(index);
            form.focused = index;
            form.error = Some(error);
            self.modal = Some(Modal::Form(form));
            return;
        }
        let result = match form.mode.clone() {
            FormMode::NewProject => self.submit_new_project(&form).map_err(FormError::from),
            FormMode::NewTask { .. } => self.submit_new_task(&form),
            FormMode::EditTask { task_id } => self.submit_edit_task(&form, &task_id),
            FormMode::QuickEditTask { task_id } => self
                .submit_quick_edit_task(&form, &task_id)
                .map_err(FormError::from),
            FormMode::EditTaskDetails { task_id } => self
                .submit_edit_task_details(&form, &task_id)
                .map_err(FormError::from),
            FormMode::AddCriterion { task_id } => self
                .submit_add_criterion(&form, &task_id)
                .map_err(FormError::from),
            FormMode::EditCriterion {
                task_id,
                criterion_id,
            } => self
                .submit_edit_criterion(&form, &task_id, &criterion_id)
                .map_err(FormError::from),
            FormMode::AttachFile {
                owner_kind,
                owner_id,
            } => self
                .submit_attach_file(&form, &owner_kind, &owner_id)
                .map_err(FormError::from),
            FormMode::ExportAttachment { attachment_id } => self
                .submit_export_attachment(&form, &attachment_id)
                .map_err(FormError::from),
            FormMode::ChangeWorkflow { project_id } => self
                .submit_change_workflow(&form, &project_id)
                .map_err(FormError::from),
            FormMode::AddDependency { task_id } => self
                .submit_add_dependency(&form, &task_id)
                .map_err(FormError::from),
            FormMode::RemoveDependency { task_id } => self
                .submit_remove_dependency(&form, &task_id)
                .map_err(FormError::from),
            FormMode::EditProject { project_id } => self
                .submit_edit_project(&form, &project_id)
                .map_err(FormError::from),
            FormMode::NewWorkItemKind { project_id } => {
                let Some(blueprint) = self
                    .client
                    .project_blueprint_definition(&project_id)
                    .ok()
                    .flatten()
                else {
                    self.form_error(form, "The project has no pinned blueprint");
                    return;
                };
                let kind = Self::form_value(&form, "kind").to_owned();
                if let Some(next) = self.build_form(FormMode::NewWorkItem {
                    project_id,
                    blueprint,
                    kind,
                }) {
                    self.modal = Some(Modal::Form(next));
                }
                return;
            }
            FormMode::NewWorkItem {
                project_id,
                blueprint,
                kind,
            } => self
                .submit_new_work_item(&form, &project_id, &blueprint, &kind)
                .map_err(FormError::from),
            FormMode::EditWorkItem {
                item_id,
                revision,
                blueprint,
                kind,
            } => self
                .submit_edit_work_item(&form, &item_id, revision, &blueprint, &kind)
                .map_err(FormError::from),
            FormMode::NewKnowledge => self.submit_new_knowledge(&form).map_err(FormError::from),
            FormMode::InstallBlueprint => self
                .submit_install_blueprint(&form)
                .map_err(FormError::from),
            FormMode::BindBlueprint { project_id } => self
                .submit_bind_blueprint(&form, &project_id)
                .map_err(FormError::from),
            FormMode::MetricTarget { .. } => {
                self.submit_metric_target(&form).map_err(FormError::from)
            }
            FormMode::MetricObservation { .. } => self
                .submit_metric_observation(&form)
                .map_err(FormError::from),
            FormMode::BindEvidence { project_id, unbind } => self
                .submit_bind_evidence(&form, &project_id, unbind)
                .map_err(FormError::from),
        };
        match result {
            Ok(()) => {
                if self.status.kind != StatusKind::Pending {
                    match &form.mode {
                        // A quick edit only changes one task's title/priority. It
                        // must not trigger a whole-workspace re-read.
                        FormMode::QuickEditTask { task_id } => {
                            let title = Self::form_value(&form, "title").trim().to_owned();
                            let priority = Self::form_value(&form, "priority").trim().to_owned();
                            // Only a title change is fully local to the project
                            // view and the in-memory snapshot. A priority change
                            // can reorder Today/Next, which needs the full
                            // planning refresh.
                            let priority_changed = self
                                .workspace
                                .as_ref()
                                .and_then(|workspace| {
                                    workspace
                                        .tasks
                                        .iter()
                                        .find(|task| task.id == *task_id)
                                        .map(|row| row.priority != priority)
                                })
                                .unwrap_or(true);
                            if priority_changed {
                                self.refresh_after_mutation();
                            } else {
                                self.refresh_after_task_title(task_id, &title);
                            }
                        }
                        _ => self.refresh_after_mutation(),
                    }
                }
                self.reopen_after_mutation(&form.mode);
            }
            Err(error) => self.form_error_with_field(form, error),
        }
    }

    /// Keep list-style modals in sync after a successful form submission.
    fn reopen_after_mutation(&mut self, mode: &FormMode) {
        match mode {
            FormMode::AddCriterion { task_id } | FormMode::EditCriterion { task_id, .. } => {
                let title = self
                    .task_dto(task_id)
                    .map(|task| task.title)
                    .unwrap_or_else(|| task_id.clone());
                self.open_criteria(task_id.clone(), title);
            }
            FormMode::AttachFile {
                owner_kind,
                owner_id,
            } => {
                let title = format!("Attachments · {owner_id}");
                self.open_attachments(owner_kind.clone(), owner_id.clone(), title);
            }
            _ => {}
        }
    }

    /// Quick edit deliberately reuses the same commands as the full forms:
    /// rename and rich-detail update. No new mutation API.
    fn submit_quick_edit_task(&mut self, form: &FormState, task_id: &str) -> Result<(), String> {
        let current = self
            .task_dto(task_id)
            .ok_or_else(|| "task no longer exists".to_owned())?;
        let title = Self::form_value(form, "title").trim().to_owned();
        let priority_raw = Self::form_value(form, "priority").trim().to_owned();
        let priority = (!priority_raw.is_empty()).then_some(priority_raw);
        let mut changed = false;
        if title != current.title {
            self.client
                .rename_task(task_id.to_owned(), title)
                .map_err(|error| error.message)?;
            changed = true;
        }
        if priority != current.priority {
            self.client
                .update_task_details(
                    task_id.to_owned(),
                    current.description.clone(),
                    priority,
                    current.labels.clone(),
                )
                .map_err(|error| error.message)?;
            changed = true;
        }
        if !changed {
            self.notify("No quick-edit changes to save", StatusKind::Info);
            return Ok(());
        }
        let (message, kind) = self.mutation_status("Quick edit saved");
        self.notify(message, kind);
        Ok(())
    }

    fn submit_edit_task_details(&mut self, form: &FormState, task_id: &str) -> Result<(), String> {
        let description = Self::form_value(form, "description").to_owned();
        let priority_raw = Self::form_value(form, "priority").trim().to_owned();
        let priority = (!priority_raw.is_empty()).then_some(priority_raw);
        let labels: Vec<String> = Self::form_value(form, "labels")
            .split(',')
            .map(str::trim)
            .filter(|label| !label.is_empty())
            .map(str::to_owned)
            .collect();
        self.client
            .update_task_details(task_id.to_owned(), description, priority, labels)
            .map_err(|error| error.message)?;
        let (message, kind) = self.mutation_status("Task details saved");
        self.notify(message, kind);
        Ok(())
    }

    fn submit_add_criterion(&mut self, form: &FormState, task_id: &str) -> Result<(), String> {
        let text = Self::form_value(form, "text").trim().to_owned();
        self.client
            .add_acceptance_criterion(task_id.to_owned(), text)
            .map_err(|error| error.message)?;
        let (message, kind) = self.mutation_status("Acceptance criterion added");
        self.notify(message, kind);
        Ok(())
    }

    fn submit_edit_criterion(
        &mut self,
        form: &FormState,
        task_id: &str,
        criterion_id: &str,
    ) -> Result<(), String> {
        let text = Self::form_value(form, "text").trim().to_owned();
        self.client
            .set_acceptance_criterion_text(task_id.to_owned(), criterion_id.to_owned(), text)
            .map_err(|error| error.message)?;
        let (message, kind) = self.mutation_status("Acceptance criterion updated");
        self.notify(message, kind);
        Ok(())
    }

    fn submit_attach_file(
        &mut self,
        form: &FormState,
        owner_kind: &str,
        owner_id: &str,
    ) -> Result<(), String> {
        let path = Self::form_value(form, "path").trim().to_owned();
        if path.is_empty() {
            return Err("File path is required".into());
        }
        let description = Self::form_value(form, "description").trim();
        let description = (!description.is_empty()).then(|| description.to_owned());
        let attachment = self
            .client
            .import_attachment(
                owner_kind.to_owned(),
                owner_id.to_owned(),
                path,
                description,
            )
            .map_err(|error| error.message)?;
        // A connected import may only stage locally. Report pending until a
        // server receipt names this exact attachment as accepted.
        let pending = self
            .client
            .pending_attachments(owner_kind, owner_id)
            .map(|items| items.iter().any(|item| item.attachment.id == attachment.id))
            .unwrap_or(false);
        if pending {
            self.notify(
                format!(
                    "Pending upload: {} (not accepted yet; flush to send it)",
                    attachment.filename
                ),
                StatusKind::Pending,
            );
        } else {
            self.notify(
                format!("Attached {}", attachment.filename),
                StatusKind::Success,
            );
        }
        Ok(())
    }

    fn submit_export_attachment(
        &mut self,
        form: &FormState,
        attachment_id: &str,
    ) -> Result<(), String> {
        let destination = Self::form_value(form, "destination").trim().to_owned();
        if destination.is_empty() {
            return Err("Destination path is required".into());
        }
        let attachment = self
            .client
            .export_attachment(attachment_id, destination.clone())
            .map_err(|error| error.message)?;
        self.notify(
            format!("Exported {} to {destination}", attachment.filename),
            StatusKind::Success,
        );
        Ok(())
    }

    fn submit_change_workflow(&mut self, form: &FormState, project_id: &str) -> Result<(), String> {
        let workflow = Self::form_value(form, "workflow").trim().to_owned();
        if workflow.is_empty() {
            return Err("Select a workflow".into());
        }
        let version = Self::form_int(form, "version", false)?
            .map(|value| u32::try_from(value).map_err(|_| "Version must be positive".to_owned()))
            .transpose()?;
        self.client
            .apply_workflow(project_id.to_owned(), workflow.clone(), version)
            .map_err(|error| error.message)?;
        let label = self.workflow_label(&workflow, version.unwrap_or(1));
        self.notify(format!("Workflow applied: {label}"), StatusKind::Success);
        Ok(())
    }

    fn default_project_signals(mode: &str) -> Vec<String> {
        orbit_domain::ProjectMode::parse(mode)
            .map(|mode| {
                mode.default_signals()
                    .into_iter()
                    .map(|signal| signal.as_str().to_owned())
                    .collect()
            })
            .unwrap_or_default()
    }

    fn submit_new_project(&mut self, form: &FormState) -> Result<(), String> {
        let name = Self::form_value(form, "name").trim().to_owned();
        let workflow = Self::form_value(form, "workflow").trim().to_owned();
        if self.client.is_connected() {
            if !Self::form_value(form, "objective").trim().is_empty() {
                return Err(
                    "connected mode cannot queue project objectives yet; create the project, then use the connected console (`:`, then Connected commands)"
                        .into(),
                );
            }
            if !workflow.is_empty() && workflow != "basic" {
                return Err(
                    "connected mode uses the server default workflow (Basic); change it after the project is accepted"
                        .into(),
                );
            }
        }
        let project = self
            .client
            .create_project(CreateProjectRequest {
                id: None,
                name: name.clone(),
            })
            .map_err(|error| error.message)?;
        if self.client.take_pending() {
            self.refresh_workspace();
            self.notify(
                format!("Project {name} is pending; it is not accepted yet"),
                StatusKind::Pending,
            );
            return Ok(());
        }
        let objective = Self::form_value(form, "objective").trim();
        let mut warning = None;
        if !objective.is_empty()
            && let Err(error) =
                self.client
                    .configure_project_strategy(ConfigureProjectStrategyRequest {
                        id: project.id.clone(),
                        mode: "standard".into(),
                        objective: Some(objective.to_owned()),
                        progress_signals: Self::default_project_signals("standard"),
                        success_criteria: Vec::new(),
                    })
        {
            warning = Some(format!("objective update failed: {}", error.message));
        }
        // New projects already carry the Basic binding from the authority; only
        // an explicit different workflow needs an extra pinned change.
        if !workflow.is_empty()
            && workflow != "basic"
            && let Err(error) =
                self.client
                    .apply_workflow(project.id.clone(), workflow.clone(), None)
        {
            warning = Some(format!("applying the workflow failed: {}", error.message));
        }
        let id = project.id.clone();
        self.refresh_workspace();
        self.open_project(&id);
        if let Some(warning) = warning {
            self.notify(
                format!("Created project {name}; {warning}"),
                StatusKind::Warning,
            );
        } else {
            self.notify(format!("Created project {name}"), StatusKind::Success);
        }
        Ok(())
    }

    fn submit_new_task(&mut self, form: &FormState) -> Result<(), FormError> {
        let title = Self::form_value(form, "title").trim().to_owned();
        let project = Self::form_value(form, "project").to_owned();
        let project_id = (!project.is_empty()).then_some(project);
        let milestone = Self::form_value(form, "milestone").trim();
        let estimate = Self::form_u32(form, "estimate")
            .map_err(|message| FormError::field("estimate", message))?;
        let earliest = Self::form_int(form, "earliest", false)
            .map_err(|message| FormError::field("earliest", message))?;
        let deadline = Self::form_int(form, "deadline", false)
            .map_err(|message| FormError::field("deadline", message))?;
        if earliest.is_some_and(|value| value < 0) {
            return Err(FormError::field(
                "earliest",
                "Earliest start must be non-negative",
            ));
        }
        if deadline.is_some_and(|value| value < 0) {
            return Err(FormError::field(
                "deadline",
                "Deadline must be non-negative",
            ));
        }
        if let (Some(earliest), Some(deadline)) = (earliest, deadline)
            && earliest > deadline
        {
            return Err(FormError::field(
                "deadline",
                "Earliest start must not be after the deadline",
            ));
        }
        let task = self
            .client
            .create_task(CreateTaskRequest {
                id: None,
                title,
                project_id,
                milestone_id: (!milestone.is_empty()).then(|| milestone.to_owned()),
            })
            .map_err(|error| error.message)?;
        if self.client.take_pending() {
            if estimate.is_some() || earliest.is_some() || deadline.is_some() {
                if let Err(error) = self.client.set_task_planning(SetTaskPlanningRequest {
                    id: task.id.clone(),
                    estimated_minutes: estimate,
                    earliest_start_at_ms: earliest,
                    deadline_at_ms: deadline,
                }) {
                    self.refresh_after_mutation();
                    self.notify(
                        format!(
                            "Task {} is pending but planning was rejected: {}",
                            task.id, error.message
                        ),
                        StatusKind::Warning,
                    );
                    return Ok(());
                }
                let _ = self.client.take_pending();
            }
            self.refresh_after_mutation();
            self.notify(
                format!("Task {} is pending; it is not accepted yet", task.id),
                StatusKind::Pending,
            );
            return Ok(());
        }
        if (estimate.is_some() || earliest.is_some() || deadline.is_some())
            && let Err(error) = self.client.set_task_planning(SetTaskPlanningRequest {
                id: task.id.clone(),
                estimated_minutes: estimate,
                earliest_start_at_ms: earliest,
                deadline_at_ms: deadline,
            })
        {
            self.notify(
                format!(
                    "Created task {} but planning was rejected: {}",
                    task.id, error.message
                ),
                StatusKind::Warning,
            );
            return Ok(());
        }
        self.notify(format!("Created task {}", task.id), StatusKind::Success);
        Ok(())
    }

    fn submit_edit_task(&mut self, form: &FormState, task_id: &str) -> Result<(), FormError> {
        let title = Self::form_value(form, "title").trim().to_owned();
        let current = self
            .client
            .list_tasks()
            .map_err(|error| error.message)?
            .into_iter()
            .find(|task| task.id == task_id)
            .ok_or_else(|| "task no longer exists".to_owned())?;
        let estimate = Self::form_u32(form, "estimate")
            .map_err(|message| FormError::field("estimate", message))?;
        let earliest = Self::form_int(form, "earliest", false)
            .map_err(|message| FormError::field("earliest", message))?;
        let deadline = Self::form_int(form, "deadline", false)
            .map_err(|message| FormError::field("deadline", message))?;
        if earliest.is_some_and(|value| value < 0) {
            return Err(FormError::field(
                "earliest",
                "Earliest start must be non-negative",
            ));
        }
        if deadline.is_some_and(|value| value < 0) {
            return Err(FormError::field(
                "deadline",
                "Deadline must be non-negative",
            ));
        }
        if let (Some(earliest), Some(deadline)) = (earliest, deadline)
            && earliest > deadline
        {
            return Err(FormError::field(
                "deadline",
                "Earliest start must not be after the deadline",
            ));
        }
        let project = Self::form_value(form, "project");
        let project_id = (!project.is_empty()).then(|| project.to_owned());
        let milestone = Self::form_value(form, "milestone").trim();
        let milestone_id = (!milestone.is_empty()).then(|| milestone.to_owned());
        if project_id == current.project_id
            && let Some(milestone_id) = &milestone_id
            && let Some(project) = &self.project
            && project.id == current.project_id.as_deref().unwrap_or("")
            && !project
                .data
                .milestones
                .iter()
                .any(|milestone| &milestone.id == milestone_id)
        {
            return Err(format!("milestone {milestone_id} is not in this project").into());
        }
        if project_id != current.project_id {
            if let Some(project_id) = &project_id
                && !self
                    .projects
                    .iter()
                    .any(|project| &project.id == project_id)
            {
                return Err(format!("project {project_id} is not available").into());
            }
            if current.milestone_id.is_some() {
                self.client
                    .assign_task_milestone(task_id.to_owned(), None)
                    .map_err(|error| error.message)?;
            }
            self.client
                .assign_task_project(task_id.to_owned(), project_id)
                .map_err(|error| error.message)?;
        }
        if milestone_id != current.milestone_id {
            self.client
                .assign_task_milestone(task_id.to_owned(), milestone_id)
                .map_err(|error| error.message)?;
        }
        if current.title != title {
            self.client
                .rename_task(task_id.to_owned(), title)
                .map_err(|error| error.message)?;
        }
        if estimate != current.estimated_minutes
            || earliest != current.earliest_start_at_ms
            || deadline != current.deadline_at_ms
        {
            self.client
                .set_task_planning(SetTaskPlanningRequest {
                    id: task_id.to_owned(),
                    estimated_minutes: estimate,
                    earliest_start_at_ms: earliest,
                    deadline_at_ms: deadline,
                })
                .map_err(|error| error.message)?;
        }
        let description = Self::form_value(form, "description").to_owned();
        let priority_raw = Self::form_value(form, "priority").trim().to_owned();
        let priority = (!priority_raw.is_empty()).then_some(priority_raw);
        let labels: Vec<String> = Self::form_value(form, "labels")
            .split(',')
            .map(str::trim)
            .filter(|label| !label.is_empty())
            .map(str::to_owned)
            .collect();
        self.client
            .update_task_details(task_id.to_owned(), description, priority, labels)
            .map_err(|error| error.message)?;
        let (message, kind) = self.mutation_status("Task updated");
        self.notify(message, kind);
        Ok(())
    }

    fn submit_add_dependency(&mut self, form: &FormState, task_id: &str) -> Result<(), String> {
        let prerequisite = Self::form_value(form, "prerequisite").to_owned();
        self.client
            .add_task_prerequisite(task_id.to_owned(), prerequisite)
            .map_err(|error| error.message)?;
        let (message, kind) = self.mutation_status("Task prerequisite added");
        self.notify(message, kind);
        Ok(())
    }

    fn submit_remove_dependency(&mut self, form: &FormState, task_id: &str) -> Result<(), String> {
        let prerequisite = Self::form_value(form, "prerequisite").to_owned();
        self.client
            .remove_task_prerequisite(task_id.to_owned(), prerequisite)
            .map_err(|error| error.message)?;
        let (message, kind) = self.mutation_status("Task prerequisite removed");
        self.notify(message, kind);
        Ok(())
    }

    fn submit_edit_project(&mut self, form: &FormState, project_id: &str) -> Result<(), String> {
        let project = self
            .project_details
            .iter()
            .find(|project| project.id == project_id)
            .cloned()
            .ok_or_else(|| "project not found".to_owned())?;
        let status = Self::form_value(form, "status").to_owned();
        let mode = Self::form_value(form, "mode").to_owned();
        let objective = {
            let objective = Self::form_value(form, "objective").trim();
            (!objective.is_empty()).then(|| objective.to_owned())
        };
        let mode_changed = mode != project.mode;
        let objective_changed = objective != project.objective;
        if mode_changed || objective_changed {
            let signals = if mode_changed {
                Self::default_project_signals(&mode)
            } else {
                project.progress_signals.clone()
            };
            self.client
                .configure_project_strategy(ConfigureProjectStrategyRequest {
                    id: project_id.to_owned(),
                    mode,
                    objective,
                    progress_signals: signals,
                    success_criteria: project
                        .success_criteria
                        .iter()
                        .map(|criterion| ProjectCriterionInputDto {
                            key: criterion.key.clone(),
                            description: criterion.description.clone(),
                            required: criterion.required,
                            status: criterion.status.clone(),
                        })
                        .collect(),
                })
                .map_err(|error| error.message)?;
        }
        let mut warning = None;
        if status != project.status
            && let Err(error) = self.client.set_project_status(SetProjectStatusRequest {
                id: project_id.to_owned(),
                status,
            })
        {
            warning = Some(error.message);
        }
        if let Some(warning) = warning {
            self.notify(
                format!("Project settings partially applied; status update failed: {warning}"),
                StatusKind::Warning,
            );
        } else {
            self.notify("Project updated", StatusKind::Success);
        }
        Ok(())
    }

    fn submit_new_work_item(
        &mut self,
        form: &FormState,
        project_id: &str,
        blueprint: &Blueprint,
        kind: &str,
    ) -> Result<(), String> {
        let definition = blueprint
            .item_kind(kind)
            .map_err(|error| error.to_string())?;
        let mut properties = BTreeMap::new();
        for field in &form.fields {
            let value = field.value.trim();
            if value.is_empty() {
                continue;
            }
            let definition = definition
                .fields
                .iter()
                .find(|candidate| candidate.id == field.key)
                .ok_or_else(|| format!("undeclared field {}", field.key))?;
            let value = match definition.kind {
                FieldKind::Integer => serde_json::Value::from(
                    value
                        .parse::<i64>()
                        .map_err(|_| format!("{} must be an integer", field.label))?,
                ),
                FieldKind::Boolean => serde_json::Value::Bool(value == "true"),
                FieldKind::Text | FieldKind::Choice { .. } => {
                    serde_json::Value::String(value.to_owned())
                }
            };
            properties.insert(field.key.clone(), value);
        }
        self.client
            .create_blueprint_work_item(project_id, kind, properties)
            .map_err(|error| error.message)?;
        self.notify(format!("Created {kind}"), StatusKind::Success);
        Ok(())
    }

    fn submit_edit_work_item(
        &mut self,
        form: &FormState,
        item_id: &str,
        revision: u64,
        blueprint: &Blueprint,
        kind: &str,
    ) -> Result<(), String> {
        let definition = blueprint
            .item_kind(kind)
            .map_err(|error| error.to_string())?;
        let mut changes = BTreeMap::new();
        for field in &form.fields {
            if field.value == field.initial {
                continue;
            }
            let value = field.value.trim();
            let definition = definition
                .fields
                .iter()
                .find(|candidate| candidate.id == field.key)
                .ok_or_else(|| format!("undeclared field {}", field.key))?;
            let value = match definition.kind {
                FieldKind::Integer => serde_json::Value::from(
                    value
                        .parse::<i64>()
                        .map_err(|_| format!("{} must be an integer", field.label))?,
                ),
                FieldKind::Boolean => serde_json::Value::Bool(value == "true"),
                FieldKind::Text | FieldKind::Choice { .. } => {
                    serde_json::Value::String(value.to_owned())
                }
            };
            changes.insert(field.key.clone(), value);
        }
        if changes.is_empty() {
            self.notify("No work-item changes to save", StatusKind::Info);
            return Ok(());
        }
        self.client
            .update_blueprint_work_item(item_id, revision, changes)
            .map_err(|error| error.message)?;
        self.notify("Work item updated", StatusKind::Success);
        Ok(())
    }

    fn submit_new_knowledge(&mut self, form: &FormState) -> Result<(), String> {
        let title = Self::form_value(form, "title").trim().to_owned();
        self.client
            .create_knowledge(CreateKnowledgeRequest {
                id: None,
                kind: "note".into(),
                title,
                body_markdown: Self::form_value(form, "body").to_owned(),
            })
            .map_err(|error| error.message)?;
        self.notify("Knowledge note created", StatusKind::Success);
        Ok(())
    }

    fn submit_install_blueprint(&mut self, form: &FormState) -> Result<(), String> {
        let id = Self::form_value(form, "blueprint");
        self.client
            .install_bundled_blueprint(id)
            .map_err(|error| error.message)?;
        self.notify(
            format!("Installed bundled blueprint {id}"),
            StatusKind::Success,
        );
        Ok(())
    }

    fn submit_bind_blueprint(&mut self, form: &FormState, project_id: &str) -> Result<(), String> {
        let value = Self::form_value(form, "version");
        let (id, version) = value
            .split_once('@')
            .ok_or_else(|| "invalid blueprint version".to_owned())?;
        let version = version
            .parse::<u32>()
            .map_err(|_| "blueprint version must be an integer".to_owned())?;
        self.client
            .bind_project_blueprint(project_id, id, version)
            .map_err(|error| error.message)?;
        self.notify(format!("Bound {id}@{version}"), StatusKind::Success);
        Ok(())
    }

    fn submit_metric_target(&mut self, form: &FormState) -> Result<(), String> {
        let metric_id = Self::form_value(form, "metric").to_owned();
        let baseline = Self::form_int(form, "baseline", true)?.unwrap();
        let target = Self::form_int(form, "target", true)?.unwrap();
        self.client
            .set_metric_target(MetricTargetDto {
                metric_id,
                baseline,
                target,
                direction: Self::form_value(form, "direction").to_owned(),
            })
            .map_err(|error| error.message)?;
        self.notify("Metric target accepted", StatusKind::Success);
        Ok(())
    }

    fn submit_metric_observation(&mut self, form: &FormState) -> Result<(), String> {
        let metric_id = Self::form_value(form, "metric").to_owned();
        let value = Self::form_int(form, "value", true)?.unwrap();
        self.client
            .record_metric_observation(metric_id, now_ms(), value)
            .map_err(|error| error.message)?;
        self.notify("Metric observation accepted", StatusKind::Success);
        Ok(())
    }

    fn submit_bind_evidence(
        &mut self,
        form: &FormState,
        project_id: &str,
        unbind: bool,
    ) -> Result<(), String> {
        self.client
            .change_project_evidence(
                project_id.to_owned(),
                Self::form_value(form, "kind").to_owned(),
                Self::form_value(form, "id").to_owned(),
                !unbind,
            )
            .map_err(|error| error.message)?;
        self.notify(
            if unbind {
                "Evidence unbound"
            } else {
                "Evidence bound"
            },
            StatusKind::Success,
        );
        Ok(())
    }
}
