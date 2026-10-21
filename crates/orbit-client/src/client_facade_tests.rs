//! Embedded client facade regression suites, separated from production entry points.
use super::*;

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_db(name: &str) -> std::path::PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "orbit-client-{name}-{}-{}.db",
            process::id(),
            CLIENT_ID_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_file(&path);
        path
    }

    #[test]
    fn schedule_create_conflict_move_and_reopen_preserves_history() {
        let path = temp_db("schedule-lifecycle");
        let mut client = OrbitClient::open(&path).unwrap();
        let request = |id: &str, start: i64, flexibility: &str| CreateScheduleBlockRequest {
            id: Some(id.into()),
            title: id.into(),
            starts_at_ms: start,
            duration_minutes: 30,
            flexibility: flexibility.into(),
            target_kind: "standalone".into(),
            target_id: None,
        };
        let first = client
            .create_schedule_block_without_conflict(request("s1", 1_000_000, "movable"))
            .unwrap();
        assert_eq!(first.ends_at_ms, 2_800_000);
        assert!(
            client
                .create_schedule_block_without_conflict(request("s2", 2_000_000, "fixed"))
                .is_err()
        );
        let second = client
            .create_schedule_block_without_conflict(request("s2", 2_800_000, "fixed"))
            .unwrap();
        assert_eq!(second.starts_at_ms, first.ends_at_ms);
        assert_eq!(
            client
                .schedule_conflicts(1_500_000, 3_000_000, None)
                .unwrap()
                .len(),
            2
        );
        assert!(
            client
                .reschedule_schedule_block("s1".into(), 3_000_000)
                .is_err()
        );
        assert_eq!(
            client
                .reschedule_schedule_block("s1".into(), 4_600_000)
                .unwrap()
                .starts_at_ms,
            4_600_000
        );
        assert!(
            client
                .reschedule_schedule_block("s2".into(), 7_000_000)
                .is_err()
        );
        drop(client);
        let reopened = OrbitClient::open(&path).unwrap();
        let schedule = reopened.list_schedule(0, 10_000_000).unwrap();
        assert_eq!(schedule.len(), 2);
        assert_eq!(
            schedule.iter().find(|b| b.id == "s1").unwrap().starts_at_ms,
            4_600_000
        );
        assert_eq!(
            schedule.iter().find(|b| b.id == "s2").unwrap().starts_at_ms,
            2_800_000
        );
        drop(reopened);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn capabilities_make_mcp_mobile_exclusion_explicit() {
        let caps = ClientCapabilities::default();
        assert!(caps.embedded_runtime);
        assert!(caps.structured_knowledge);
        assert!(caps.durable_change_cursor);
        assert!(caps.external_secure_secret_store);
        assert!(!caps.mobile_mcp);
    }

    #[test]
    fn client_mutations_are_visible_through_durable_change_cursor() {
        let path = temp_db("changes");
        let mut client = OrbitClient::open(&path).expect("open client");
        let cursor = client.current_change_cursor().expect("initial cursor");

        let project = client
            .create_project(CreateProjectRequest {
                id: Some("project-1".into()),
                name: "Orbit".into(),
            })
            .expect("create project");
        assert_eq!(project.status, "proposed");
        let task = client
            .create_task(CreateTaskRequest {
                id: Some("task-1".into()),
                title: "Wire Flutter".into(),
                project_id: Some(project.id),
                milestone_id: None,
            })
            .expect("create task");
        assert_eq!(task.status, "inbox");

        let batch = client.poll_changes(cursor).expect("poll changes");
        assert_eq!(batch.changes.len(), 2);
        assert_eq!(batch.changes[0].subject_kind, "project");
        assert_eq!(batch.changes[1].subject_kind, "task");
        assert_eq!(batch.cursor.activity_count, 2);
        drop(client);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn platform_capabilities_keep_mcp_desktop_only() {
        let mobile = ClientPlatformCapabilities::for_platform(ClientPlatform::Ios);
        let desktop = ClientPlatformCapabilities::for_platform(ClientPlatform::Macos);
        assert!(!mobile.mcp_available);
        assert!(!mobile.local_ai_endpoint_supported);
        assert!(desktop.mcp_available);
        assert!(desktop.local_ai_endpoint_supported);
        assert!(mobile.external_secure_secret_store_required);
    }

    #[test]
    fn secret_references_are_validated_without_storing_secret_material() {
        let reference =
            ClientSecretRef::new("ai-provider", "anthropic-default").expect("valid ref");
        assert_eq!(reference.namespace, "ai-provider");
        assert_eq!(reference.key, "anthropic-default");
        assert_eq!(
            ClientSecretRef::new(" ", "key").unwrap_err().code,
            ClientErrorCode::InvalidInput
        );
    }

    #[test]
    fn change_cursor_survives_reopening_the_workspace() {
        let path = temp_db("reopen");
        let cursor = {
            let mut client = OrbitClient::open(&path).expect("open client");
            client
                .create_knowledge(CreateKnowledgeRequest {
                    id: Some("knowledge-1".into()),
                    kind: "decision".into(),
                    title: "Use Rust".into(),
                    body_markdown: "Because local-first.".into(),
                })
                .expect("create knowledge");
            client.current_change_cursor().expect("cursor")
        };
        let reopened = OrbitClient::open(&path).expect("reopen client");
        let batch = reopened
            .poll_changes(ClientChangeCursor::default())
            .expect("poll from beginning");
        assert_eq!(batch.cursor, cursor);
        assert_eq!(batch.changes.len(), 1);
        assert_eq!(batch.changes[0].subject_kind, "knowledge");
        drop(reopened);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn reviewed_plan_requires_confirmation_and_creates_only_approved_blocks() {
        let path = temp_db("approved-plan");
        let mut client = OrbitClient::open(&path).expect("open");
        client
            .create_task(CreateTaskRequest {
                id: Some("approval-task".into()),
                title: "Review first".into(),
                project_id: None,
                milestone_id: None,
            })
            .unwrap();
        client
            .set_task_planning(SetTaskPlanningRequest {
                id: "approval-task".into(),
                estimated_minutes: Some(30),
                earliest_start_at_ms: None,
                deadline_at_ms: None,
            })
            .unwrap();
        let proposal = client
            .preview_task_plan(0, 7_200_000, PlanningPolicyDto::default())
            .unwrap();
        let reviewed = client.review_task_plan(proposal.clone()).unwrap();
        assert_eq!(reviewed.review.scheduled_tasks, vec!["approval-task"]);
        assert!(reviewed.review.requires_explicit_approval);
        assert!(client.list_schedule(0, 7_200_000).unwrap().is_empty());
        let revision = client.runtime.snapshot().unwrap().0;
        assert!(client.approve_task_plan(reviewed, false).is_err());
        assert_eq!(client.runtime.snapshot().unwrap().0, revision);
        assert!(client.list_schedule(0, 7_200_000).unwrap().is_empty());
        let reviewed = client.review_task_plan(proposal).unwrap();
        let created = client.approve_task_plan(reviewed, true).unwrap();
        assert_eq!(created.len(), 1);
        assert_eq!(client.runtime.snapshot().unwrap().0, revision + 1);
        assert_eq!(client.list_schedule(0, 7_200_000).unwrap().len(), 1);
        drop(client);
        let reopened = OrbitClient::open(&path).unwrap();
        assert_eq!(reopened.list_schedule(0, 7_200_000).unwrap().len(), 1);
        drop(reopened);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn reviewed_plan_rejects_intervening_authoritative_change() {
        let path = temp_db("stale-approved-plan");
        let mut client = OrbitClient::open(&path).unwrap();
        client
            .create_task(CreateTaskRequest {
                id: Some("stale-task".into()),
                title: "Plan later".into(),
                project_id: None,
                milestone_id: None,
            })
            .unwrap();
        client
            .set_task_planning(SetTaskPlanningRequest {
                id: "stale-task".into(),
                estimated_minutes: Some(30),
                earliest_start_at_ms: None,
                deadline_at_ms: None,
            })
            .unwrap();
        let proposal = client
            .preview_task_plan(0, 7_200_000, PlanningPolicyDto::default())
            .unwrap();
        let prepared = client.review_task_plan(proposal).unwrap();
        client
            .create_task(CreateTaskRequest {
                id: Some("intervening-task".into()),
                title: "Concurrent write".into(),
                project_id: None,
                milestone_id: None,
            })
            .unwrap();
        let before = client.runtime.snapshot().unwrap().0;
        assert!(client.approve_task_plan(prepared, true).is_err());
        assert_eq!(client.runtime.snapshot().unwrap().0, before);
        assert!(client.list_schedule(0, 7_200_000).unwrap().is_empty());
        drop(client);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn daily_workflow_client_can_preview_and_apply_task_plan() {
        let path = temp_db("task-plan");
        let mut client = OrbitClient::open(&path).expect("open client");
        client
            .create_task(CreateTaskRequest {
                id: Some("task-plan-1".into()),
                title: "Deep work".into(),
                project_id: None,
                milestone_id: None,
            })
            .expect("create task");
        client
            .set_task_planning(SetTaskPlanningRequest {
                id: "task-plan-1".into(),
                estimated_minutes: Some(30),
                earliest_start_at_ms: None,
                deadline_at_ms: None,
            })
            .expect("set planning");
        let proposal = client
            .preview_task_plan(0, 120 * 60_000, PlanningPolicyDto::default())
            .expect("preview");
        assert_eq!(proposal.scheduled.len(), 1);
        assert_eq!(proposal.scheduled[0].task_id, "task-plan-1");
        let before_apply = client.runtime.snapshot().expect("before plan").0;
        let created = client.apply_task_plan(proposal).expect("apply");
        assert_eq!(
            client.runtime.snapshot().expect("after plan").0,
            before_apply + 1
        );
        assert_eq!(created.len(), 1);
        assert_eq!(created[0].target_kind, "task");
        assert_eq!(
            client
                .list_schedule(0, 120 * 60_000)
                .expect("schedule")
                .len(),
            1
        );
        drop(client);
        let reopened = OrbitClient::open(&path).expect("reopen planned workspace");
        assert_eq!(
            reopened
                .list_schedule(0, 120 * 60_000)
                .expect("durable schedule")
                .len(),
            1
        );
        assert_eq!(
            reopened
                .runtime
                .snapshot()
                .expect("durable accepted history")
                .0,
            before_apply + 1
        );
        drop(reopened);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn habit_history_schedule_and_habit_plan_are_exposed_through_client() {
        let path = temp_db("habit-plan");
        let mut client = OrbitClient::open(&path).expect("open client");
        let habit = client
            .create_habit(CreateHabitRequest {
                id: Some("habit-1".into()),
                name: "Run".into(),
                cadence: "daily".into(),
            })
            .expect("create habit");
        assert!(habit.active);
        client
            .record_habit_occurrence(RecordHabitOccurrenceRequest {
                id: Some("occ-1".into()),
                habit_id: habit.id.clone(),
                occurred_at_ms: Some(10),
                duration_minutes: Some(25),
            })
            .expect("record occurrence");
        assert_eq!(
            client
                .habit_history(habit.id.clone())
                .expect("history")
                .len(),
            1
        );

        let proposal = client
            .preview_habit_plan(
                100,
                120 * 60_000,
                0,
                50,
                vec![HabitPlanInputDto {
                    habit_id: habit.id.clone(),
                    completed_in_period: 1,
                    duration_per_occurrence_minutes: 20,
                }],
                PlanningPolicyDto::default(),
            )
            .expect("preview fulfilled habit");
        assert!(proposal.scheduled.is_empty());
        assert_eq!(proposal.unscheduled[0].reason, "no_remaining_completions");
        drop(client);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn project_strategy_round_trips_through_client_v2() {
        let path = temp_db("project-strategy");
        let mut client = OrbitClient::open(&path).expect("open client");
        let project = client
            .create_project(CreateProjectRequest {
                id: Some("learn-japanese".into()),
                name: "Learn Japanese".into(),
            })
            .unwrap();
        let configured = client
            .configure_project_strategy(ConfigureProjectStrategyRequest {
                id: project.id,
                mode: "learning".into(),
                objective: Some("Pass JLPT N2".into()),
                progress_signals: vec![
                    "milestones".into(),
                    "learning_evidence".into(),
                    "habit_consistency".into(),
                ],
                success_criteria: vec![ProjectCriterionInputDto {
                    key: "mock".into(),
                    description: "Pass an N2 mock exam".into(),
                    required: true,
                    status: "pending".into(),
                }],
            })
            .unwrap();
        assert_eq!(configured.mode, "learning");
        assert!(!configured.completion_ready);
        let updated = client
            .set_project_criterion_status(SetProjectCriterionStatusRequest {
                project_id: configured.id,
                criterion_key: "mock".into(),
                status: "satisfied".into(),
            })
            .unwrap();
        assert!(updated.completion_ready);
        drop(client);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn bridge_contract_is_versioned_and_requires_refresh_after_sync_pull() {
        let path = temp_db("bridge-contract");
        let client = OrbitClient::open(&path).expect("open client");
        let contract = client.bridge_contract();
        assert_eq!(contract.api_version, 4);
        assert!(contract.primitive_dtos_only);
        assert!(contract.embedded_runtime);
        assert!(contract.refresh_after_sync_pull_required);
        assert!(contract.mobile_mcp_excluded);
        drop(client);
        let _ = fs::remove_file(path);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectAgentPlaybookDto {
    pub mode: String,
    pub roadmap_focus: Vec<String>,
    pub evidence_focus: Vec<String>,
    pub review_questions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectRoadmapMilestoneDto {
    pub title: String,
    pub rationale: String,
    pub success_criteria: Vec<String>,
    pub dependencies: Vec<String>,
    pub suggested_target_at_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectRoadmapProposalDto {
    pub mode: String,
    pub objective: String,
    pub assumptions: Vec<String>,
    pub success_criteria: Vec<String>,
    pub milestones: Vec<ProjectRoadmapMilestoneDto>,
    pub suggested_habits: Vec<String>,
    pub suggested_metrics: Vec<String>,
    pub risks: Vec<String>,
    pub suggested_review_cadence: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectCriterionAssessmentDto {
    pub key: String,
    pub satisfied: bool,
    pub evidence: Vec<String>,
    pub rationale: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectCompletionAssessmentDto {
    pub criteria: Vec<ProjectCriterionAssessmentDto>,
    pub ai_recommends_completion: bool,
    pub deterministic_completion_ready: bool,
    pub missing_required_criteria: Vec<String>,
}

impl OrbitClient {
    pub fn project_agent_playbook(
        &self,
        project_id: String,
    ) -> Result<ProjectAgentPlaybookDto, ClientError> {
        let project_id = ProjectId::new(project_id).map_err(invalid_input)?;
        let project = self
            .runtime
            .application_queries()
            .project(&project_id)
            .map_err(ClientError::from)?;
        let playbook = ProjectAgentPlaybook::for_mode(project.mode());
        Ok(ProjectAgentPlaybookDto {
            mode: playbook.mode.as_str().to_owned(),
            roadmap_focus: playbook
                .roadmap_focus
                .into_iter()
                .map(str::to_owned)
                .collect(),
            evidence_focus: playbook
                .evidence_focus
                .into_iter()
                .map(str::to_owned)
                .collect(),
            review_questions: playbook
                .review_questions
                .into_iter()
                .map(str::to_owned)
                .collect(),
        })
    }

    pub fn parse_ai_project_roadmap(
        &self,
        text: String,
    ) -> Result<ProjectRoadmapProposalDto, ClientError> {
        let proposal = parse_project_roadmap_proposal(&text).map_err(|error| ClientError {
            code: ClientErrorCode::InvalidInput,
            message: format!("invalid AI roadmap proposal: {error:?}"),
        })?;
        Ok(ProjectRoadmapProposalDto {
            mode: proposal.mode.as_str().to_owned(),
            objective: proposal.objective,
            assumptions: proposal.assumptions,
            success_criteria: proposal.project_success_criteria,
            milestones: proposal
                .milestones
                .into_iter()
                .map(|milestone| ProjectRoadmapMilestoneDto {
                    title: milestone.title,
                    rationale: milestone.rationale,
                    success_criteria: milestone.success_criteria,
                    dependencies: milestone.dependencies,
                    suggested_target_at_ms: milestone.suggested_target_at_ms,
                })
                .collect(),
            suggested_habits: proposal.suggested_habits,
            suggested_metrics: proposal.suggested_metrics,
            risks: proposal.risks,
            suggested_review_cadence: proposal.suggested_review_cadence,
        })
    }

    pub fn validate_ai_project_completion(
        &self,
        project_id: String,
        text: String,
    ) -> Result<ProjectCompletionAssessmentDto, ClientError> {
        let project_id = ProjectId::new(project_id).map_err(invalid_input)?;
        let project = self
            .runtime
            .application_queries()
            .project(&project_id)
            .map_err(ClientError::from)?;
        let assessment =
            parse_project_completion_assessment(&project, &text).map_err(|error| ClientError {
                code: ClientErrorCode::InvalidInput,
                message: format!("invalid AI completion assessment: {error:?}"),
            })?;
        Ok(ProjectCompletionAssessmentDto {
            criteria: assessment
                .criteria
                .into_iter()
                .map(|criterion| ProjectCriterionAssessmentDto {
                    key: criterion.key,
                    satisfied: criterion.satisfied,
                    evidence: criterion.evidence,
                    rationale: criterion.rationale,
                })
                .collect(),
            ai_recommends_completion: assessment.ai_recommends_completion,
            deterministic_completion_ready: assessment.deterministic_completion_ready,
            missing_required_criteria: assessment.missing_required_criteria,
        })
    }
}

// ---------------------------------------------------------------------------
// Embedded AI/BYOK bridge DTOs. These keep provider protocols and raw secrets
// out of Flutter while preserving an explicit disclosure-before-call flow.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClientAiProviderKind {
    OpenAi,
    Anthropic,
    Gemini,
    OpenAiCompatible,
    Ollama,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClientAiDataCategory {
    ProjectMetadata,
    TaskTitles,
    TaskPlanning,
    Schedule,
    HabitMetadata,
    KnowledgeMetadata,
    KnowledgeBodies,
    ResourceMetadata,
    ActivityHistory,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClientAiProjectAccessMode {
    Disabled,
    LocalOnly,
    SelectedProviders,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientAiAuthMode {
    None,
    Bearer {
        secret: ClientSecretRef,
    },
    ApiKeyHeader {
        header: String,
        secret: ClientSecretRef,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientAiCapabilitiesDto {
    pub chat: bool,
    pub tools: bool,
    pub structured_output: bool,
    pub embeddings: bool,
    pub vision: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientAiProviderConfigDto {
    pub id: String,
    pub kind: ClientAiProviderKind,
    pub endpoint: String,
    pub model: String,
    pub auth: ClientAiAuthMode,
    pub capabilities: ClientAiCapabilitiesDto,
    pub local_only: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientAiProjectPolicyDto {
    pub mode: ClientAiProjectAccessMode,
    pub allowed_remote_providers: Vec<ClientAiProviderKind>,
    pub allowed_categories: Vec<ClientAiDataCategory>,
    pub require_remote_confirmation: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientAiProjectPolicyEntryDto {
    pub project_id: String,
    pub policy: ClientAiProjectPolicyDto,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientAiAccessPolicyDto {
    pub default_policy: ClientAiProjectPolicyDto,
    pub project_policies: Vec<ClientAiProjectPolicyEntryDto>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientAiContextFragmentDto {
    pub project_id: Option<String>,
    pub source_id: String,
    pub category: ClientAiDataCategory,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientAiOmissionDto {
    pub source_id: String,
    pub category: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientAiDisclosureDto {
    pub provider_id: String,
    pub provider_kind: String,
    pub destination: String,
    pub included_source_ids: Vec<String>,
    pub included_categories: Vec<String>,
    pub included_project_ids: Vec<String>,
    pub omitted: Vec<ClientAiOmissionDto>,
    pub requires_confirmation: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientAiResponseDto {
    pub disclosure: ClientAiDisclosureDto,
    pub text: String,
}

fn client_ai_kind(kind: ClientAiProviderKind) -> AiProviderKind {
    match kind {
        ClientAiProviderKind::OpenAi => AiProviderKind::OpenAi,
        ClientAiProviderKind::Anthropic => AiProviderKind::Anthropic,
        ClientAiProviderKind::Gemini => AiProviderKind::Gemini,
        ClientAiProviderKind::OpenAiCompatible => AiProviderKind::OpenAiCompatible,
        ClientAiProviderKind::Ollama => AiProviderKind::Ollama,
    }
}

fn client_ai_category(category: ClientAiDataCategory) -> AiDataCategory {
    match category {
        ClientAiDataCategory::ProjectMetadata => AiDataCategory::ProjectMetadata,
        ClientAiDataCategory::TaskTitles => AiDataCategory::TaskTitles,
        ClientAiDataCategory::TaskPlanning => AiDataCategory::TaskPlanning,
        ClientAiDataCategory::Schedule => AiDataCategory::Schedule,
        ClientAiDataCategory::HabitMetadata => AiDataCategory::HabitMetadata,
        ClientAiDataCategory::KnowledgeMetadata => AiDataCategory::KnowledgeMetadata,
        ClientAiDataCategory::KnowledgeBodies => AiDataCategory::KnowledgeBodies,
        ClientAiDataCategory::ResourceMetadata => AiDataCategory::ResourceMetadata,
        ClientAiDataCategory::ActivityHistory => AiDataCategory::ActivityHistory,
    }
}

fn client_ai_policy(policy: ClientAiProjectPolicyDto) -> AiProjectPolicy {
    AiProjectPolicy {
        mode: match policy.mode {
            ClientAiProjectAccessMode::Disabled => AiProjectAccessMode::Disabled,
            ClientAiProjectAccessMode::LocalOnly => AiProjectAccessMode::LocalOnly,
            ClientAiProjectAccessMode::SelectedProviders => AiProjectAccessMode::SelectedProviders,
        },
        allowed_remote_providers: policy
            .allowed_remote_providers
            .into_iter()
            .map(client_ai_kind)
            .collect(),
        allowed_categories: policy
            .allowed_categories
            .into_iter()
            .map(client_ai_category)
            .collect(),
        require_remote_confirmation: policy.require_remote_confirmation,
    }
}

fn client_ai_access_policy(policy: ClientAiAccessPolicyDto) -> AiAccessPolicy {
    let mut result = AiAccessPolicy::new(client_ai_policy(policy.default_policy));
    for entry in policy.project_policies {
        result.set_project_policy(entry.project_id, client_ai_policy(entry.policy));
    }
    result
}

fn client_ai_secret_ref(reference: &ClientSecretRef) -> Result<AiSecretRef, ClientError> {
    AiSecretRef::new(reference.namespace.clone(), reference.key.clone()).map_err(|error| {
        ClientError {
            code: ClientErrorCode::InvalidInput,
            message: format!("{error:?}"),
        }
    })
}

fn client_ai_provider_config(
    config: &ClientAiProviderConfigDto,
) -> Result<AiProviderConfig, ClientError> {
    let auth = match &config.auth {
        ClientAiAuthMode::None => AiAuthMode::None,
        ClientAiAuthMode::Bearer { secret } => AiAuthMode::Bearer {
            secret: client_ai_secret_ref(secret)?,
        },
        ClientAiAuthMode::ApiKeyHeader { header, secret } => AiAuthMode::ApiKeyHeader {
            header: header.clone(),
            secret: client_ai_secret_ref(secret)?,
        },
    };
    AiProviderConfig::new(
        config.id.clone(),
        client_ai_kind(config.kind),
        config.endpoint.clone(),
        config.model.clone(),
        auth,
        AiCapabilities {
            chat: config.capabilities.chat,
            tools: config.capabilities.tools,
            structured_output: config.capabilities.structured_output,
            embeddings: config.capabilities.embeddings,
            vision: config.capabilities.vision,
        },
        config.local_only,
    )
    .map_err(|error| ClientError {
        code: ClientErrorCode::InvalidInput,
        message: format!("{error:?}"),
    })
}

fn client_ai_fragments(items: Vec<ClientAiContextFragmentDto>) -> Vec<AiContextFragment> {
    items
        .into_iter()
        .map(|item| AiContextFragment {
            project_id: item.project_id,
            source_id: item.source_id,
            category: client_ai_category(item.category),
            text: item.text,
        })
        .collect()
}

fn client_ai_disclosure(disclosure: orbit_ai::AiDisclosure) -> ClientAiDisclosureDto {
    let provider_kind = format!("{:?}", disclosure.provider_kind).to_ascii_lowercase();
    let destination = match disclosure.destination {
        AiDestination::Local => "local",
        AiDestination::Remote => "remote",
    }
    .to_owned();
    let included_source_ids = disclosure
        .included
        .iter()
        .map(|item| item.source_id.clone())
        .collect();
    let included_categories = disclosure
        .included_categories()
        .into_iter()
        .map(|item| item.as_str().to_owned())
        .collect();
    let included_project_ids = disclosure.included_project_ids().into_iter().collect();
    let omitted = disclosure
        .omitted
        .into_iter()
        .map(|item| ClientAiOmissionDto {
            source_id: item.source_id,
            category: item.category.as_str().to_owned(),
            reason: format!("{:?}", item.reason).to_ascii_lowercase(),
        })
        .collect();
    ClientAiDisclosureDto {
        provider_id: disclosure.provider_id,
        provider_kind,
        destination,
        included_source_ids,
        included_categories,
        included_project_ids,
        omitted,
        requires_confirmation: disclosure.requires_confirmation,
    }
}

fn resolve_client_ai_secret(
    config: &ClientAiProviderConfigDto,
    secret_store: &dyn SecureSecretStore,
) -> Result<Option<Vec<u8>>, ClientError> {
    let reference = match &config.auth {
        ClientAiAuthMode::None => return Ok(None),
        ClientAiAuthMode::Bearer { secret } | ClientAiAuthMode::ApiKeyHeader { secret, .. } => {
            secret
        }
    };
    secret_store
        .get(reference)?
        .ok_or_else(|| ClientError {
            code: ClientErrorCode::NotFound,
            message: format!(
                "AI secret not found: {}/{}",
                reference.namespace, reference.key
            ),
        })
        .map(Some)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientProjectRoadmapAiResponseDto {
    pub disclosure: ClientAiDisclosureDto,
    pub proposal: ProjectRoadmapProposalDto,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientProjectCompletionAiResponseDto {
    pub disclosure: ClientAiDisclosureDto,
    pub assessment: ProjectCompletionAssessmentDto,
}

impl OrbitClient {
    pub fn preview_ai_disclosure(
        &self,
        config: ClientAiProviderConfigDto,
        policy: ClientAiAccessPolicyDto,
        context: Vec<ClientAiContextFragmentDto>,
    ) -> Result<ClientAiDisclosureDto, ClientError> {
        let config = client_ai_provider_config(&config)?;
        let policy = client_ai_access_policy(policy);
        Ok(client_ai_disclosure(build_disclosure(
            &config,
            &policy,
            client_ai_fragments(context),
        )))
    }

    pub fn invoke_ai(
        &self,
        config: ClientAiProviderConfigDto,
        policy: ClientAiAccessPolicyDto,
        context: Vec<ClientAiContextFragmentDto>,
        instruction: String,
        confirmed: bool,
        secret_store: &dyn SecureSecretStore,
    ) -> Result<ClientAiResponseDto, ClientError> {
        let resolved_secret = resolve_client_ai_secret(&config, secret_store)?;
        let provider_config = client_ai_provider_config(&config)?;
        let provider = DirectHttpAiProvider::<ReqwestAiHttpTransport>::native(provider_config)
            .map_err(|error| ClientError {
                code: ClientErrorCode::Unavailable,
                message: format!("{error:?}"),
            })?;
        let policy = client_ai_access_policy(policy);
        let (disclosure, response) = invoke_with_disclosure(
            &provider,
            &policy,
            client_ai_fragments(context),
            instruction,
            confirmed,
            resolved_secret.as_deref(),
        )
        .map_err(|error| ClientError {
            code: match &error {
                orbit_ai::AiInvocationError::ConfirmationRequired => ClientErrorCode::Conflict,
                orbit_ai::AiInvocationError::EmptyAllowedContext => ClientErrorCode::InvalidInput,
                orbit_ai::AiInvocationError::Provider(_) => ClientErrorCode::Unavailable,
            },
            message: format!("{error:?}"),
        })?;
        Ok(ClientAiResponseDto {
            disclosure: client_ai_disclosure(disclosure),
            text: response.text,
        })
    }

    pub fn invoke_ai_project_roadmap(
        &self,
        project_id: String,
        config: ClientAiProviderConfigDto,
        policy: ClientAiAccessPolicyDto,
        context: Vec<ClientAiContextFragmentDto>,
        confirmed: bool,
        secret_store: &dyn SecureSecretStore,
    ) -> Result<ClientProjectRoadmapAiResponseDto, ClientError> {
        let project_id = ProjectId::new(project_id).map_err(invalid_input)?;
        let project = self
            .runtime
            .application_queries()
            .project(&project_id)
            .map_err(ClientError::from)?;
        let resolved_secret = resolve_client_ai_secret(&config, secret_store)?;
        let provider = DirectHttpAiProvider::<ReqwestAiHttpTransport>::native(
            client_ai_provider_config(&config)?,
        )
        .map_err(|error| ClientError {
            code: ClientErrorCode::Unavailable,
            message: format!("{error:?}"),
        })?;
        let (disclosure, proposal) = invoke_project_roadmap_with_disclosure(
            &provider,
            &client_ai_access_policy(policy),
            client_ai_fragments(context),
            &project,
            confirmed,
            resolved_secret.as_deref(),
        )
        .map_err(project_agent_invocation_client_error)?;
        Ok(ClientProjectRoadmapAiResponseDto {
            disclosure: client_ai_disclosure(disclosure),
            proposal: ProjectRoadmapProposalDto {
                mode: proposal.mode.as_str().to_owned(),
                objective: proposal.objective,
                assumptions: proposal.assumptions,
                success_criteria: proposal.project_success_criteria,
                milestones: proposal
                    .milestones
                    .into_iter()
                    .map(|milestone| ProjectRoadmapMilestoneDto {
                        title: milestone.title,
                        rationale: milestone.rationale,
                        success_criteria: milestone.success_criteria,
                        dependencies: milestone.dependencies,
                        suggested_target_at_ms: milestone.suggested_target_at_ms,
                    })
                    .collect(),
                suggested_habits: proposal.suggested_habits,
                suggested_metrics: proposal.suggested_metrics,
                risks: proposal.risks,
                suggested_review_cadence: proposal.suggested_review_cadence,
            },
        })
    }

    pub fn invoke_ai_project_completion_review(
        &self,
        project_id: String,
        config: ClientAiProviderConfigDto,
        policy: ClientAiAccessPolicyDto,
        context: Vec<ClientAiContextFragmentDto>,
        confirmed: bool,
        secret_store: &dyn SecureSecretStore,
    ) -> Result<ClientProjectCompletionAiResponseDto, ClientError> {
        let project_id = ProjectId::new(project_id).map_err(invalid_input)?;
        let project = self
            .runtime
            .application_queries()
            .project(&project_id)
            .map_err(ClientError::from)?;
        let resolved_secret = resolve_client_ai_secret(&config, secret_store)?;
        let provider = DirectHttpAiProvider::<ReqwestAiHttpTransport>::native(
            client_ai_provider_config(&config)?,
        )
        .map_err(|error| ClientError {
            code: ClientErrorCode::Unavailable,
            message: format!("{error:?}"),
        })?;
        let (disclosure, assessment) = invoke_project_completion_review_with_disclosure(
            &provider,
            &client_ai_access_policy(policy),
            client_ai_fragments(context),
            &project,
            confirmed,
            resolved_secret.as_deref(),
        )
        .map_err(project_agent_invocation_client_error)?;
        Ok(ClientProjectCompletionAiResponseDto {
            disclosure: client_ai_disclosure(disclosure),
            assessment: ProjectCompletionAssessmentDto {
                criteria: assessment
                    .criteria
                    .into_iter()
                    .map(|criterion| ProjectCriterionAssessmentDto {
                        key: criterion.key,
                        satisfied: criterion.satisfied,
                        evidence: criterion.evidence,
                        rationale: criterion.rationale,
                    })
                    .collect(),
                ai_recommends_completion: assessment.ai_recommends_completion,
                deterministic_completion_ready: assessment.deterministic_completion_ready,
                missing_required_criteria: assessment.missing_required_criteria,
            },
        })
    }
}

fn project_agent_invocation_client_error(
    error: orbit_ai::ProjectAgentInvocationError,
) -> ClientError {
    let code = match &error {
        orbit_ai::ProjectAgentInvocationError::ConfirmationRequired => ClientErrorCode::Conflict,
        orbit_ai::ProjectAgentInvocationError::EmptyAllowedContext
        | orbit_ai::ProjectAgentInvocationError::InvalidProposal(_) => {
            ClientErrorCode::InvalidInput
        }
        orbit_ai::ProjectAgentInvocationError::Provider(_) => ClientErrorCode::Unavailable,
    };
    ClientError {
        code,
        message: format!("{error:?}"),
    }
}

impl OrbitClient {
    /// Converts an AI-produced JSON plan into the same deterministic proposal
    /// type used by normal Orbit planning. This method never persists changes;
    /// callers must still pass the returned DTO to `apply_task_plan`, which
    /// revalidates against current canonical state before any write.
    pub fn validate_ai_task_plan_response(
        &self,
        response_text: String,
        start_ms: i64,
        end_ms: i64,
        policy: PlanningPolicyDto,
    ) -> Result<TaskPlanProposalDto, ClientError> {
        let window = PlanningWindow::new(start_ms, end_ms).map_err(invalid_input)?;
        let capacity = capacity_policy(&policy)?;
        let buffers = buffer_policy(&policy);
        let blocks = self
            .runtime
            .application_queries()
            .schedule_between(start_ms, end_ms)
            .map_err(ClientError::from)?;
        let tasks = self
            .runtime
            .application_queries()
            .tasks()
            .map_err(ClientError::from)?;
        let proposal =
            orbit_ai::parse_task_plan_proposal(&response_text).map_err(|error| ClientError {
                code: ClientErrorCode::InvalidInput,
                message: format!("invalid AI plan proposal: {error:?}"),
            })?;
        let validated =
            validate_proposal_with_buffers(window, &blocks, &tasks, capacity, buffers, proposal)
                .map_err(|violations| ClientError {
                    code: ClientErrorCode::InvalidInput,
                    message: format!("AI plan rejected: {violations:?}"),
                })?;
        Ok(task_plan_dto(
            start_ms,
            end_ms,
            policy,
            validated.into_inner(),
        ))
    }
}

#[cfg(test)]
mod m14_client_evidence_tests {
    use super::*;
    use std::fs;
    #[test]
    fn embedded_client_metric_and_binding_workflow() {
        let path = std::env::temp_dir().join(format!(
            "orbit-m14-client-{}-{}.db",
            std::process::id(),
            now_ms()
        ));
        let mut client = OrbitClient::open(&path).unwrap();
        let project = client
            .create_project(CreateProjectRequest {
                id: None,
                name: "M14 integration".into(),
            })
            .unwrap();
        client
            .change_project_evidence(
                project.id.clone(),
                "metric".into(),
                "m14-value".into(),
                true,
            )
            .unwrap();
        client
            .set_metric_target(MetricTargetDto {
                metric_id: "m14-value".into(),
                baseline: 0,
                target: 100,
                direction: "at_least".into(),
            })
            .unwrap();
        client
            .record_metric_observation("m14-value".into(), 1000, 50)
            .unwrap();
        let progress = client
            .project_evidence_progress(project.id.clone(), 1, 1500, 1500)
            .unwrap();
        assert_eq!(
            progress.metric_progress,
            vec![("m14-value".into(), Some(5000))]
        );
        assert_eq!(progress.evidence_progress_bp, Some(5000));
        assert_eq!(
            client
                .list_project_evidence(project.id.clone())
                .unwrap()
                .len(),
            1
        );
        client
            .change_project_evidence(
                project.id.clone(),
                "metric".into(),
                "m14-value".into(),
                false,
            )
            .unwrap();
        assert!(client.list_project_evidence(project.id).unwrap().is_empty());
        drop(client);
        let _ = fs::remove_file(path);
    }
}

#[cfg(test)]
mod local_markdown_knowledge_acceptance {
    use super::*;

    #[test]
    fn legacy_local_client_knowledge_uses_real_markdown_without_sql_body_rows() {
        let path = std::env::temp_dir().join(format!(
            "orbit-markdown-bridge-{}-{}.sqlite",
            std::process::id(),
            super::generate_id("test")
        ));
        let mut client = OrbitClient::open(&path).unwrap();
        let note = client
            .create_knowledge(CreateKnowledgeRequest {
                id: Some("note-bridge".into()),
                kind: "decision".into(),
                title: "Use Rust".into(),
                body_markdown: "# Rust\nLocal and portable".into(),
            })
            .unwrap();
        assert_eq!(note.kind, "decision");
        assert_eq!(client.list_knowledge().unwrap().len(), 1);
        assert!(
            client
                .runtime
                .application_queries()
                .legacy_knowledge_rows_empty()
                .unwrap()
        );
        let tree = client.runtime.knowledge_tree().unwrap();
        assert!(format!("{tree:?}").contains("Inbox/note-bridge.md"));
        let new_note = client
            .update_knowledge(UpdateKnowledgeRequest {
                id: "note-bridge".into(),
                title: "Choose Rust".into(),
                body_markdown: "# Updated".into(),
            })
            .unwrap();
        assert_eq!(new_note.title, "Choose Rust");
        let resolved = client
            .set_knowledge_status(SetKnowledgeStatusRequest {
                id: "note-bridge".into(),
                status: "resolved".into(),
            })
            .unwrap();
        assert_eq!(resolved.status, "resolved");
        assert_eq!(client.current_change_cursor().unwrap().activity_count, 3);
        drop(client);
        let reopened = OrbitClient::open(&path).unwrap();
        let notes = reopened.list_knowledge().unwrap();
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].body_markdown, "# Updated");
        assert_eq!(notes[0].status, "resolved");
        drop(reopened);
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir_all(path.with_extension("knowledge"));
    }
}

#[cfg(test)]
mod m18_accepted_client_workflow_tests {
    use super::*;

    #[test]
    fn embedded_public_client_writes_join_one_accepted_revision_history() {
        let mut client = OrbitClient::open(":memory:").unwrap();
        let created = client
            .create_project(CreateProjectRequest {
                id: Some("project-client".into()),
                name: "Client project".into(),
            })
            .unwrap();
        assert_eq!(created.id, "project-client");
        client
            .set_project_status(SetProjectStatusRequest {
                id: "project-client".into(),
                status: "active".into(),
            })
            .unwrap();
        let task = client
            .create_task(CreateTaskRequest {
                id: Some("task-client".into()),
                title: "Client task".into(),
                project_id: Some("project-client".into()),
                milestone_id: None,
            })
            .unwrap();
        assert_eq!(task.id, "task-client");
        client
            .set_task_planning(SetTaskPlanningRequest {
                id: "task-client".into(),
                estimated_minutes: Some(25),
                earliest_start_at_ms: None,
                deadline_at_ms: None,
            })
            .unwrap();
        let habit = client
            .create_habit(CreateHabitRequest {
                id: Some("habit-client".into()),
                name: "Client habit".into(),
                cadence: "daily".into(),
            })
            .unwrap();
        assert_eq!(habit.id, "habit-client");
        client
            .set_habit_active(SetHabitActiveRequest {
                id: "habit-client".into(),
                active: false,
            })
            .unwrap();
        assert_eq!(client.runtime.snapshot().unwrap().0, 6);
        assert_eq!(
            client
                .runtime
                .application_queries()
                .activities()
                .unwrap()
                .len(),
            6
        );
        assert_eq!(client.list_projects().unwrap().len(), 1);
        assert_eq!(client.list_tasks().unwrap().len(), 1);
        assert_eq!(client.list_habits().unwrap().len(), 1);
    }
}
