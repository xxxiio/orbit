//! Optional self-hosted Orbit authoritative server. Local-only clients do not need it.
mod blueprints;
mod authority_evidence {
    pub use orbit_runtime::authority_evidence::*;
}
mod automation;
mod domain_commands {
    pub use orbit_runtime::domain_commands::*;
}
mod proposals;
mod remote_mcp;
mod session;
mod web;

use automation::{AutomationStore, Job, JobInput, QueueError, Run, SettleInput};
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Path as AxumPath, Query, Request, State},
    http::{HeaderMap, Method, StatusCode, header},
    middleware::{self, Next},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
};
use domain_commands::{ProjectRecord, ProjectResult, TaskCommand, TaskRecord, TaskResult};
use orbit_application::ApplicationError;
use orbit_runtime::AuthoritativeHost;
use orbit_runtime::AuthorityError;
use orbit_runtime::authority_milestones::{MilestoneCommand, MilestoneResult, MilestoneSnapshot};
use orbit_runtime::authority_routines::{RoutineCommand, RoutineResult, RoutineSnapshot};
use orbit_runtime::authority_schedule::{ScheduleCommand, ScheduleResult, ScheduleSnapshot};
use orbit_runtime::authority_strategy::{StrategyCommand, StrategyResult, StrategySnapshot};
use orbit_runtime::capability_server::CapabilityServerError;
use proposals::{Audit, NewProposal, Proposal, ProposalError, ProposalStore};
use serde::{Deserialize, Serialize};
use session::{Authenticated, LoginThrottle, SessionError, SessionStore};
use sha2::{Digest, Sha256};
use std::{
    path::Path,
    sync::{Arc, Mutex},
};
use subtle::ConstantTimeEq;

#[derive(Clone)]
struct AppState {
    store: Arc<Mutex<AuthoritativeHost>>,
    automation: Arc<Mutex<AutomationStore>>,
    proposals: Arc<Mutex<ProposalStore>>,
    sessions: Arc<Mutex<SessionStore>>,
    login_attempts: Arc<Mutex<LoginThrottle>>,
    web_dist: Arc<web::WebDist>,
    /// `Secure` is only safe when the browser reaches the server over TLS.
    cookie_secure: bool,
    admin_key_hash: Vec<u8>,
    /// Upper bound for attachment import bodies, checked before buffering.
    attachment_body_limit: usize,
}
#[derive(Deserialize)]
struct EnrollRequest {
    device_id: String,
    token: String,
}
#[derive(Deserialize)]
struct RotateTokenRequest {
    token: String,
}
#[derive(Serialize)]
struct StatusResponse {
    status: &'static str,
}
#[derive(Serialize)]
struct ErrorResponse {
    /// Owned so validation reasons can carry the rejected value. Storage and
    /// internal failures always use a fixed, opaque message.
    error: String,
}

type ApiError = (StatusCode, Json<ErrorResponse>);
fn error(code: StatusCode, message: impl Into<String>) -> ApiError {
    (
        code,
        Json(ErrorResponse {
            error: message.into(),
        }),
    )
}
/// Map a typed application rejection to the transport status. Keeping the
/// original error until the adapter means not-found, conflict, oversize and
/// validation failures no longer collapse into a generic 400. Domain
/// validation messages are safe to quote: they are human text, never SQL, a
/// filesystem path, a debug dump or an internal identifier.
fn application_error_status(error: &ApplicationError) -> (StatusCode, String) {
    use orbit_application::RepositoryError;
    use orbit_domain::DomainError;
    match error {
        ApplicationError::NotFound { .. } => (StatusCode::NOT_FOUND, "not found".to_owned()),
        ApplicationError::AlreadyExists { .. } => {
            (StatusCode::CONFLICT, "already exists".to_owned())
        }
        ApplicationError::Repository(RepositoryError::Conflict) => {
            (StatusCode::CONFLICT, "conflict".to_owned())
        }
        ApplicationError::AttachmentTooLarge { .. }
        | ApplicationError::Domain(DomainError::AttachmentTooLarge { .. }) => (
            StatusCode::PAYLOAD_TOO_LARGE,
            "attachment exceeds the server byte limit".to_owned(),
        ),
        ApplicationError::Domain(domain) => (StatusCode::BAD_REQUEST, domain.to_string()),
        ApplicationError::InconsistentReference { .. }
        | ApplicationError::PlanRejected { .. }
        | ApplicationError::HabitPlanRejected { .. }
        | ApplicationError::Progress(_) => (StatusCode::BAD_REQUEST, "invalid input".to_owned()),
        ApplicationError::AttachmentContentUnavailable { .. } => (
            StatusCode::CONFLICT,
            "attachment content is unavailable".to_owned(),
        ),
        ApplicationError::Repository(RepositoryError::Unavailable) => (
            StatusCode::SERVICE_UNAVAILABLE,
            "storage unavailable".to_owned(),
        ),
        ApplicationError::Repository(RepositoryError::CorruptData { .. })
        | ApplicationError::Repository(RepositoryError::Other(_))
        | ApplicationError::AttachmentStore(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "storage error".to_owned(),
        ),
    }
}
fn map_error(err: AuthorityError) -> ApiError {
    match err {
        // `Invalid` carries the validation refusal written by the command
        // handler. It is fixed human text (no SQL, path, or debug formatting),
        // so the browser may quote it verbatim.
        AuthorityError::Invalid(message) => error(StatusCode::BAD_REQUEST, message),
        AuthorityError::Application(application) => {
            let (status, message) = application_error_status(&application);
            if status == StatusCode::INTERNAL_SERVER_ERROR {
                eprintln!("orbit-server application command error: {application:?}");
            }
            error(status, message)
        }
        AuthorityError::ImmutableConflict => {
            error(StatusCode::CONFLICT, "immutable sequence already used")
        }
        AuthorityError::StaleRevision => error(StatusCode::CONFLICT, "stale server revision"),
        AuthorityError::IncompleteProjection => error(
            StatusCode::CONFLICT,
            "semantic history is incomplete or conflicted",
        ),
        AuthorityError::MigrationConflict => error(
            StatusCode::CONFLICT,
            "incompatible or conflicting authority history",
        ),
        AuthorityError::Unauthorized => error(StatusCode::FORBIDDEN, "device identity mismatch"),
        AuthorityError::Knowledge(vault_error) => {
            // The Markdown vault is a local file store; an ambiguous or missing
            // identity is a data-integrity condition, not a client mistake.
            eprintln!("orbit-server knowledge error: {vault_error:?}");
            error(StatusCode::CONFLICT, "knowledge vault is unavailable")
        }
        AuthorityError::Sql(rusqlite_error) => {
            // Never expose private SQL or local paths in HTTP error bodies.
            eprintln!("orbit-server storage error: {rusqlite_error}");
            error(StatusCode::INTERNAL_SERVER_ERROR, "storage error")
        }
    }
}

async fn authoritative_strategies(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<StrategySnapshot>, ApiError> {
    device(&state, &headers)?;
    let store = state
        .store
        .lock()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "storage lock error"))?;
    Ok(Json(store.strategies().map_err(map_error)?))
}
async fn authoritative_strategy_command(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(command): Json<StrategyCommand>,
) -> Result<Json<StrategyResult>, ApiError> {
    let actor = device(&state, &headers)?;
    let mut store = state
        .store
        .lock()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "storage lock error"))?;
    Ok(Json(
        store.strategy_command(&actor, command).map_err(map_error)?,
    ))
}

async fn authoritative_milestones(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<MilestoneSnapshot>, ApiError> {
    device(&state, &headers)?;
    let store = state
        .store
        .lock()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "storage lock error"))?;
    Ok(Json(store.milestones().map_err(map_error)?))
}
async fn authoritative_milestone_command(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(command): Json<MilestoneCommand>,
) -> Result<Json<MilestoneResult>, ApiError> {
    let actor = device(&state, &headers)?;
    let mut store = state
        .store
        .lock()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "storage lock error"))?;
    Ok(Json(
        store
            .milestone_command(&actor, command)
            .map_err(map_error)?,
    ))
}

async fn authoritative_routines(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<RoutineSnapshot>, ApiError> {
    device(&state, &headers)?;
    let store = state
        .store
        .lock()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "storage lock error"))?;
    Ok(Json(store.routines().map_err(map_error)?))
}
async fn authoritative_routine_command(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(command): Json<RoutineCommand>,
) -> Result<Json<RoutineResult>, ApiError> {
    let actor = device(&state, &headers)?;
    let mut store = state
        .store
        .lock()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "storage lock error"))?;
    Ok(Json(
        store.routine_command(&actor, command).map_err(map_error)?,
    ))
}

#[derive(Deserialize)]
struct ScheduleConflictQuery {
    starts_at_ms: i64,
    duration_minutes: u32,
    excluding_id: Option<String>,
}
#[derive(Serialize)]
struct ScheduleConflictsResponse {
    conflicts: Vec<orbit_runtime::authority_schedule::ScheduleConflict>,
}
async fn authoritative_schedule(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<ScheduleSnapshot>, ApiError> {
    device(&state, &headers)?;
    let store = state
        .store
        .lock()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "storage lock error"))?;
    Ok(Json(store.schedule().map_err(map_error)?))
}
async fn authoritative_schedule_command(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(command): Json<ScheduleCommand>,
) -> Result<Json<ScheduleResult>, ApiError> {
    let actor = device(&state, &headers)?;
    let mut store = state
        .store
        .lock()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "storage lock error"))?;
    Ok(Json(
        store.schedule_command(&actor, command).map_err(map_error)?,
    ))
}
async fn authoritative_schedule_conflicts(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ScheduleConflictQuery>,
) -> Result<Json<ScheduleConflictsResponse>, ApiError> {
    device(&state, &headers)?;
    let store = state
        .store
        .lock()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "storage lock error"))?;
    let conflicts = store
        .schedule_conflicts(
            query.starts_at_ms,
            query.duration_minutes,
            query.excluding_id.as_deref(),
        )
        .map_err(map_error)?;
    Ok(Json(ScheduleConflictsResponse { conflicts }))
}

#[derive(Deserialize)]
struct WorkloadQuery {
    now_ms: i64,
    #[serde(default = "default_workload_horizon")]
    horizon_ms: i64,
}
fn default_workload_horizon() -> i64 {
    604800000
}
async fn authoritative_workload(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<WorkloadQuery>,
) -> Result<Json<orbit_runtime::workload_insights::WorkloadInsights>, ApiError> {
    device(&state, &headers)?;
    let store = state
        .store
        .lock()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "storage lock error"))?;
    Ok(Json(
        store
            .workload_insights(query.now_ms, query.horizon_ms)
            .map_err(map_error)?,
    ))
}

#[derive(Deserialize)]
struct SchedulePlanQuery {
    starts_at_ms: i64,
    ends_at_ms: i64,
    #[serde(default = "default_next_limit")]
    max_tasks: usize,
    project_id: Option<String>,
    default_minutes: Option<i64>,
}
async fn authoritative_schedule_plan(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<SchedulePlanQuery>,
) -> Result<Json<orbit_runtime::schedule_planning::SchedulePlan>, ApiError> {
    device(&state, &headers)?;
    let store = state
        .store
        .lock()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "storage lock error"))?;
    let request = orbit_runtime::schedule_planning::SchedulePlanRequest {
        starts_at_ms: query.starts_at_ms,
        ends_at_ms: query.ends_at_ms,
        max_tasks: query.max_tasks,
        project_id: query.project_id,
        default_minutes: query.default_minutes,
        include_unsized: false,
    };
    Ok(Json(store.schedule_plan(&request).map_err(map_error)?))
}

#[derive(Deserialize)]
struct CompletionImpactQuery {
    task_id: String,
    now_ms: i64,
    #[serde(default = "default_next_limit")]
    limit: usize,
}
async fn authoritative_completion_impact(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<CompletionImpactQuery>,
) -> Result<Json<orbit_runtime::task_impact::CompletionImpact>, ApiError> {
    device(&state, &headers)?;
    let store = state
        .store
        .lock()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "storage lock error"))?;
    Ok(Json(
        store
            .preview_task_completion(&orbit_runtime::task_impact::CompletionImpactRequest {
                task_id: query.task_id,
                now_ms: query.now_ms,
                next_limit: query.limit,
            })
            .map_err(map_error)?,
    ))
}

#[derive(Deserialize)]
struct WorkBriefQuery {
    now_ms: i64,
    ends_at_ms: i64,
    #[serde(default = "default_next_limit")]
    limit: usize,
    project_id: Option<String>,
    default_minutes: Option<i64>,
}
async fn authoritative_work_brief(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<WorkBriefQuery>,
) -> Result<Json<orbit_runtime::planning_brief::WorkBrief>, ApiError> {
    device(&state, &headers)?;
    let store = state
        .store
        .lock()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "storage lock error"))?;
    Ok(Json(
        store
            .work_brief(&orbit_runtime::planning_brief::WorkBriefRequest {
                now_ms: query.now_ms,
                ends_at_ms: query.ends_at_ms,
                limit: query.limit,
                project_id: query.project_id,
                default_minutes: query.default_minutes,
            })
            .map_err(map_error)?,
    ))
}

#[derive(Deserialize)]
struct PlanningFocusQuery {
    task_id: String,
    now_ms: i64,
    ends_at_ms: i64,
    #[serde(default = "default_next_limit")]
    limit: usize,
    project_id: Option<String>,
    #[serde(default = "default_focus_neighbors")]
    max_neighbors: usize,
}
fn default_focus_neighbors() -> usize {
    12
}
async fn authoritative_planning_focus(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<PlanningFocusQuery>,
) -> Result<Json<orbit_runtime::planning_focus::TaskFocus>, ApiError> {
    device(&state, &headers)?;
    let store = state
        .store
        .lock()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "storage lock error"))?;
    let brief = store
        .work_brief(&orbit_runtime::planning_brief::WorkBriefRequest {
            now_ms: query.now_ms,
            ends_at_ms: query.ends_at_ms,
            limit: query.limit,
            project_id: query.project_id,
            default_minutes: None,
        })
        .map_err(map_error)?;
    Ok(Json(
        store
            .planning_focus(
                &brief,
                &orbit_runtime::planning_focus::FocusRequest {
                    task_id: query.task_id,
                    max_neighbors: query.max_neighbors,
                },
            )
            .map_err(map_error)?,
    ))
}

#[derive(Deserialize)]
struct PlanningGroupQuery {
    project_id: String,
    now_ms: i64,
    ends_at_ms: i64,
    milestone_id: Option<String>,
    #[serde(default = "default_next_limit")]
    limit: usize,
    #[serde(default = "default_focus_neighbors")]
    max_tasks: usize,
}
async fn authoritative_planning_group(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<PlanningGroupQuery>,
) -> Result<Json<orbit_runtime::planning_focus::GroupFocus>, ApiError> {
    device(&state, &headers)?;
    let store = state
        .store
        .lock()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "storage lock error"))?;
    let brief = store
        .work_brief(&orbit_runtime::planning_brief::WorkBriefRequest {
            now_ms: query.now_ms,
            ends_at_ms: query.ends_at_ms,
            limit: query.limit,
            project_id: Some(query.project_id.clone()),
            default_minutes: None,
        })
        .map_err(map_error)?;
    Ok(Json(
        store
            .planning_group(
                &brief,
                &orbit_runtime::planning_focus::GroupFocusRequest {
                    project_id: query.project_id,
                    milestone_id: query.milestone_id,
                    max_tasks: query.max_tasks,
                },
            )
            .map_err(map_error)?,
    ))
}

/// Authority-checked read-only action discovery. Every mutation in the menu
/// remains a separately authenticated, reviewed command.
async fn authoritative_planning_task_actions(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<PlanningFocusQuery>,
) -> Result<Json<orbit_runtime::planning_actions::PlanningActionMenu>, ApiError> {
    device(&state, &headers)?;
    let store = state
        .store
        .lock()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "storage lock error"))?;
    let brief = store
        .work_brief(&orbit_runtime::planning_brief::WorkBriefRequest {
            now_ms: query.now_ms,
            ends_at_ms: query.ends_at_ms,
            limit: query.limit,
            project_id: query.project_id,
            default_minutes: None,
        })
        .map_err(map_error)?;
    Ok(Json(
        store
            .planning_task_actions(
                &brief,
                &orbit_runtime::planning_actions::TaskActionRequest {
                    task_id: query.task_id,
                    authority_available: true,
                },
            )
            .map_err(map_error)?,
    ))
}
async fn authoritative_planning_group_actions(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<PlanningGroupQuery>,
) -> Result<Json<orbit_runtime::planning_actions::PlanningActionMenu>, ApiError> {
    device(&state, &headers)?;
    let store = state
        .store
        .lock()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "storage lock error"))?;
    let brief = store
        .work_brief(&orbit_runtime::planning_brief::WorkBriefRequest {
            now_ms: query.now_ms,
            ends_at_ms: query.ends_at_ms,
            limit: query.limit,
            project_id: Some(query.project_id.clone()),
            default_minutes: None,
        })
        .map_err(map_error)?;
    Ok(Json(
        store
            .planning_group_actions(
                &brief,
                &orbit_runtime::planning_actions::GroupActionRequest {
                    project_id: query.project_id,
                    milestone_id: query.milestone_id,
                    authority_available: true,
                },
            )
            .map_err(map_error)?,
    ))
}

/// Explicit action resolution is a read-only POST because selections contain
/// structured inputs. It cannot enqueue or apply an authority command.
#[derive(Deserialize)]
struct ResolveTaskActionBody {
    task_id: String,
    now_ms: i64,
    ends_at_ms: i64,
    #[serde(default = "default_next_limit")]
    limit: usize,
    project_id: Option<String>,
    selection: orbit_runtime::planning_actions::ActionSelection,
}
#[derive(Deserialize)]
struct ResolveGroupActionBody {
    project_id: String,
    now_ms: i64,
    ends_at_ms: i64,
    #[serde(default = "default_next_limit")]
    limit: usize,
    milestone_id: Option<String>,
    selection: orbit_runtime::planning_actions::ActionSelection,
}
async fn authoritative_handoff_task_action(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<ResolveTaskActionBody>,
) -> Result<Json<orbit_runtime::planning_handoff::PlanningHandoff>, ApiError> {
    device(&state, &headers)?;
    let store = state
        .store
        .lock()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "storage lock error"))?;
    let brief = store
        .work_brief(&orbit_runtime::planning_brief::WorkBriefRequest {
            now_ms: body.now_ms,
            ends_at_ms: body.ends_at_ms,
            limit: body.limit,
            project_id: body.project_id,
            default_minutes: None,
        })
        .map_err(map_error)?;
    Ok(Json(
        orbit_runtime::planning_handoff::dispatch_task(
            &brief,
            &orbit_runtime::planning_actions::TaskActionRequest {
                task_id: body.task_id,
                authority_available: true,
            },
            &body.selection,
        )
        .map_err(|message| error(StatusCode::BAD_REQUEST, message))?,
    ))
}
async fn authoritative_handoff_group_action(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<ResolveGroupActionBody>,
) -> Result<Json<orbit_runtime::planning_handoff::PlanningHandoff>, ApiError> {
    device(&state, &headers)?;
    let store = state
        .store
        .lock()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "storage lock error"))?;
    let brief = store
        .work_brief(&orbit_runtime::planning_brief::WorkBriefRequest {
            now_ms: body.now_ms,
            ends_at_ms: body.ends_at_ms,
            limit: body.limit,
            project_id: Some(body.project_id.clone()),
            default_minutes: None,
        })
        .map_err(map_error)?;
    Ok(Json(
        orbit_runtime::planning_handoff::dispatch_group(
            &brief,
            &orbit_runtime::planning_actions::GroupActionRequest {
                project_id: body.project_id,
                milestone_id: body.milestone_id,
                authority_available: true,
            },
            &body.selection,
        )
        .map_err(|message| error(StatusCode::BAD_REQUEST, message))?,
    ))
}
async fn authoritative_resolve_task_action(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<ResolveTaskActionBody>,
) -> Result<Json<orbit_runtime::planning_actions::ResolvedAction>, ApiError> {
    device(&state, &headers)?;
    let store = state
        .store
        .lock()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "storage lock error"))?;
    let brief = store
        .work_brief(&orbit_runtime::planning_brief::WorkBriefRequest {
            now_ms: body.now_ms,
            ends_at_ms: body.ends_at_ms,
            limit: body.limit,
            project_id: body.project_id,
            default_minutes: None,
        })
        .map_err(map_error)?;
    Ok(Json(
        store
            .resolve_planning_task_action(
                &brief,
                &orbit_runtime::planning_actions::TaskActionRequest {
                    task_id: body.task_id,
                    authority_available: true,
                },
                &body.selection,
            )
            .map_err(map_error)?,
    ))
}
async fn authoritative_resolve_group_action(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<ResolveGroupActionBody>,
) -> Result<Json<orbit_runtime::planning_actions::ResolvedAction>, ApiError> {
    device(&state, &headers)?;
    let store = state
        .store
        .lock()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "storage lock error"))?;
    let brief = store
        .work_brief(&orbit_runtime::planning_brief::WorkBriefRequest {
            now_ms: body.now_ms,
            ends_at_ms: body.ends_at_ms,
            limit: body.limit,
            project_id: Some(body.project_id.clone()),
            default_minutes: None,
        })
        .map_err(map_error)?;
    Ok(Json(
        store
            .resolve_planning_group_action(
                &brief,
                &orbit_runtime::planning_actions::GroupActionRequest {
                    project_id: body.project_id,
                    milestone_id: body.milestone_id,
                    authority_available: true,
                },
                &body.selection,
            )
            .map_err(map_error)?,
    ))
}

#[derive(Deserialize)]
struct DayPlanQuery {
    now_ms: i64,
    available_minutes: i64,
    #[serde(default = "default_next_limit")]
    max_tasks: usize,
    project_id: Option<String>,
    #[serde(default)]
    include_unsized: bool,
    default_minutes: Option<i64>,
}
async fn authoritative_day_plan(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<DayPlanQuery>,
) -> Result<Json<orbit_runtime::daily_planning::DayPlan>, ApiError> {
    device(&state, &headers)?;
    let store = state
        .store
        .lock()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "storage lock error"))?;
    let request = orbit_runtime::daily_planning::DayPlanRequest {
        now_ms: query.now_ms,
        available_minutes: query.available_minutes,
        max_tasks: query.max_tasks,
        project_id: query.project_id,
        include_unsized: query.include_unsized,
        default_minutes: query.default_minutes,
    };
    Ok(Json(store.day_plan(&request).map_err(map_error)?))
}

#[derive(Deserialize)]
struct NextActionsQuery {
    now_ms: i64,
    #[serde(default = "default_next_limit")]
    limit: usize,
}
fn default_next_limit() -> usize {
    20
}
async fn authoritative_next_actions(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<NextActionsQuery>,
) -> Result<Json<orbit_runtime::task_next::NextActionsReport>, ApiError> {
    device(&state, &headers)?;
    let store = state
        .store
        .lock()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "storage lock error"))?;
    Ok(Json(
        store
            .next_actions(query.now_ms, query.limit)
            .map_err(map_error)?,
    ))
}

/// A POST is used only because the read-only scenario model can be sizeable.
/// No task commands or calendar reservations are issued by this handler.
async fn authoritative_compare_scenarios(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<orbit_runtime::planning_scenarios::ScenarioCompareRequest>,
) -> Result<Json<orbit_runtime::planning_scenarios::ScenarioComparison>, ApiError> {
    device(&state, &headers)?;
    let store = state
        .store
        .lock()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "storage lock error"))?;
    Ok(Json(
        store
            .compare_planning_scenarios(&request)
            .map_err(map_error)?,
    ))
}

/// Compare concrete, immutable calendar feasibility for all named scenarios.
/// This is read-only; approving any one option is a separate command.
async fn authoritative_review_scenario_options(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<orbit_runtime::scenario_execution::ScenarioExecutionRequest>,
) -> Result<Json<orbit_runtime::scenario_execution::ScenarioCalendarOptions>, ApiError> {
    let actor = device(&state, &headers)?;
    let store = state
        .store
        .lock()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "storage lock error"))?;
    Ok(Json(
        store
            .review_scenario_options(&actor, &request)
            .map_err(map_error)?,
    ))
}

/// Convert a selected scenario into concrete slots and review the exact
/// authority command. The authenticated device supplies the actor identity.
async fn authoritative_review_scenario_execution(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<orbit_runtime::scenario_execution::ScenarioExecutionRequest>,
) -> Result<Json<orbit_runtime::scenario_execution::ScenarioExecutionReview>, ApiError> {
    let actor = device(&state, &headers)?;
    let store = state
        .store
        .lock()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "storage lock error"))?;
    Ok(Json(
        store
            .review_scenario_execution(&actor, &request)
            .map_err(map_error)?,
    ))
}

/// Review does not mutate accepted state or the calendar.
async fn authoritative_review_schedule_plan(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<orbit_runtime::planning_approval::ScheduleApprovalRequest>,
) -> Result<Json<orbit_runtime::planning_approval::PlanReview>, ApiError> {
    let actor = device(&state, &headers)?;
    let store = state
        .store
        .lock()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "storage lock error"))?;
    Ok(Json(
        store
            .review_schedule_request(&actor, &request)
            .map_err(map_error)?,
    ))
}

/// An explicitly approved, revision-bound write. There is no silent auto-apply.
async fn authoritative_apply_approved_schedule(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<orbit_runtime::planning_approval::ApprovedScheduleRequest>,
) -> Result<Json<orbit_runtime::planning_approval::ApprovedScheduleReceipt>, ApiError> {
    let actor = device(&state, &headers)?;
    let mut store = state
        .store
        .lock()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "storage lock error"))?;
    Ok(Json(
        store
            .execute_schedule_request(&actor, request)
            .map_err(map_error)?,
    ))
}

#[derive(Deserialize)]
struct PortfolioCapacityQuery {
    now_ms: i64,
    available_minutes: i64,
    #[serde(default = "default_next_limit")]
    max_tasks: usize,
    project_id: Option<String>,
    #[serde(default)]
    include_unassigned: bool,
    #[serde(default)]
    include_unsized: bool,
    default_minutes: Option<i64>,
}
async fn authoritative_portfolio_capacity(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<PortfolioCapacityQuery>,
) -> Result<Json<orbit_runtime::portfolio_capacity::PortfolioCapacityPlan>, ApiError> {
    device(&state, &headers)?;
    let store = state
        .store
        .lock()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "storage lock error"))?;
    Ok(Json(
        store
            .portfolio_capacity_plan(
                &orbit_runtime::portfolio_capacity::PortfolioCapacityRequest {
                    now_ms: query.now_ms,
                    available_minutes: query.available_minutes,
                    max_tasks: query.max_tasks,
                    project_id: query.project_id,
                    include_unassigned: query.include_unassigned,
                    include_unsized: query.include_unsized,
                    default_minutes: query.default_minutes,
                },
            )
            .map_err(map_error)?,
    ))
}

#[derive(Deserialize)]
struct PortfolioActionsQuery {
    now_ms: i64,
    project_id: Option<String>,
    #[serde(default = "default_next_limit")]
    limit: usize,
    #[serde(default)]
    include_unassigned: bool,
}
async fn authoritative_portfolio_actions(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<PortfolioActionsQuery>,
) -> Result<Json<orbit_runtime::portfolio_actions::PortfolioActionReport>, ApiError> {
    device(&state, &headers)?;
    let store = state
        .store
        .lock()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "storage lock error"))?;
    Ok(Json(
        store
            .portfolio_actions(&orbit_runtime::portfolio_actions::PortfolioActionRequest {
                now_ms: query.now_ms,
                project_id: query.project_id,
                limit: query.limit,
                include_unassigned: query.include_unassigned,
            })
            .map_err(map_error)?,
    ))
}

async fn authoritative_task_graph(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ReadinessQuery>,
) -> Result<Json<orbit_runtime::task_graph::TaskGraphReport>, ApiError> {
    device(&state, &headers)?;
    let store = state
        .store
        .lock()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "storage lock error"))?;
    Ok(Json(store.task_graph(query.now_ms).map_err(map_error)?))
}

#[derive(Deserialize)]
struct ProjectDependenciesQuery {
    now_ms: i64,
    project_id: Option<String>,
    #[serde(default)]
    include_unassigned: bool,
}
async fn authoritative_project_dependencies(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ProjectDependenciesQuery>,
) -> Result<Json<orbit_runtime::project_dependency_planning::ProjectDependencyReport>, ApiError> {
    device(&state, &headers)?;
    let store = state
        .store
        .lock()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "storage lock error"))?;
    Ok(Json(
        store
            .project_dependency_report(
                &orbit_runtime::project_dependency_planning::ProjectDependencyRequest {
                    now_ms: query.now_ms,
                    project_id: query.project_id,
                    include_unassigned: query.include_unassigned,
                },
            )
            .map_err(map_error)?,
    ))
}

#[derive(Deserialize)]
struct ProjectHandoffsQuery {
    now_ms: i64,
    project_id: Option<String>,
    #[serde(default)]
    include_completed: bool,
}
async fn authoritative_project_handoffs(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ProjectHandoffsQuery>,
) -> Result<Json<orbit_runtime::project_handoffs::ProjectHandoffReport>, ApiError> {
    device(&state, &headers)?;
    let store = state
        .store
        .lock()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "storage lock error"))?;
    Ok(Json(
        store
            .project_handoffs(&orbit_runtime::project_handoffs::HandoffRequest {
                now_ms: query.now_ms,
                project_id: query.project_id,
                include_completed: query.include_completed,
            })
            .map_err(map_error)?,
    ))
}

#[derive(Deserialize)]
struct ReadinessQuery {
    now_ms: i64,
}

/// Deterministic, read-only planning. The caller supplies the evaluation time
/// so online and offline evaluations can agree on a specific observation.
async fn authoritative_task_readiness(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ReadinessQuery>,
) -> Result<Json<orbit_runtime::task_dependencies::TaskReadinessSnapshot>, ApiError> {
    device(&state, &headers)?;
    let store = state
        .store
        .lock()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "storage lock error"))?;
    Ok(Json(store.task_readiness(query.now_ms).map_err(map_error)?))
}

#[derive(Serialize)]
struct TaskDependenciesResponse {
    revision: i64,
    edges: Vec<orbit_runtime::task_dependencies::TaskDependencyEdge>,
}
async fn authoritative_task_dependencies(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<TaskDependenciesResponse>, ApiError> {
    device(&state, &headers)?;
    let store = state
        .store
        .lock()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "storage lock error"))?;
    let (revision, _) = store.tasks().map_err(map_error)?;
    Ok(Json(TaskDependenciesResponse {
        revision,
        edges: store.task_dependency_edges().map_err(map_error)?,
    }))
}

#[derive(Serialize)]
struct DomainTasksResponse {
    revision: i64,
    tasks: Vec<TaskRecord>,
}

/// Canonical task API: domain state and semantic history commit atomically.
async fn authoritative_agent_continue(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(command): Json<orbit_runtime::agent_authority::QueueSessionPrompt>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let owner = device(&state, &headers)?;
    let mut host = state
        .store
        .lock()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "storage lock error"))?;
    Ok(Json(
        host.queue_agent_prompt_authoritatively(&owner, command)
            .map_err(map_error)?,
    ))
}
async fn authoritative_machine_prompts(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<serde_json::Value>>, ApiError> {
    let machine = device(&state, &headers)?;
    let host = state
        .store
        .lock()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "storage lock error"))?;
    Ok(Json(
        host.machine_prompt_inbox_authoritatively(&machine)
            .map_err(map_error)?,
    ))
}
async fn authoritative_machine_claim_prompt(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(command): Json<orbit_runtime::agent_authority::ClaimSessionPrompt>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let machine = device(&state, &headers)?;
    let mut host = state
        .store
        .lock()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "storage lock error"))?;
    Ok(Json(
        host.machine_claim_prompt_authoritatively(&machine, command)
            .map_err(map_error)?,
    ))
}
async fn authoritative_machine_report_prompt(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(command): Json<orbit_runtime::agent_authority::ReportSessionPrompt>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let machine = device(&state, &headers)?;
    let mut host = state
        .store
        .lock()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "storage lock error"))?;
    Ok(Json(
        host.machine_report_prompt_authoritatively(&machine, command)
            .map_err(map_error)?,
    ))
}

#[derive(Deserialize)]
struct AgentJobLookup {
    id: String,
}
async fn authoritative_machine_agent_inbox(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<orbit_runtime::agent_authority::AgentJobView>>, ApiError> {
    let machine = device(&state, &headers)?;
    let host = state
        .store
        .lock()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "storage lock error"))?;
    Ok(Json(
        host.machine_agent_inbox_authoritatively(&machine)
            .map_err(map_error)?,
    ))
}
async fn authoritative_machine_agent_brief(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<AgentJobLookup>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let machine = device(&state, &headers)?;
    let host = state
        .store
        .lock()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "storage lock error"))?;
    Ok(Json(
        host.machine_agent_brief_authoritatively(&machine, &query.id)
            .map_err(map_error)?,
    ))
}
async fn authoritative_machine_agent_claim(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(command): Json<orbit_runtime::agent_authority::ClaimMachineAgentJob>,
) -> Result<Json<orbit_runtime::agent_authority::MachineLaunchClaimReceipt>, ApiError> {
    let machine = device(&state, &headers)?;
    let mut host = state
        .store
        .lock()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "storage lock error"))?;
    Ok(Json(
        host.machine_agent_claim_authoritatively(&machine, command)
            .map_err(map_error)?,
    ))
}
async fn authoritative_machine_agent_launch_result(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(command): Json<orbit_runtime::agent_authority::ReportMachineAgentLaunch>,
) -> Result<Json<orbit_runtime::agent_authority::AgentJobView>, ApiError> {
    let machine = device(&state, &headers)?;
    let mut host = state
        .store
        .lock()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "storage lock error"))?;
    Ok(Json(
        host.machine_agent_launch_result_authoritatively(&machine, command)
            .map_err(map_error)?,
    ))
}
async fn authoritative_machine_agent_observe(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(command): Json<orbit_runtime::agent_authority::ReportMachineAgentObservation>,
) -> Result<Json<orbit_runtime::agent_authority::AgentJobView>, ApiError> {
    let machine = device(&state, &headers)?;
    let mut host = state
        .store
        .lock()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "storage lock error"))?;
    Ok(Json(
        host.machine_agent_observe_authoritatively(&machine, command)
            .map_err(map_error)?,
    ))
}
async fn authoritative_machine_agent_handoff(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(command): Json<orbit_runtime::agent_authority::ReportMachineAgentHandoff>,
) -> Result<Json<Vec<serde_json::Value>>, ApiError> {
    let machine = device(&state, &headers)?;
    let mut host = state
        .store
        .lock()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "storage lock error"))?;
    Ok(Json(
        host.machine_agent_handoff_authoritatively(&machine, command)
            .map_err(map_error)?,
    ))
}
async fn authoritative_agent_start(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(command): Json<orbit_runtime::agent_authority::StartAgentJob>,
) -> Result<Json<orbit_runtime::agent_authority::AgentJobView>, ApiError> {
    let actor = device(&state, &headers)?;
    let mut host = state
        .store
        .lock()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "storage lock error"))?;
    Ok(Json(
        host.start_agent_job_authoritatively(&actor, command)
            .map_err(map_error)?,
    ))
}
async fn authoritative_agent_job_reserve(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(command): Json<orbit_runtime::agent_authority::ReserveAgentJob>,
) -> Result<Json<orbit_runtime::agent_authority::AgentJobView>, ApiError> {
    let actor = device(&state, &headers)?;
    let mut host = state
        .store
        .lock()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "storage lock error"))?;
    Ok(Json(
        host.reserve_agent_job_authoritatively(&actor, command)
            .map_err(map_error)?,
    ))
}
async fn authoritative_agent_job_command(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(command): Json<orbit_runtime::agent_authority::AgentJobCommand>,
) -> Result<Json<orbit_runtime::agent_authority::AgentJobView>, ApiError> {
    let actor = device(&state, &headers)?;
    let mut host = state
        .store
        .lock()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "storage lock error"))?;
    Ok(Json(
        host.accept_agent_job_command_authoritatively(&actor, command)
            .map_err(map_error)?,
    ))
}
async fn authoritative_agent_job(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<AgentJobLookup>,
) -> Result<Json<Option<orbit_runtime::agent_authority::AgentJobView>>, ApiError> {
    let actor = device(&state, &headers)?;
    let host = state
        .store
        .lock()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "storage lock error"))?;
    Ok(Json(
        host.agent_job_authoritative(&actor, &query.id)
            .map_err(map_error)?,
    ))
}
async fn authoritative_agent_attention(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<orbit_runtime::agent_authority::AgentJobView>>, ApiError> {
    let actor = device(&state, &headers)?;
    let host = state
        .store
        .lock()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "storage lock error"))?;
    Ok(Json(
        host.agent_jobs_needing_attention_authoritatively(&actor)
            .map_err(map_error)?,
    ))
}
async fn authoritative_agent_handoffs(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<AgentJobLookup>,
) -> Result<Json<Vec<serde_json::Value>>, ApiError> {
    let actor = device(&state, &headers)?;
    let host = state
        .store
        .lock()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "storage lock error"))?;
    Ok(Json(
        host.agent_handoffs_authoritatively(&actor, &query.id)
            .map_err(map_error)?,
    ))
}
async fn authoritative_agent_submit_handoff(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(command): Json<orbit_runtime::agent_authority::SubmitAgentHandoff>,
) -> Result<Json<Vec<serde_json::Value>>, ApiError> {
    let actor = device(&state, &headers)?;
    let mut host = state
        .store
        .lock()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "storage lock error"))?;
    Ok(Json(
        host.submit_agent_handoff_authoritatively(&actor, command)
            .map_err(map_error)?,
    ))
}
async fn authoritative_agent_review(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<AgentJobLookup>,
) -> Result<Json<Option<serde_json::Value>>, ApiError> {
    let actor = device(&state, &headers)?;
    let host = state
        .store
        .lock()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "storage lock error"))?;
    Ok(Json(
        host.agent_review_authoritatively(&actor, &query.id)
            .map_err(map_error)?,
    ))
}
async fn authoritative_agent_submit_review(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(command): Json<orbit_runtime::agent_authority::ReviewAgentHandoff>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let actor = device(&state, &headers)?;
    let mut host = state
        .store
        .lock()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "storage lock error"))?;
    Ok(Json(
        host.review_agent_handoff_authoritatively(&actor, command)
            .map_err(map_error)?,
    ))
}
async fn authoritative_agent_resume(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(command): Json<orbit_runtime::agent_authority::ResumeAgentJob>,
) -> Result<Json<orbit_runtime::agent_authority::AgentJobView>, ApiError> {
    let actor = device(&state, &headers)?;
    let mut host = state
        .store
        .lock()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "storage lock error"))?;
    Ok(Json(
        host.resume_agent_job_authoritatively(&actor, command)
            .map_err(map_error)?,
    ))
}
async fn authoritative_agent_resume_context(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<AgentJobLookup>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let actor = device(&state, &headers)?;
    let host = state
        .store
        .lock()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "storage lock error"))?;
    Ok(Json(
        host.agent_resume_context_authoritatively(&actor, &query.id)
            .map_err(map_error)?,
    ))
}
async fn authoritative_agent_executors(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<orbit_runtime::agent_authority::AgentExecutorList>, ApiError> {
    let actor = device(&state, &headers)?;
    let host = state
        .store
        .lock()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "storage lock error"))?;
    Ok(Json(
        host.agent_executors_authoritatively(&actor)
            .map_err(map_error)?,
    ))
}
async fn authoritative_agent_overview(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<serde_json::Value>>, ApiError> {
    let actor = device(&state, &headers)?;
    let host = state
        .store
        .lock()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "storage lock error"))?;
    Ok(Json(
        host.agent_overview_authoritatively(&actor)
            .map_err(map_error)?,
    ))
}
#[derive(Deserialize)]
struct AttachmentImportQuery {
    command_id: String,
    expected_revision: i64,
    attachment_id: String,
    activity_id: String,
    owner_kind: String,
    owner_id: String,
    filename: String,
    content_type: Option<String>,
    description: Option<String>,
}
#[derive(Deserialize)]
struct AttachmentOwnerQuery {
    owner_kind: String,
    owner_id: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AttachmentRemoveRequest {
    command_id: String,
    expected_revision: i64,
    attachment_id: String,
    activity_id: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AttachmentAssociateRequest {
    command_id: String,
    expected_revision: i64,
    source_id: String,
    new_id: String,
    owner_kind: String,
    owner_id: String,
    activity_id: String,
}
#[derive(Serialize)]
struct AttachmentView {
    id: String,
    owner_kind: String,
    owner_id: String,
    filename: String,
    content_type: String,
    size_bytes: u64,
    checksum: String,
    created_at_ms: i64,
    description: Option<String>,
    is_image: bool,
}
impl From<&orbit_domain::Attachment> for AttachmentView {
    fn from(attachment: &orbit_domain::Attachment) -> Self {
        Self {
            id: attachment.id().as_str().to_owned(),
            owner_kind: attachment.owner().kind().as_str().to_owned(),
            owner_id: attachment.owner().id().to_owned(),
            filename: attachment.filename().to_owned(),
            content_type: attachment.content_type().to_owned(),
            size_bytes: attachment.size_bytes(),
            checksum: attachment.checksum().to_owned(),
            created_at_ms: attachment.created_at_ms(),
            description: attachment.description().map(str::to_owned),
            is_image: attachment.is_image(),
        }
    }
}
#[derive(Serialize)]
struct AttachmentImportResult {
    revision: i64,
    duplicate: bool,
    /// True when a retried import found its original receipt but the accepted
    /// attachment has since been removed. `attachment` is then null.
    removed: bool,
    attachment: Option<AttachmentView>,
}
#[derive(Serialize)]
struct AttachmentListResult {
    attachments: Vec<AttachmentView>,
}
#[derive(Serialize)]
struct AttachmentRemoveResult {
    revision: i64,
    /// The desired end state (the attachment is no longer present).
    removed: bool,
    duplicate: bool,
    content_removed: bool,
    /// True when a fresh removal command found the attachment already gone.
    already_removed: bool,
}
#[derive(Serialize)]
struct AttachmentAssociateResult {
    revision: i64,
    duplicate: bool,
    attachment_id: String,
}
#[derive(Serialize)]
struct AttachmentErrorResponse {
    error: String,
}
type AttachmentFailure = (StatusCode, Json<AttachmentErrorResponse>);
fn attachment_error(code: StatusCode, message: impl Into<String>) -> AttachmentFailure {
    (
        code,
        Json(AttachmentErrorResponse {
            error: message.into(),
        }),
    )
}
/// Reuse device authentication but answer with an actionable message body.
fn attachment_device(state: &AppState, headers: &HeaderMap) -> Result<String, AttachmentFailure> {
    device(state, headers)
        .map_err(|(status, Json(body))| attachment_error(status, body.error.to_owned()))
}
fn attachment_owner(
    owner_kind: &str,
    owner_id: &str,
) -> Result<orbit_domain::AttachmentOwner, AttachmentFailure> {
    use orbit_domain::{AttachmentOwner, AttachmentOwnerKind};
    let kind = AttachmentOwnerKind::parse(owner_kind).ok_or_else(|| {
        attachment_error(
            StatusCode::BAD_REQUEST,
            format!(
                "unknown owner_kind {owner_kind:?}; expected task, project, work_item, knowledge, or agent_job"
            ),
        )
    })?;
    AttachmentOwner::new(kind, owner_id)
        .map_err(|_| attachment_error(StatusCode::BAD_REQUEST, "owner_id must not be empty"))
}
fn attachment_owner_missing(owner: &orbit_domain::AttachmentOwner) -> AttachmentFailure {
    attachment_error(
        StatusCode::NOT_FOUND,
        format!(
            "{} {} does not exist; create it before attaching files",
            owner.kind().as_str(),
            owner.id()
        ),
    )
}
fn attachment_application_error(error: ApplicationError) -> AttachmentFailure {
    let (status, message) = application_error_status(&error);
    if status == StatusCode::INTERNAL_SERVER_ERROR {
        eprintln!("orbit-server attachment storage error: {error:?}");
    }
    attachment_error(status, message)
}
fn attachment_authority_error(error: AuthorityError) -> AttachmentFailure {
    match error {
        AuthorityError::Application(application) => attachment_application_error(application),
        AuthorityError::Invalid(message) => attachment_error(
            StatusCode::BAD_REQUEST,
            format!("invalid attachment command: {message}"),
        ),
        AuthorityError::ImmutableConflict => attachment_error(
            StatusCode::CONFLICT,
            "command_id was already accepted with a different attachment payload",
        ),
        AuthorityError::StaleRevision => attachment_error(
            StatusCode::CONFLICT,
            "stale server revision; refresh and retry",
        ),
        AuthorityError::IncompleteProjection => attachment_error(
            StatusCode::CONFLICT,
            "semantic history is incomplete or conflicted",
        ),
        AuthorityError::MigrationConflict => attachment_error(
            StatusCode::CONFLICT,
            "incompatible or conflicting authority history",
        ),
        AuthorityError::Unauthorized => {
            attachment_error(StatusCode::FORBIDDEN, "device identity mismatch")
        }
        AuthorityError::Knowledge(_) => {
            attachment_error(StatusCode::CONFLICT, "knowledge vault is unavailable")
        }
        AuthorityError::Sql(error) => {
            eprintln!("orbit-server attachment storage error: {error}");
            attachment_error(StatusCode::INTERNAL_SERVER_ERROR, "storage error")
        }
    }
}
fn attachment_lock(
    state: &AppState,
) -> Result<std::sync::MutexGuard<'_, AuthoritativeHost>, AttachmentFailure> {
    state
        .store
        .lock()
        .map_err(|_| attachment_error(StatusCode::INTERNAL_SERVER_ERROR, "storage lock error"))
}
/// Raw bytes arrive only through this bounded route; metadata comes from the
/// query string so no multipart parser or client path is involved. The device
/// is authenticated before the body is read, so unauthorised callers cannot
/// make the server buffer large uploads.
async fn authoritative_attachment_import(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<AttachmentImportQuery>,
    request: Request,
) -> Result<Json<AttachmentImportResult>, AttachmentFailure> {
    use orbit_application::ImportAttachmentCommand;
    let actor = attachment_device(&state, &headers)?;
    let owner = attachment_owner(&query.owner_kind, &query.owner_id)?;
    let id = orbit_domain::AttachmentId::new(query.attachment_id.clone()).map_err(|_| {
        attachment_error(
            StatusCode::BAD_REQUEST,
            "attachment_id is not a valid identifier",
        )
    })?;
    let activity_id = orbit_domain::ActivityId::new(query.activity_id.clone()).map_err(|_| {
        attachment_error(
            StatusCode::BAD_REQUEST,
            "activity_id is not a valid identifier",
        )
    })?;
    let body = axum::body::to_bytes(request.into_body(), state.attachment_body_limit)
        .await
        .map_err(|_| {
            attachment_error(
                StatusCode::PAYLOAD_TOO_LARGE,
                "attachment body exceeds the server byte limit",
            )
        })?;
    let mut store = attachment_lock(&state)?;
    if !store
        .attachment_owner_exists(&owner)
        .map_err(attachment_application_error)?
    {
        return Err(attachment_owner_missing(&owner));
    }
    let (revision, duplicate, attachment_id) = store
        .import_attachment_accepted(
            &actor,
            &query.command_id,
            query.expected_revision,
            ImportAttachmentCommand {
                id,
                owner: owner.clone(),
                filename: query.filename,
                content_type: query.content_type,
                content: body.to_vec(),
                description: query.description,
                activity_id,
                origin: orbit_domain::ActivityOrigin::External(actor.clone()),
            },
        )
        .map_err(attachment_authority_error)?;
    // Listed metadata is the canonical projection of the accepted row; a
    // duplicate retry therefore returns the original stored record.
    let attachment = store
        .list_attachments(&owner)
        .map_err(attachment_application_error)?
        .into_iter()
        .find(|item| item.id() == &attachment_id);
    let Some(attachment) = attachment else {
        // A lost-ack retry of an import whose attachment was later removed
        // must not report a permanent conflict: the command really was
        // accepted, so report the accepted revision with no metadata.
        if duplicate {
            return Ok(Json(AttachmentImportResult {
                revision,
                duplicate: true,
                removed: true,
                attachment: None,
            }));
        }
        return Err(attachment_error(
            StatusCode::CONFLICT,
            "attachment receipt exists but its metadata is missing",
        ));
    };
    Ok(Json(AttachmentImportResult {
        revision,
        duplicate,
        removed: false,
        attachment: Some(AttachmentView::from(&attachment)),
    }))
}
async fn authoritative_attachment_list(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<AttachmentOwnerQuery>,
) -> Result<Json<AttachmentListResult>, AttachmentFailure> {
    attachment_device(&state, &headers)?;
    let owner = attachment_owner(&query.owner_kind, &query.owner_id)?;
    let store = attachment_lock(&state)?;
    let attachments = store
        .list_attachments(&owner)
        .map_err(attachment_application_error)?
        .iter()
        .map(AttachmentView::from)
        .collect();
    Ok(Json(AttachmentListResult { attachments }))
}
/// Sanitize a display filename for a `Content-Disposition` header: quotes,
/// backslashes and control characters are stripped and non-ASCII characters are
/// replaced so the header is always a valid ASCII header value.
fn attachment_download_filename(filename: &str) -> String {
    let sanitized: String = filename
        .chars()
        .filter(|character| *character != '"' && *character != '\\' && !character.is_control())
        .map(|character| if character.is_ascii() { character } else { '_' })
        .collect();
    if sanitized.trim().is_empty() {
        "attachment".to_owned()
    } else {
        sanitized
    }
}
async fn authoritative_attachment_content(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(id): AxumPath<String>,
) -> Result<(HeaderMap, Vec<u8>), AttachmentFailure> {
    attachment_device(&state, &headers)?;
    let id = orbit_domain::AttachmentId::new(id).map_err(|_| {
        attachment_error(
            StatusCode::BAD_REQUEST,
            "attachment id is not a valid identifier",
        )
    })?;
    let store = attachment_lock(&state)?;
    let (attachment, content) = store
        .attachment_content(&id)
        .map_err(attachment_application_error)?;
    let mut response = HeaderMap::new();
    let content_type = header::HeaderValue::from_str(attachment.content_type()).map_err(|_| {
        attachment_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "stored content type is not a valid header value",
        )
    })?;
    let checksum = header::HeaderValue::from_str(attachment.checksum()).map_err(|_| {
        attachment_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "stored checksum is not a valid header value",
        )
    })?;
    response.insert(header::CONTENT_TYPE, content_type);
    response.insert("x-orbit-checksum", checksum);
    response.insert(
        "x-orbit-size",
        header::HeaderValue::from_str(&attachment.size_bytes().to_string()).map_err(|_| {
            attachment_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "stored size is not a valid header value",
            )
        })?,
    );
    response.insert(
        header::CONTENT_DISPOSITION,
        header::HeaderValue::from_str(&format!(
            "attachment; filename=\"{}\"",
            attachment_download_filename(attachment.filename())
        ))
        .map_err(|_| {
            attachment_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "attachment filename is not a valid header value",
            )
        })?,
    );
    response.insert(
        "x-content-type-options",
        header::HeaderValue::from_static("nosniff"),
    );
    // Content bytes are never logged; the caller receives them once.
    Ok((response, content))
}
async fn authoritative_attachment_remove(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<AttachmentRemoveRequest>,
) -> Result<Json<AttachmentRemoveResult>, AttachmentFailure> {
    use orbit_application::RemoveAttachmentCommand;
    let actor = attachment_device(&state, &headers)?;
    let id = orbit_domain::AttachmentId::new(request.attachment_id).map_err(|_| {
        attachment_error(
            StatusCode::BAD_REQUEST,
            "attachment_id is not a valid identifier",
        )
    })?;
    let activity_id = orbit_domain::ActivityId::new(request.activity_id).map_err(|_| {
        attachment_error(
            StatusCode::BAD_REQUEST,
            "activity_id is not a valid identifier",
        )
    })?;
    let mut store = attachment_lock(&state)?;
    let outcome = store.remove_attachment_accepted(
        &actor,
        &request.command_id,
        request.expected_revision,
        RemoveAttachmentCommand {
            id,
            activity_id,
            origin: orbit_domain::ActivityOrigin::External(actor.clone()),
        },
    );
    let (revision, duplicate, content_removed) = match outcome {
        Ok(value) => value,
        // A fresh removal command for an attachment that is already gone is a
        // successful no-op rather than a permanent 404. No receipt is written,
        // so each retry remains idempotent.
        Err(AuthorityError::Application(ApplicationError::NotFound { .. })) => {
            let revision = store
                .snapshot()
                .map_err(|_| attachment_error(StatusCode::INTERNAL_SERVER_ERROR, "storage error"))?
                .0;
            return Ok(Json(AttachmentRemoveResult {
                revision,
                removed: true,
                duplicate: false,
                content_removed: false,
                already_removed: true,
            }));
        }
        Err(error) => return Err(attachment_authority_error(error)),
    };
    Ok(Json(AttachmentRemoveResult {
        revision,
        removed: true,
        duplicate,
        content_removed,
        already_removed: false,
    }))
}
async fn authoritative_attachment_associate(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<AttachmentAssociateRequest>,
) -> Result<Json<AttachmentAssociateResult>, AttachmentFailure> {
    use orbit_application::AssociateAttachmentCommand;
    let actor = attachment_device(&state, &headers)?;
    let owner = attachment_owner(&request.owner_kind, &request.owner_id)?;
    let source_id = orbit_domain::AttachmentId::new(request.source_id).map_err(|_| {
        attachment_error(
            StatusCode::BAD_REQUEST,
            "source_id is not a valid identifier",
        )
    })?;
    let new_id = orbit_domain::AttachmentId::new(request.new_id).map_err(|_| {
        attachment_error(StatusCode::BAD_REQUEST, "new_id is not a valid identifier")
    })?;
    let activity_id = orbit_domain::ActivityId::new(request.activity_id).map_err(|_| {
        attachment_error(
            StatusCode::BAD_REQUEST,
            "activity_id is not a valid identifier",
        )
    })?;
    let mut store = attachment_lock(&state)?;
    if !store
        .attachment_owner_exists(&owner)
        .map_err(attachment_application_error)?
    {
        return Err(attachment_owner_missing(&owner));
    }
    let (revision, duplicate, attachment_id) = store
        .associate_attachment_accepted(
            &actor,
            &request.command_id,
            request.expected_revision,
            AssociateAttachmentCommand {
                source_id,
                new_id,
                owner,
                activity_id,
                origin: orbit_domain::ActivityOrigin::External(actor.clone()),
            },
        )
        .map_err(attachment_authority_error)?;
    Ok(Json(AttachmentAssociateResult {
        revision,
        duplicate,
        attachment_id: attachment_id.as_str().to_owned(),
    }))
}
async fn authoritative_task_command(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(command): Json<TaskCommand>,
) -> Result<Json<TaskResult>, ApiError> {
    let actor = device(&state, &headers)?;
    let mut store = state
        .store
        .lock()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "storage lock error"))?;
    Ok(Json(
        store.task_command(&actor, command).map_err(map_error)?,
    ))
}
async fn authoritative_tasks(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<DomainTasksResponse>, ApiError> {
    device(&state, &headers)?;
    let store = state
        .store
        .lock()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "storage lock error"))?;
    let (revision, tasks) = store.tasks().map_err(map_error)?;
    Ok(Json(DomainTasksResponse { revision, tasks }))
}
#[derive(Serialize)]
struct TaskDetailResponse {
    id: String,
    title: String,
    status: String,
    project_id: Option<String>,
    milestone_id: Option<String>,
    estimated_minutes: Option<i64>,
    earliest_start_at_ms: Option<i64>,
    deadline_at_ms: Option<i64>,
    description: String,
    priority: Option<String>,
    labels: Vec<String>,
    /// Derived from the recorded activity history; null when the task has no
    /// history row, never a read-time timestamp.
    created_at_ms: Option<i64>,
    updated_at_ms: Option<i64>,
}
impl From<TaskRecord> for TaskDetailResponse {
    fn from(task: TaskRecord) -> Self {
        Self {
            id: task.id,
            title: task.title,
            status: task.status,
            project_id: task.project_id,
            milestone_id: task.milestone_id,
            estimated_minutes: task.estimated_minutes,
            earliest_start_at_ms: task.earliest_start_at_ms,
            deadline_at_ms: task.deadline_at_ms,
            description: task.description,
            priority: task.priority,
            labels: task.labels,
            created_at_ms: None,
            updated_at_ms: None,
        }
    }
}
impl TaskDetailResponse {
    /// Attach the history-derived moments to the canonical projection.
    fn with_timestamps(task: TaskRecord, timestamps: orbit_runtime::TaskTimestamps) -> Self {
        Self {
            created_at_ms: timestamps.created_at_ms,
            updated_at_ms: timestamps.updated_at_ms,
            ..Self::from(task)
        }
    }
}
#[derive(Serialize)]
struct AcceptanceCriterionResponse {
    id: String,
    text: String,
    completed: bool,
    completed_at_ms: Option<i64>,
    position: usize,
}
#[derive(Serialize)]
struct ActivityResponse {
    id: String,
    kind: String,
    origin: String,
    occurred_at_ms: i64,
    summary: String,
}
fn activity_kind(value: &orbit_domain::ActivityKind) -> String {
    use orbit_domain::ActivityKind;
    match value {
        ActivityKind::Created => "created",
        ActivityKind::Updated => "updated",
        ActivityKind::StatusChanged => "status_changed",
        ActivityKind::ProgressUpdated => "progress_updated",
        ActivityKind::BlockerOpened => "blocker_opened",
        ActivityKind::BlockerResolved => "blocker_resolved",
        ActivityKind::Scheduled => "scheduled",
        ActivityKind::Rescheduled => "rescheduled",
        ActivityKind::Completed => "completed",
        ActivityKind::AttachmentAdded => "attachment_added",
        ActivityKind::AttachmentRemoved => "attachment_removed",
        ActivityKind::Custom(value) => return format!("custom:{value}"),
    }
    .to_owned()
}
fn activity_origin(value: &orbit_domain::ActivityOrigin) -> String {
    use orbit_domain::ActivityOrigin;
    match value {
        ActivityOrigin::Human => "human".to_owned(),
        ActivityOrigin::System => "system".to_owned(),
        ActivityOrigin::Agent(agent) => format!("agent:{agent}"),
        ActivityOrigin::External(external) => format!("external:{external}"),
    }
}
fn application_error(err: orbit_application::ApplicationError) -> ApiError {
    eprintln!("orbit-server application read error: {err:?}");
    error(StatusCode::INTERNAL_SERVER_ERROR, "storage error")
}
async fn authoritative_task_detail(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<TaskDetailResponse>, ApiError> {
    device(&state, &headers)?;
    let store = state
        .store
        .lock()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "storage lock error"))?;
    let task = store
        .task_record(&id)
        .map_err(map_error)?
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "task not found"))?;
    let timestamps = store.task_timestamps(&id).map_err(map_error)?;
    Ok(Json(TaskDetailResponse::with_timestamps(task, timestamps)))
}
async fn authoritative_task_acceptance_criteria(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<Vec<AcceptanceCriterionResponse>>, ApiError> {
    device(&state, &headers)?;
    let store = state
        .store
        .lock()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "storage lock error"))?;
    if store.task_record(&id).map_err(map_error)?.is_none() {
        return Err(error(StatusCode::NOT_FOUND, "task not found"));
    }
    let task_id = orbit_domain::TaskId::new(id)
        .map_err(|_| error(StatusCode::BAD_REQUEST, "invalid task id"))?;
    let criteria = store
        .acceptance_criteria(&task_id)
        .map_err(application_error)?;
    let items = criteria
        .items()
        .iter()
        .enumerate()
        .map(|(position, criterion)| AcceptanceCriterionResponse {
            id: criterion.id().as_str().to_owned(),
            text: criterion.text().to_owned(),
            completed: criterion.is_completed(),
            completed_at_ms: criterion.completed_at_ms(),
            position,
        })
        .collect();
    Ok(Json(items))
}
async fn authoritative_task_activity(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<Vec<ActivityResponse>>, ApiError> {
    device(&state, &headers)?;
    let store = state
        .store
        .lock()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "storage lock error"))?;
    if store.task_record(&id).map_err(map_error)?.is_none() {
        return Err(error(StatusCode::NOT_FOUND, "task not found"));
    }
    let records = store
        .application_queries()
        .activities()
        .map_err(application_error)?;
    let mut items: Vec<ActivityResponse> = records
        .iter()
        .filter(|record| {
            matches!(record.subject(), orbit_domain::ActivitySubject::Task(task) if task.as_str() == id)
        })
        .map(|record| ActivityResponse {
            id: record.id().as_str().to_owned(),
            kind: activity_kind(record.kind()),
            origin: activity_origin(record.origin()),
            occurred_at_ms: record.occurred_at_ms(),
            summary: record.summary().to_owned(),
        })
        .collect();
    items.reverse();
    Ok(Json(items))
}
#[derive(Deserialize)]
struct HistoryPrefixQuery {
    revision: i64,
}
#[derive(Serialize)]
struct HistoryPrefixResponse {
    revision: i64,
    history_fingerprint: String,
}
async fn authoritative_history_prefix(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<HistoryPrefixQuery>,
) -> Result<Json<HistoryPrefixResponse>, ApiError> {
    device(&state, &headers)?;
    let store = state
        .store
        .lock()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "storage lock error"))?;
    let fingerprint = store.history_prefix(query.revision).map_err(map_error)?;
    Ok(Json(HistoryPrefixResponse {
        revision: query.revision,
        history_fingerprint: fingerprint,
    }))
}
#[derive(Serialize)]
struct AuthoritySnapshotResponse {
    revision: i64,
    history_fingerprint: String,
    projects: Vec<ProjectRecord>,
    tasks: Vec<TaskRecord>,
    task_dependencies: Vec<orbit_runtime::task_dependencies::TaskDependencyEdge>,
    habits: Vec<authority_evidence::HabitRecord>,
    habit_occurrences: Vec<authority_evidence::HabitOccurrenceRecord>,
    metrics: Vec<authority_evidence::MetricRecord>,
    metric_observations: Vec<authority_evidence::MetricObservationRecord>,
    schedule: Vec<orbit_runtime::authority_schedule::ScheduleRecord>,
    routines: Vec<orbit_runtime::authority_routines::RoutineRecord>,
    milestones: Vec<orbit_runtime::authority_milestones::MilestoneRecord>,
    strategies: Vec<orbit_runtime::authority_strategy::StrategyRecord>,
}
async fn authoritative_snapshot(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<AuthoritySnapshotResponse>, ApiError> {
    device(&state, &headers)?;
    let store = state
        .store
        .lock()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "storage lock error"))?;
    let (revision, history_fingerprint, projects, tasks, evidence) =
        store.snapshot().map_err(map_error)?;
    let schedule = store.schedule().map_err(map_error)?;
    let routines = store.routines().map_err(map_error)?;
    let milestones = store.milestones().map_err(map_error)?;
    let strategies = store.strategies().map_err(map_error)?;
    if milestones.revision != revision
        || strategies.revision != revision
        || routines.revision != revision
        || schedule.revision != revision
    {
        return Err(error(
            StatusCode::CONFLICT,
            "inconsistent schedule snapshot",
        ));
    }
    Ok(Json(AuthoritySnapshotResponse {
        revision,
        history_fingerprint,
        projects,
        tasks,
        task_dependencies: store.task_dependency_edges().map_err(map_error)?,
        habits: evidence.habits,
        habit_occurrences: evidence.habit_occurrences,
        metrics: evidence.metrics,
        metric_observations: evidence.metric_observations,
        schedule: schedule.blocks,
        routines: routines.routines,
        milestones: milestones.milestones,
        strategies: strategies.strategies,
    }))
}
#[derive(Serialize)]
struct AuthorityProjectsResponse {
    revision: i64,
    projects: Vec<ProjectRecord>,
}
async fn authoritative_projects(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<AuthorityProjectsResponse>, ApiError> {
    device(&state, &headers)?;
    let store = state
        .store
        .lock()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "storage lock error"))?;
    let (revision, projects) = store.projects().map_err(map_error)?;
    Ok(Json(AuthorityProjectsResponse { revision, projects }))
}
async fn authoritative_project_command(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(command): Json<domain_commands::ProjectCommandRequest>,
) -> Result<Json<ProjectResult>, ApiError> {
    let actor = device(&state, &headers)?;
    let mut store = state
        .store
        .lock()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "storage lock error"))?;
    Ok(Json(
        store.project_command(&actor, command).map_err(map_error)?,
    ))
}
#[derive(Serialize)]
struct ProjectStatusResponse {
    status: &'static str,
    next: Vec<&'static str>,
}
#[derive(Serialize)]
struct ProjectStatusesResponse {
    statuses: Vec<ProjectStatusResponse>,
}
/// The project status edges the domain actually accepts, read from
/// [`orbit_domain::ProjectStatus::can_transition_to`] rather than a second
/// transition table, so the client offers exactly what the write path accepts.
/// Every status is listed in canonical order and never offers itself.
async fn authoritative_project_statuses(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<ProjectStatusesResponse>, ApiError> {
    device(&state, &headers)?;
    let statuses = orbit_domain::ProjectStatus::ALL
        .into_iter()
        .map(|status| ProjectStatusResponse {
            status: status.as_str(),
            next: orbit_domain::ProjectStatus::ALL
                .into_iter()
                .filter(|next| *next != status && status.can_transition_to(*next))
                .map(|next| next.as_str())
                .collect(),
        })
        .collect();
    Ok(Json(ProjectStatusesResponse { statuses }))
}
#[derive(Deserialize)]
struct SearchQuery {
    /// Absent is treated as empty so the refusal text stays the contract's.
    #[serde(default)]
    q: String,
    #[serde(default)]
    limit: Option<usize>,
    #[serde(default)]
    project_id: Option<String>,
}
/// One query across projects, tasks and notes. All matching and composition
/// happen in the runtime; this handler only maps the response.
async fn authoritative_search(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<SearchQuery>,
) -> Result<Json<orbit_runtime::search::SearchResponse>, ApiError> {
    device(&state, &headers)?;
    let store = state
        .store
        .lock()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "storage lock error"))?;
    let limit = query.limit.unwrap_or(orbit_runtime::search::DEFAULT_LIMIT);
    // An empty filter means "no filter"; only a real id narrows the answer.
    let project_id = query
        .project_id
        .as_deref()
        .map(str::trim)
        .filter(|id| !id.is_empty());
    Ok(Json(
        store
            .search(&query.q, limit, project_id)
            .map_err(map_error)?,
    ))
}
#[derive(Serialize)]
struct SavedViewsResponse {
    views: Vec<orbit_runtime::saved_views::SavedViewRecord>,
}
async fn authoritative_saved_views(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<SavedViewsResponse>, ApiError> {
    let actor = device(&state, &headers)?;
    let store = state
        .store
        .lock()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "storage lock error"))?;
    Ok(Json(SavedViewsResponse {
        views: store.saved_views(&actor).map_err(map_error)?,
    }))
}
async fn authoritative_saved_view_command(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(command): Json<orbit_runtime::saved_views::SavedViewCommand>,
) -> Result<Json<orbit_runtime::saved_views::SavedViewResult>, ApiError> {
    let actor = device(&state, &headers)?;
    let mut store = state
        .store
        .lock()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "storage lock error"))?;
    Ok(Json(
        store
            .saved_view_command(&actor, command)
            .map_err(map_error)?,
    ))
}
#[derive(Deserialize)]
struct TaskTransitionsQuery {
    status: String,
}
#[derive(Serialize)]
struct TaskTransitionsResponse {
    status: &'static str,
    next: Vec<&'static str>,
}
/// The status edges the domain actually accepts from one canonical status.
/// This is a read of [`orbit_domain::TaskStatus::can_transition_to`], not a
/// second transition table: the client offers exactly what the write path
/// would accept, and a failure to read it falls back to the older
/// offer-everything behaviour.
async fn authoritative_task_transitions(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<TaskTransitionsQuery>,
) -> Result<Json<TaskTransitionsResponse>, ApiError> {
    device(&state, &headers)?;
    let requested = q.status.trim().to_ascii_lowercase();
    let current = orbit_domain::TaskStatus::ALL
        .into_iter()
        .find(|status| status.as_str() == requested)
        .ok_or_else(|| error(StatusCode::BAD_REQUEST, "unknown task status"))?;
    let next = orbit_domain::TaskStatus::ALL
        .into_iter()
        .filter(|next| *next != current && current.can_transition_to(*next))
        .map(|next| next.as_str())
        .collect();
    Ok(Json(TaskTransitionsResponse {
        status: current.as_str(),
        next,
    }))
}
/// File-backed tree for folder navigation; kept separate from transitional SQL note bodies.
async fn authoritative_knowledge_tree(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<orbit_runtime::knowledge_files::KnowledgeNode>, ApiError> {
    device(&state, &headers)?;
    let store = state
        .store
        .lock()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "storage lock error"))?;
    Ok(Json(store.knowledge_tree().map_err(|_| {
        error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "knowledge filesystem error",
        )
    })?))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MarkdownDocumentQuery {
    path: String,
}
#[derive(Serialize)]
struct MarkdownDocumentResponse {
    path: String,
    body: String,
}

/// Read real Markdown bytes from the folder tree, never from the legacy SQL body.
/// A symlink, non-Markdown path, or traversal attempt is rejected by MarkdownVault.
async fn authoritative_markdown_document(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<MarkdownDocumentQuery>,
) -> Result<Json<MarkdownDocumentResponse>, ApiError> {
    device(&state, &headers)?;
    if query.path.len() > 2048 || query.path.split('/').any(|part| part.starts_with('.')) {
        return Err(error(StatusCode::BAD_REQUEST, "invalid Markdown path"));
    }
    let store = state
        .store
        .lock()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "storage lock error"))?;
    use orbit_runtime::knowledge_files::VaultError;
    let body = store
        .markdown_document(&query.path)
        .map_err(|err| match err {
            VaultError::InvalidPath => error(StatusCode::BAD_REQUEST, "invalid Markdown path"),
            VaultError::NotFound => error(StatusCode::NOT_FOUND, "Markdown document not found"),
            VaultError::Io(_) => error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "knowledge filesystem error",
            ),
            VaultError::AlreadyExists | VaultError::Conflict => {
                error(StatusCode::CONFLICT, "knowledge conflict")
            }
        })?;
    Ok(Json(MarkdownDocumentResponse {
        path: query.path,
        body,
    }))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MarkdownIdQuery {
    id: String,
}
#[derive(Serialize)]
struct MarkdownIdResponse {
    note: orbit_runtime::knowledge_service::KnowledgeDocumentInfo,
    body: String,
}

async fn authoritative_markdown_by_id(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<MarkdownIdQuery>,
) -> Result<Json<MarkdownIdResponse>, ApiError> {
    device(&state, &headers)?;
    let store = state
        .store
        .lock()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "storage lock error"))?;
    let result = store
        .markdown_by_id(&query.id)
        .map_err(|_| {
            error(
                StatusCode::CONFLICT,
                "invalid or ambiguous Markdown identity",
            )
        })?
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "Markdown note not found"))?;
    Ok(Json(MarkdownIdResponse {
        note: result.0,
        body: result.1,
    }))
}

/// Server-authoritative inventory for clients building an offline Markdown mirror.
/// Returning the complete sorted identity set allows clients to identify moved
/// and deleted files without making their pending local writes authoritative.
async fn authoritative_markdown_inventory(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<orbit_runtime::knowledge_service::KnowledgeDocumentInfo>>, ApiError> {
    device(&state, &headers)?;
    let store = state
        .store
        .lock()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "storage lock error"))?;
    let mut notes = store
        .markdown_knowledge_index()
        .map_err(|_| error(StatusCode::CONFLICT, "ambiguous Markdown identities"))?;
    notes.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(Json(notes))
}

/// A byte-sensitive inventory for a connected client's full Markdown mirror.
/// The ordinary inventory endpoint stays available for lightweight browsing.
/// A mirror client must compare two manifests, including body digests, and
/// verify each downloaded body against the manifest before publication.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct MarkdownManifestEntry {
    note: orbit_runtime::knowledge_service::KnowledgeDocumentInfo,
    body_sha256: String,
}

async fn authoritative_markdown_manifest(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<MarkdownManifestEntry>>, ApiError> {
    use sha2::{Digest, Sha256};
    device(&state, &headers)?;
    let store = state
        .store
        .lock()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "storage lock error"))?;
    let mut notes = store
        .markdown_knowledge_index()
        .map_err(|_| error(StatusCode::CONFLICT, "ambiguous Markdown identities"))?;
    notes.sort_by(|a, b| a.id.cmp(&b.id));
    let mut result = Vec::with_capacity(notes.len());
    let mut paths = std::collections::HashSet::new();
    for note in notes {
        if !paths.insert(note.path.clone()) {
            return Err(error(StatusCode::CONFLICT, "duplicate Markdown path"));
        }
        let (actual, body) = store
            .markdown_by_id(&note.id)
            .map_err(|_| {
                error(
                    StatusCode::CONFLICT,
                    "Markdown changed during manifest discovery",
                )
            })?
            .ok_or_else(|| {
                error(
                    StatusCode::CONFLICT,
                    "Markdown disappeared during manifest discovery",
                )
            })?;
        if actual != note {
            return Err(error(
                StatusCode::CONFLICT,
                "Markdown moved during manifest discovery",
            ));
        }
        result.push(MarkdownManifestEntry {
            note,
            body_sha256: format!("{:x}", Sha256::digest(body.as_bytes())),
        });
    }
    Ok(Json(result))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MarkdownSearchRequest {
    query: String,
    limit: usize,
}
async fn authoritative_markdown_search(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<MarkdownSearchRequest>,
) -> Result<Json<Vec<orbit_runtime::knowledge_service::KnowledgeDocumentInfo>>, ApiError> {
    device(&state, &headers)?;
    let store = state
        .store
        .lock()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "storage lock error"))?;
    Ok(Json(
        store
            .markdown_search(&request.query, request.limit)
            .map_err(|_| {
                error(
                    StatusCode::CONFLICT,
                    "Markdown vault has ambiguous identities",
                )
            })?,
    ))
}

async fn authoritative_markdown_relations(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<MarkdownIdQuery>,
) -> Result<Json<orbit_runtime::knowledge_service::KnowledgeFileRelations>, ApiError> {
    device(&state, &headers)?;
    let store = state
        .store
        .lock()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "storage lock error"))?;
    Ok(Json(
        store
            .markdown_relations(&query.id)
            .map_err(|_| {
                error(
                    StatusCode::CONFLICT,
                    "Markdown vault has ambiguous identities",
                )
            })?
            .ok_or_else(|| error(StatusCode::NOT_FOUND, "Markdown note not found"))?,
    ))
}

/// Notes that carry an exact typed reference to one entity, ordered by path.
///
/// The target is refused, never silently answered with an empty list: a client
/// that asked about `habit:` or a malformed id must learn it asked something
/// this server cannot answer.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MarkdownReferencesQuery {
    target: String,
}

async fn authoritative_markdown_references(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<MarkdownReferencesQuery>,
) -> Result<Json<orbit_runtime::references::MarkdownReferences>, ApiError> {
    device(&state, &headers)?;
    let store = state
        .store
        .lock()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "storage lock error"))?;
    Ok(Json(
        store
            .markdown_references(&query.target)
            .map_err(map_error)?,
    ))
}

/// Add or remove one typed entity reference on a note.
///
/// The request is flat (not wrapped in `command`) because it names three
/// scalars rather than one document. The runtime still publishes the body
/// through the Markdown command ledger, so a repeated command id replays the
/// same receipt and a reused id with a different payload conflicts.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MarkdownReferenceCommandRequest {
    command_id: String,
    note_id: String,
    target: String,
    action: String,
}

async fn authoritative_markdown_reference_command(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<MarkdownReferenceCommandRequest>,
) -> Result<Json<orbit_runtime::references::MarkdownReferenceReceipt>, ApiError> {
    let actor = device(&state, &headers)?;
    let mut store = state
        .store
        .lock()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "storage lock error"))?;
    Ok(Json(
        store
            .execute_markdown_reference_command(
                &actor,
                &request.command_id,
                orbit_runtime::references::MarkdownReferenceCommand {
                    note_id: request.note_id,
                    target: request.target,
                    action: request.action,
                },
            )
            .map_err(map_error)?,
    ))
}

/// Project-scoped knowledge: notes referencing the project or one of its tasks.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProjectKnowledgeQuery {
    project_id: String,
}

async fn authoritative_project_knowledge(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ProjectKnowledgeQuery>,
) -> Result<Json<orbit_runtime::references::ProjectKnowledge>, ApiError> {
    device(&state, &headers)?;
    let store = state
        .store
        .lock()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "storage lock error"))?;
    Ok(Json(
        store
            .project_knowledge(&query.project_id)
            .map_err(map_error)?,
    ))
}

// File-authoritative command endpoint. Unlike the transitional /knowledge-commands
// API, the body is published to a real .md file before an accepted receipt.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MarkdownWriteRequest {
    command_id: String,
    command: orbit_runtime::markdown_commands::MarkdownCommand,
}
async fn authoritative_markdown_command(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<MarkdownWriteRequest>,
) -> Result<Json<orbit_runtime::markdown_commands::MarkdownReceipt>, ApiError> {
    let actor = device(&state, &headers)?;
    let mut store = state
        .store
        .lock()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "storage lock error"))?;
    Ok(Json(
        store
            .execute_markdown_command(&actor, &request.command_id, request.command)
            .map_err(map_error)?,
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FolderCreateRequest {
    command_id: String,
    command: orbit_runtime::folder_commands::FolderCreate,
}
async fn authoritative_folder_create(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<FolderCreateRequest>,
) -> Result<Json<orbit_runtime::folder_commands::FolderCreateReceipt>, ApiError> {
    let actor = device(&state, &headers)?;
    let mut host = state
        .store
        .lock()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "storage lock error"))?;
    Ok(Json(
        host.execute_folder_create(&actor, &request.command_id, request.command)
            .map_err(map_error)?,
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FolderMoveRequest {
    command_id: String,
    command: orbit_runtime::folder_commands::FolderMove,
}
async fn authoritative_folder_move(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<FolderMoveRequest>,
) -> Result<Json<orbit_runtime::folder_commands::FolderReceipt>, ApiError> {
    let actor = device(&state, &headers)?;
    let mut host = state
        .store
        .lock()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "storage lock error"))?;
    Ok(Json(
        host.execute_folder_move(&actor, &request.command_id, request.command)
            .map_err(map_error)?,
    ))
}

async fn authoritative_evidence_command(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(command): Json<authority_evidence::EvidenceCommand>,
) -> Result<Json<authority_evidence::EvidenceResult>, ApiError> {
    let actor = device(&state, &headers)?;
    let mut store = state
        .store
        .lock()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "storage lock error"))?;
    Ok(Json(
        store.evidence_command(&actor, command).map_err(map_error)?,
    ))
}
async fn authoritative_evidence(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<authority_evidence::EvidenceSnapshot>, ApiError> {
    device(&state, &headers)?;
    let store = state
        .store
        .lock()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "storage lock error"))?;
    Ok(Json(store.evidence().map_err(map_error)?))
}
fn bearer(headers: &HeaderMap) -> Result<&str, ApiError> {
    headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .filter(|s| !s.is_empty())
        .ok_or_else(|| error(StatusCode::UNAUTHORIZED, "bearer token required"))
}
fn admin(state: &AppState, headers: &HeaderMap) -> Result<(), ApiError> {
    let token = bearer(headers)?;
    let hash = Sha256::digest(token.as_bytes());
    if bool::from(state.admin_key_hash.as_slice().ct_eq(hash.as_slice())) {
        Ok(())
    } else {
        Err(error(StatusCode::UNAUTHORIZED, "unauthorized"))
    }
}
fn device(state: &AppState, headers: &HeaderMap) -> Result<String, ApiError> {
    if headers.contains_key(header::AUTHORIZATION) {
        return bearer_device(state, headers);
    }
    // Browser sessions are the only alternative credential; absent a bearer
    // token there is nothing else to authenticate.
    let token =
        session_cookie(headers).ok_or_else(|| error(StatusCode::UNAUTHORIZED, "unauthorized"))?;
    let now = now_ms()?;
    let session = {
        let mut sessions = state
            .sessions
            .lock()
            .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "session lock error"))?;
        sessions.authenticate(&token, now).map_err(session_error)?
    };
    let Some(Authenticated { device_id, .. }) = session else {
        return Err(error(StatusCode::UNAUTHORIZED, "unauthorized"));
    };
    // A session outlives the device credential, so liveness is re-checked on
    // every request through the runtime rather than trusted from the session.
    let active = state
        .store
        .lock()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "storage lock error"))?
        .device_active(&device_id)
        .map_err(map_error)?;
    if active {
        Ok(device_id)
    } else {
        Err(error(StatusCode::UNAUTHORIZED, "unauthorized"))
    }
}
fn bearer_device(state: &AppState, headers: &HeaderMap) -> Result<String, ApiError> {
    let token = bearer(headers)?;
    let id = headers
        .get("x-orbit-device-id")
        .and_then(|v| v.to_str().ok())
        .filter(|v| !v.trim().is_empty())
        .ok_or_else(|| error(StatusCode::BAD_REQUEST, "x-orbit-device-id header required"))?;
    let store = state
        .store
        .lock()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "storage lock error"))?;
    if store.authorize_device(id, token).map_err(map_error)? {
        Ok(id.to_owned())
    } else {
        Err(error(StatusCode::UNAUTHORIZED, "unauthorized"))
    }
}

const SESSION_COOKIE: &str = "orbit_session";
const CLEARED_SESSION_COOKIE: &str = "orbit_session=; Path=/; Max-Age=0; HttpOnly; SameSite=Strict";

/// Tolerant cookie parsing: other cookies, malformed pairs and whitespace are
/// ignored, and a suspicious session value is treated as absent.
fn session_cookie(headers: &HeaderMap) -> Option<String> {
    for value in headers.get_all(header::COOKIE) {
        let Ok(raw) = value.to_str() else { continue };
        for pair in raw.split(';') {
            let Some((name, value)) = pair.split_once('=') else {
                continue;
            };
            if name.trim() != SESSION_COOKIE {
                continue;
            }
            let value = value.trim();
            if value.is_empty() || value.contains(';') || value.chars().any(char::is_whitespace) {
                return None;
            }
            return Some(value.to_owned());
        }
    }
    None
}
fn session_cookie_header(token: &str, secure: bool, max_age_seconds: i64) -> String {
    let secure = if secure { "; Secure" } else { "" };
    format!(
        "orbit_session={token}; Path=/; Max-Age={}; HttpOnly; SameSite=Strict{secure}",
        max_age_seconds.max(0)
    )
}
/// Cookie `Max-Age` is whole seconds of remaining server lifetime. A session
/// that is already at its deadline clamps to zero rather than going negative.
fn remaining_seconds(expires_at_ms: i64, now_ms: i64) -> i64 {
    expires_at_ms.saturating_sub(now_ms) / 1000
}

fn session_error(failure: SessionError) -> ApiError {
    match failure {
        SessionError::Sql(sql) => {
            eprintln!("orbit-server session storage error: {sql}");
            error(StatusCode::INTERNAL_SERVER_ERROR, "storage error")
        }
        SessionError::Invalid(message) => error(StatusCode::BAD_REQUEST, message),
        SessionError::Random => error(StatusCode::INTERNAL_SERVER_ERROR, "session error"),
    }
}
/// Small error wrapper so handler signatures stay cheap to move; the response
/// itself is boxed.
struct WebFailure(Box<Response>);
impl IntoResponse for WebFailure {
    fn into_response(self) -> Response {
        *self.0
    }
}
/// Session endpoints never cache, including their failures.
fn web_api_error(api: ApiError) -> WebFailure {
    let mut response = api.into_response();
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        axum::http::HeaderValue::from_static("no-store"),
    );
    WebFailure(Box::new(response))
}
fn web_error(status: StatusCode, message: &'static str) -> WebFailure {
    web_api_error(error(status, message))
}
/// A failed session read invalidates the browser cookie too.
fn web_session_unauthorized() -> WebFailure {
    let failure = web_error(StatusCode::UNAUTHORIZED, "unauthorized");
    let mut response = failure.into_response();
    response.headers_mut().insert(
        header::SET_COOKIE,
        axum::http::HeaderValue::from_static(CLEARED_SESSION_COOKIE),
    );
    WebFailure(Box::new(response))
}
fn web_lock_error() -> WebFailure {
    web_error(
        StatusCode::INTERNAL_SERVER_ERROR,
        "session storage lock error",
    )
}

#[derive(Deserialize)]
struct WebLogin {
    device_id: String,
    token: String,
}
#[derive(Serialize)]
struct WebSessionResponse {
    device_id: String,
    expires_at_ms: i64,
}

/// Exchange device credentials for an HttpOnly cookie session. Failures never
/// reveal whether the device id or the token was wrong.
async fn create_web_session(
    State(state): State<AppState>,
    Json(request): Json<WebLogin>,
) -> Result<Response, WebFailure> {
    // Throttle buckets and the credential check must agree on the device id,
    // otherwise padding the claimed id sidesteps the failure series.
    let device_id = request.device_id.trim();
    if device_id.is_empty() || device_id.len() > 128 {
        return Err(web_error(
            StatusCode::UNAUTHORIZED,
            "invalid device credentials",
        ));
    }
    let now = now_ms().map_err(web_api_error)?;
    {
        let throttle = state.login_attempts.lock().map_err(|_| web_lock_error())?;
        if throttle.blocked(device_id, now) {
            return Err(web_error(
                StatusCode::TOO_MANY_REQUESTS,
                "too many attempts",
            ));
        }
    }
    let authorized = state
        .store
        .lock()
        .map_err(|_| web_error(StatusCode::INTERNAL_SERVER_ERROR, "storage lock error"))?
        .authorize_device(device_id, &request.token)
        .map_err(|err| web_api_error(map_error(err)))?;
    if !authorized {
        state
            .login_attempts
            .lock()
            .map_err(|_| web_lock_error())?
            .record_failure(device_id, now);
        return Err(web_error(
            StatusCode::UNAUTHORIZED,
            "invalid device credentials",
        ));
    }
    state
        .login_attempts
        .lock()
        .map_err(|_| web_lock_error())?
        .reset(device_id);
    let (token, expires_at_ms) = {
        let mut sessions = state.sessions.lock().map_err(|_| web_lock_error())?;
        let ttl = sessions.ttl_ms();
        let absolute_ttl = sessions.absolute_ttl_ms();
        let created = sessions
            .create(device_id, now, ttl, absolute_ttl)
            .map_err(|err| web_api_error(session_error(err)))?;
        (created.token, created.expires_at_ms)
    };
    let cookie = axum::http::HeaderValue::from_str(&session_cookie_header(
        &token,
        state.cookie_secure,
        remaining_seconds(expires_at_ms, now),
    ))
    .map_err(|_| web_error(StatusCode::INTERNAL_SERVER_ERROR, "session error"))?;
    let mut response = (
        StatusCode::CREATED,
        Json(WebSessionResponse {
            device_id: device_id.to_owned(),
            expires_at_ms,
        }),
    )
        .into_response();
    response.headers_mut().insert(header::SET_COOKIE, cookie);
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        axum::http::HeaderValue::from_static("no-store"),
    );
    Ok(response)
}
async fn get_web_session(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, WebFailure> {
    let now = now_ms().map_err(web_api_error)?;
    let Some(token) = session_cookie(&headers) else {
        return Err(web_session_unauthorized());
    };
    let session = state
        .sessions
        .lock()
        .map_err(|_| web_lock_error())?
        .authenticate(&token, now)
        .map_err(|err| web_api_error(session_error(err)))?;
    let Some(Authenticated {
        device_id,
        expires_at_ms,
    }) = session
    else {
        return Err(web_session_unauthorized());
    };
    let active = state
        .store
        .lock()
        .map_err(|_| web_error(StatusCode::INTERNAL_SERVER_ERROR, "storage lock error"))?
        .device_active(&device_id)
        .map_err(|err| web_api_error(map_error(err)))?;
    if !active {
        // Revoked device: drop the session and clear the browser cookie.
        if let Ok(mut sessions) = state.sessions.lock() {
            let _ = sessions.revoke(&token, now);
        }
        return Err(web_session_unauthorized());
    }
    let mut response = Json(WebSessionResponse {
        device_id,
        expires_at_ms,
    })
    .into_response();
    // Sliding renewal is reflected client-side: the browser keeps the cookie
    // for the new server deadline instead of the deadline it was first issued.
    let cookie = axum::http::HeaderValue::from_str(&session_cookie_header(
        &token,
        state.cookie_secure,
        remaining_seconds(expires_at_ms, now),
    ))
    .map_err(|_| web_error(StatusCode::INTERNAL_SERVER_ERROR, "session error"))?;
    response.headers_mut().insert(header::SET_COOKIE, cookie);
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        axum::http::HeaderValue::from_static("no-store"),
    );
    Ok(response)
}
async fn delete_web_session(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, WebFailure> {
    let now = now_ms().map_err(web_api_error)?;
    if let Some(token) = session_cookie(&headers) {
        state
            .sessions
            .lock()
            .map_err(|_| web_lock_error())?
            .revoke(&token, now)
            .map_err(|err| web_api_error(session_error(err)))?;
    }
    let mut response = StatusCode::NO_CONTENT.into_response();
    response.headers_mut().insert(
        header::SET_COOKIE,
        axum::http::HeaderValue::from_static(CLEARED_SESSION_COOKIE),
    );
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        axum::http::HeaderValue::from_static("no-store"),
    );
    Ok(response)
}

/// Cookie-authenticated browser mutations need a custom header (impossible to
/// set on a cross-site form post) and must not be cross-site fetches.
async fn csrf_guard(State(_state): State<AppState>, request: Request, next: Next) -> Response {
    if csrf_violation(request.method(), request.headers(), request.uri().path()) {
        return error(StatusCode::FORBIDDEN, "csrf check failed").into_response();
    }
    next.run(request).await
}
fn csrf_violation(method: &Method, headers: &HeaderMap, path: &str) -> bool {
    if matches!(method.as_str(), "GET" | "HEAD" | "OPTIONS") {
        return false;
    }
    // Login has no ambient cookie authority yet and requires device credentials.
    if method.as_str() == "POST" && path == "/v1/web/session" {
        return false;
    }
    // Bearer clients (CLI, MCP workers) are not browsers and are unaffected.
    if headers.contains_key(header::AUTHORIZATION) || session_cookie(headers).is_none() {
        return false;
    }
    if headers
        .get("sec-fetch-site")
        .and_then(|value| value.to_str().ok())
        == Some("cross-site")
    {
        return true;
    }
    headers
        .get("x-orbit-csrf")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        != Some("1")
}

async fn console() -> impl IntoResponse {
    (
        [
            (header::CACHE_CONTROL, "no-store"),
            (
                header::CONTENT_SECURITY_POLICY,
                "default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; connect-src 'self'; form-action 'none'; frame-ancestors 'none'",
            ),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
        ],
        Html(include_str!("../static/console.html")),
    )
}
async fn health() -> Json<StatusResponse> {
    Json(StatusResponse { status: "ok" })
}
async fn enroll(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<EnrollRequest>,
) -> Result<StatusCode, ApiError> {
    admin(&state, &headers)?;
    let mut store = state
        .store
        .lock()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "storage lock error"))?;
    store
        .enroll_device(&request.device_id, &request.token)
        .map_err(map_error)?;
    Ok(StatusCode::CREATED)
}
async fn revoke(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<StatusCode, ApiError> {
    admin(&state, &headers)?;
    let mut store = state
        .store
        .lock()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "storage lock error"))?;
    if store.revoke_device(&id).map_err(map_error)? {
        revoke_device_sessions(&state, &id);
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(error(
            StatusCode::NOT_FOUND,
            "device not found or already revoked",
        ))
    }
}
async fn rotate_token(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Path(id): axum::extract::Path<String>,
    Json(request): Json<RotateTokenRequest>,
) -> Result<StatusCode, ApiError> {
    admin(&state, &headers)?;
    let mut store = state
        .store
        .lock()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "storage lock error"))?;
    if store
        .rotate_device_token(&id, &request.token)
        .map_err(map_error)?
    {
        // A rotated credential invalidates delegated browser sessions too.
        revoke_device_sessions(&state, &id);
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(error(StatusCode::NOT_FOUND, "active device not found"))
    }
}

fn revoke_device_sessions(state: &AppState, device_id: &str) {
    match state.sessions.lock() {
        Ok(mut sessions) => {
            if let Err(error) = sessions.revoke_device_sessions(device_id) {
                eprintln!("orbit-server session revoke error: {error:?}");
            }
        }
        Err(_) => eprintln!("orbit-server session lock error during device revoke"),
    }
}

fn now_ms() -> Result<i64, ApiError> {
    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "clock unavailable"))?
        .as_millis();
    i64::try_from(ms).map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "clock overflow"))
}
fn queue_error(e: QueueError) -> ApiError {
    match e {
        QueueError::Invalid(_) => error(StatusCode::BAD_REQUEST, "invalid automation request"),
        QueueError::Conflict => error(StatusCode::CONFLICT, "automation conflict"),
        QueueError::Sql(e) => {
            eprintln!("automation storage error: {e}");
            error(StatusCode::INTERNAL_SERVER_ERROR, "storage error")
        }
    }
}
async fn put_job(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<JobInput>,
) -> Result<Json<Job>, ApiError> {
    admin(&state, &headers)?;
    let mut queue = state
        .automation
        .lock()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "automation lock error"))?;
    Ok(Json(queue.put(req, now_ms()?).map_err(queue_error)?))
}
async fn list_jobs(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<Job>>, ApiError> {
    admin(&state, &headers)?;
    let queue = state
        .automation
        .lock()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "automation lock error"))?;
    Ok(Json(queue.jobs().map_err(queue_error)?))
}
async fn disable_job(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<StatusCode, ApiError> {
    admin(&state, &headers)?;
    let mut queue = state
        .automation
        .lock()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "automation lock error"))?;
    if queue.delete(&id).map_err(queue_error)? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(error(StatusCode::NOT_FOUND, "enabled job not found"))
    }
}
async fn refresh_job(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<Json<Run>, ApiError> {
    admin(&state, &headers)?;
    let mut queue = state
        .automation
        .lock()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "automation lock error"))?;
    Ok(Json(
        queue.request_refresh(&id, now_ms()?).map_err(queue_error)?,
    ))
}
async fn list_runs(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<Run>>, ApiError> {
    admin(&state, &headers)?;
    let queue = state
        .automation
        .lock()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "automation lock error"))?;
    Ok(Json(queue.runs(100).map_err(queue_error)?))
}
async fn claim_run(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Option<Run>>, ApiError> {
    let worker = device(&state, &headers)?;
    let mut queue = state
        .automation
        .lock()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "automation lock error"))?;
    Ok(Json(queue.claim(&worker, now_ms()?).map_err(queue_error)?))
}
async fn settle_run(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Path(id): axum::extract::Path<i64>,
    Json(req): Json<SettleInput>,
) -> Result<StatusCode, ApiError> {
    let worker = device(&state, &headers)?;
    let mut queue = state
        .automation
        .lock()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "automation lock error"))?;
    if queue
        .settle(
            id,
            &worker,
            now_ms()?,
            req.succeeded,
            req.result_json.as_deref(),
        )
        .map_err(queue_error)?
    {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(error(
            StatusCode::CONFLICT,
            "run not leased by this worker or lease expired",
        ))
    }
}
fn proposal_error(e: ProposalError) -> ApiError {
    match e {
        ProposalError::Invalid => error(StatusCode::BAD_REQUEST, "invalid or forbidden proposal"),
        ProposalError::Conflict => error(StatusCode::CONFLICT, "proposal state conflict"),
        ProposalError::Sql(e) => {
            eprintln!("proposal storage error: {e}");
            error(StatusCode::INTERNAL_SERVER_ERROR, "storage error")
        }
    }
}
async fn propose_action(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<NewProposal>,
) -> Result<Json<Proposal>, ApiError> {
    let owner = device(&state, &headers)?;
    let mut store = state
        .proposals
        .lock()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "proposal lock error"))?;
    Ok(Json(
        store
            .propose(&owner, req, now_ms()?)
            .map_err(proposal_error)?,
    ))
}
// The browser administrator can submit a proposal, but submission never approves it.
// Approval is a separate, explicit audited operation.
async fn propose_admin_action(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<NewProposal>,
) -> Result<Json<Proposal>, ApiError> {
    admin(&state, &headers)?;
    let mut store = state
        .proposals
        .lock()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "proposal lock error"))?;
    Ok(Json(
        store
            .propose("admin-console", req, now_ms()?)
            .map_err(proposal_error)?,
    ))
}
async fn list_proposals(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<Proposal>>, ApiError> {
    admin(&state, &headers)?;
    let store = state
        .proposals
        .lock()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "proposal lock error"))?;
    Ok(Json(store.list().map_err(proposal_error)?))
}
#[derive(Deserialize)]
struct Decision {
    approve: bool,
}
async fn decide_proposal(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Path(id): axum::extract::Path<i64>,
    Json(req): Json<Decision>,
) -> Result<StatusCode, ApiError> {
    admin(&state, &headers)?;
    let mut store = state
        .proposals
        .lock()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "proposal lock error"))?;
    if store
        .decide(id, req.approve, now_ms()?)
        .map_err(proposal_error)?
    {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(error(StatusCode::CONFLICT, "proposal not pending"))
    }
}
async fn proposal_audit(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Path(id): axum::extract::Path<i64>,
) -> Result<Json<Vec<Audit>>, ApiError> {
    admin(&state, &headers)?;
    let store = state
        .proposals
        .lock()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "proposal lock error"))?;
    Ok(Json(store.audit(id).map_err(proposal_error)?))
}
async fn claim_proposal(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Option<Proposal>>, ApiError> {
    let worker = device(&state, &headers)?;
    let mut store = state
        .proposals
        .lock()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "proposal lock error"))?;
    Ok(Json(
        store.claim(&worker, now_ms()?).map_err(proposal_error)?,
    ))
}
#[derive(Deserialize)]
struct ProposalSettlement {
    succeeded: bool,
    result: serde_json::Value,
}
async fn settle_proposal(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Path(id): axum::extract::Path<i64>,
    Json(req): Json<ProposalSettlement>,
) -> Result<StatusCode, ApiError> {
    let worker = device(&state, &headers)?;
    let mut store = state
        .proposals
        .lock()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "proposal lock error"))?;
    if store
        .settle(id, &worker, now_ms()?, req.succeeded, &req.result)
        .map_err(proposal_error)?
    {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(error(
            StatusCode::CONFLICT,
            "proposal not leased by this worker",
        ))
    }
}
// Offline operator commands. Restore requires the server process to be stopped;
// never replace an active SQLite database or leave its WAL sidecar inconsistent.
fn operator_command() -> Result<bool, Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() == 1 {
        return Ok(false);
    }
    if args.len() != 3 || !matches!(args[1].as_str(), "backup" | "restore") {
        return Err("usage: orbit-server [backup <snapshot.db> | restore <snapshot.db>]".into());
    }
    let db = std::env::var("ORBIT_SERVER_DB").unwrap_or_else(|_| "orbit-server.db".into());
    let source_or_dest = Path::new(&args[2]);
    let database = Path::new(&db);
    if source_or_dest == database {
        return Err("backup path must differ from server database".into());
    }
    match args[1].as_str() {
        "backup" => {
            let store =
                AuthoritativeHost::open(database).map_err(|e| format!("open database: {e:?}"))?;
            store
                .backup_to(source_or_dest)
                .map_err(|e| format!("backup failed: {e:?}"))?;
            eprintln!("orbit-server backup written: {}", source_or_dest.display());
        }
        "restore" => {
            AuthoritativeHost::validate_backup(source_or_dest)
                .map_err(|e| format!("invalid backup: {e:?}"))?;
            if std::path::PathBuf::from(format!("{}-wal", database.display())).exists()
                || std::path::PathBuf::from(format!("{}-shm", database.display())).exists()
            {
                return Err("SQLite sidecar files detected: stop server and checkpoint/remove WAL safely before restoring".into());
            }
            if database.exists() {
                return Err("restore refuses to overwrite database; move existing database aside while server is stopped".into());
            }
            if let Some(parent) = database.parent().filter(|p| !p.as_os_str().is_empty()) {
                std::fs::create_dir_all(parent)?;
            }
            let temporary = database.with_extension("restore-tmp");
            if temporary.exists() {
                return Err("temporary restore target exists".into());
            }
            std::fs::copy(source_or_dest, &temporary)?;
            if let Err(err) = AuthoritativeHost::validate_backup(&temporary) {
                let _ = std::fs::remove_file(&temporary);
                return Err(format!("copied backup failed validation: {err:?}").into());
            }
            std::fs::rename(&temporary, database)?;
            eprintln!("orbit-server restore complete: {}", database.display());
        }
        _ => unreachable!(),
    }
    Ok(true)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    if operator_command()? {
        return Ok(());
    }
    let admin_key = std::env::var("ORBIT_SERVER_ADMIN_KEY")
        .map_err(|_| "ORBIT_SERVER_ADMIN_KEY must be configured")?;
    if admin_key.len() < 32 {
        return Err("ORBIT_SERVER_ADMIN_KEY must be at least 32 characters".into());
    }
    let database = std::env::var("ORBIT_SERVER_DB").unwrap_or_else(|_| "orbit-server.db".into());
    if let Some(parent) = Path::new(&database)
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent)?;
    }
    let attachment_body_limit = std::env::var("ORBIT_ATTACHMENT_MAX_BYTES")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(64 * 1024 * 1024);
    let state = AppState {
        store: Arc::new(Mutex::new(
            AuthoritativeHost::open(Path::new(&database))
                .map_err(|e| format!("storage open: {e:?}"))?,
        )),
        automation: Arc::new(Mutex::new(
            AutomationStore::open(Path::new(&database))
                .map_err(|e| format!("automation open: {e:?}"))?,
        )),
        proposals: Arc::new(Mutex::new(
            ProposalStore::open(Path::new(&database))
                .map_err(|e| format!("proposals open: {e:?}"))?,
        )),
        sessions: Arc::new(Mutex::new(
            SessionStore::open(Path::new(&database))
                .map_err(|e| format!("sessions open: {e:?}"))?,
        )),
        login_attempts: Arc::new(Mutex::new(LoginThrottle::new())),
        web_dist: Arc::new(web::WebDist::from_env()),
        cookie_secure: std::env::var("ORBIT_WEB_COOKIE_SECURE").as_deref() == Ok("1"),
        admin_key_hash: Sha256::digest(admin_key.as_bytes()).to_vec(),
        attachment_body_limit,
    };
    // A single scheduler writes durable due runs; no cloud service needed.
    let scheduler = state.automation.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(15));
        loop {
            interval.tick().await;
            if let Ok(duration) = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
                && let Ok(now) = i64::try_from(duration.as_millis())
                && let Ok(mut queue) = scheduler.lock()
                && let Err(err) = queue.tick(now)
            {
                eprintln!("automation scheduling error: {err:?}");
            }
        }
    });
    let app = routes(state);
    // Intentionally loopback by default. Put the service behind a TLS
    // reverse-proxy/private-network gate before making it remotely reachable.
    let bind = std::env::var("ORBIT_SERVER_BIND").unwrap_or_else(|_| "127.0.0.1:8787".into());
    let listener = tokio::net::TcpListener::bind(&bind).await?;
    eprintln!("orbit-server authoritative runtime listening on {bind}");
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Connected workflow capability surface (M32 I4).
//
// These handlers are transport only. They authenticate, shape the wire contract
// and delegate every decision to `orbit_runtime::capability_server`, which has
// no executor registry and no `TrustedComposition`: the server persists an
// authorized, fenced intent, and a separately enrolled executor claims and runs
// it. No handler here can call a provider, and none of them may learn one from a
// request body.
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CapabilityProjectQuery {
    project_id: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CapabilityItemQuery {
    item_id: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CapabilityShowQuery {
    command_id: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CapabilityListQuery {
    item_id: Option<String>,
    state: Option<String>,
}
/// Announcement names only capabilities. Unknown field names (a provider, an
/// action list, a status) are refused by the extractor, not ignored.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CapabilityAnnounceBody {
    capability_ids: Vec<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CapabilityInstallBody {
    command_id: String,
    capability_id: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CapabilityAuthorizeBody {
    command_id: String,
    capability_id: String,
    project_id: String,
    #[serde(default)]
    confirm: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CapabilityRevokeBody {
    command_id: String,
    capability_id: String,
    project_id: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CapabilityRequestBody {
    command_id: String,
    item_id: String,
    expected_revision: u64,
    action_id: String,
    #[serde(default)]
    inputs: std::collections::BTreeMap<String, serde_json::Value>,
    #[serde(default)]
    confirm: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CapabilityClaimBody {
    command_id: String,
    claim_token: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CapabilityResultBody {
    command_id: String,
    claim_token: String,
    outcome: orbit_runtime::capability_server::CapabilityOutcome,
    #[serde(default)]
    detail: String,
    /// The declared effect the executor produced, if it produced one. It is
    /// validated against the capability's compile-time declaration: a body can
    /// name no kind, target, path or table that the contract does not already
    /// describe.
    #[serde(default)]
    effect: Option<serde_json::Value>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CapabilityReconcileBody {
    command_id: String,
    decision: orbit_runtime::capability_composition::CapabilityReconcileDecision,
    #[serde(default)]
    detail: String,
}

/// Operator writes are naturally idempotent on their natural key (the contract,
/// the grant, the revocation), so the envelope `command_id` is not a fence here.
/// It is still validated: an empty or unbounded id is a malformed envelope, not
/// a permission to install anything.
fn capability_command_envelope(command_id: &str) -> Result<(), ApiError> {
    if orbit_runtime::capability_contracts::valid_command_id(command_id) {
        Ok(())
    } else {
        Err(error(StatusCode::BAD_REQUEST, "invalid capability command"))
    }
}

/// Map a capability refusal onto the transport status without collapsing it.
/// Availability codes, input reasons and the mixed-case prose that explains them
/// reach the client verbatim so a browser never has to parse a generic message.
fn capability_error(failure: CapabilityServerError) -> ApiError {
    use orbit_application::RepositoryError;
    use orbit_runtime::capability_server::CapabilityServerError as Failure;
    match failure {
        Failure::BadRequest(message) => error(StatusCode::BAD_REQUEST, message),
        Failure::InvalidInput { reason } => error(
            StatusCode::BAD_REQUEST,
            format!("invalid capability action input: {reason}"),
        ),
        Failure::Unavailable { code, reason } => error(
            StatusCode::BAD_REQUEST,
            format!("capability action unavailable: {code}: {reason}"),
        ),
        Failure::Forbidden(message) => error(StatusCode::FORBIDDEN, message),
        Failure::NotFound(message) => error(StatusCode::NOT_FOUND, message),
        Failure::Conflict(message) => error(StatusCode::CONFLICT, message),
        Failure::Storage(RepositoryError::Unavailable) => error(
            StatusCode::SERVICE_UNAVAILABLE,
            "capability storage unavailable",
        ),
        Failure::Storage(RepositoryError::Conflict) => {
            error(StatusCode::CONFLICT, "capability operation conflict")
        }
        Failure::Storage(failure) => {
            // Never expose SQL, a local path or a debug dump in a response body.
            eprintln!("orbit-server capability error: {failure:?}");
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "capability storage error",
            )
        }
    }
}

fn capability_host(
    state: &AppState,
) -> Result<std::sync::MutexGuard<'_, AuthoritativeHost>, ApiError> {
    state
        .store
        .lock()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "storage lock error"))
}

/// Executor routes require a bearer credential plus an active enrolled device.
/// A browser cookie session is explicitly refused: being logged in never makes a
/// browser an executor, and provider code must not run behind a cookie.
fn executor_device(state: &AppState, headers: &HeaderMap) -> Result<String, ApiError> {
    if headers.contains_key(header::AUTHORIZATION) {
        return bearer_device(state, headers);
    }
    if session_cookie(headers).is_some() {
        return Err(error(StatusCode::FORBIDDEN, "executor credential required"));
    }
    Err(error(StatusCode::UNAUTHORIZED, "unauthorized"))
}

/// Installation/authorization/revocation change what the workspace may execute at
/// all, so they are operator writes: bearer plus device id, never a cookie.
fn operator_device(state: &AppState, headers: &HeaderMap) -> Result<String, ApiError> {
    if headers.contains_key(header::AUTHORIZATION) {
        return bearer_device(state, headers);
    }
    if session_cookie(headers).is_some() {
        return Err(error(StatusCode::FORBIDDEN, "operator credential required"));
    }
    Err(error(StatusCode::UNAUTHORIZED, "unauthorized"))
}

async fn authoritative_capabilities(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<CapabilityProjectQuery>,
) -> Result<Json<orbit_runtime::capability_server::CapabilityStatusResponse>, ApiError> {
    device(&state, &headers)?;
    let host = capability_host(&state)?;
    Ok(Json(
        orbit_runtime::capability_server::status(&host, query.project_id.as_deref())
            .map_err(capability_error)?,
    ))
}

async fn authoritative_capability_executors(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<orbit_runtime::capability_server::CapabilityExecutorRosterResponse>, ApiError> {
    device(&state, &headers)?;
    let host = capability_host(&state)?;
    Ok(Json(
        orbit_runtime::capability_server::executor_roster(&host).map_err(capability_error)?,
    ))
}

async fn authoritative_capability_announce(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<CapabilityAnnounceBody>,
) -> Result<Json<orbit_runtime::capability_server::CapabilityAnnounceResponse>, ApiError> {
    let device_id = executor_device(&state, &headers)?;
    let at_ms = now_ms()?;
    let mut host = capability_host(&state)?;
    Ok(Json(
        orbit_runtime::capability_server::announce_executors(
            &mut host,
            &device_id,
            &body.capability_ids,
            at_ms,
        )
        .map_err(capability_error)?,
    ))
}

async fn authoritative_capability_actions(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<CapabilityItemQuery>,
) -> Result<Json<orbit_runtime::capability_server::CapabilityOffersResponse>, ApiError> {
    device(&state, &headers)?;
    let host = capability_host(&state)?;
    Ok(Json(
        orbit_runtime::capability_server::offers(&host, &query.item_id)
            .map_err(capability_error)?,
    ))
}

async fn authoritative_capability_request(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<CapabilityRequestBody>,
) -> Result<Json<orbit_runtime::capability_server::CapabilityRequestResponse>, ApiError> {
    device(&state, &headers)?;
    let mut host = capability_host(&state)?;
    Ok(Json(
        orbit_runtime::capability_server::request(
            &mut host,
            orbit_runtime::capability_server::ServerCapabilityRequest {
                command_id: body.command_id,
                item_id: body.item_id,
                expected_revision: body.expected_revision,
                action_id: body.action_id,
                inputs: body.inputs,
                confirm: body.confirm,
            },
        )
        .map_err(capability_error)?,
    ))
}

async fn authoritative_capability_claim(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<CapabilityClaimBody>,
) -> Result<Json<orbit_runtime::capability_server::CapabilityClaimResponse>, ApiError> {
    let device_id = executor_device(&state, &headers)?;
    let mut host = capability_host(&state)?;
    Ok(Json(
        orbit_runtime::capability_server::claim(
            &mut host,
            &device_id,
            &body.command_id,
            &body.claim_token,
        )
        .map_err(capability_error)?,
    ))
}

async fn authoritative_capability_result(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<CapabilityResultBody>,
) -> Result<Json<orbit_runtime::capability_server::CapabilityResultResponse>, ApiError> {
    let device_id = executor_device(&state, &headers)?;
    let at_ms = now_ms()?;
    let mut host = capability_host(&state)?;
    Ok(Json(
        orbit_runtime::capability_server::result(
            &mut host,
            &device_id,
            &body.command_id,
            &body.claim_token,
            body.outcome,
            &body.detail,
            body.effect.as_ref(),
            at_ms,
        )
        .map_err(capability_error)?,
    ))
}

/// The durable effects recorded for one work item. This is a read of rows the
/// authority already committed, so a browser or an acceptance test can prove the
/// result of an executor's action without reading the database.
async fn authoritative_capability_effects(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<CapabilityItemQuery>,
) -> Result<Json<orbit_runtime::capability_server::CapabilityEffectsResponse>, ApiError> {
    device(&state, &headers)?;
    let host = capability_host(&state)?;
    Ok(Json(
        orbit_runtime::capability_server::effects(&host, &query.item_id)
            .map_err(capability_error)?,
    ))
}

async fn authoritative_capability_show(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<CapabilityShowQuery>,
) -> Result<Json<orbit_runtime::capability_server::CapabilityShowResponse>, ApiError> {
    device(&state, &headers)?;
    let host = capability_host(&state)?;
    Ok(Json(
        orbit_runtime::capability_server::show(&host, &query.command_id)
            .map_err(capability_error)?,
    ))
}

async fn authoritative_capability_list(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<CapabilityListQuery>,
) -> Result<Json<orbit_runtime::capability_server::CapabilityListResponse>, ApiError> {
    device(&state, &headers)?;
    let host = capability_host(&state)?;
    Ok(Json(
        orbit_runtime::capability_server::list(
            &host,
            query.item_id.as_deref(),
            query.state.as_deref(),
        )
        .map_err(capability_error)?,
    ))
}

async fn authoritative_capability_reconcile(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<CapabilityReconcileBody>,
) -> Result<Json<orbit_runtime::capability_server::CapabilityShowResponse>, ApiError> {
    // Reconciliation is an operator decision, not an executor one: the
    // authenticated device is recorded as the actor who decided.
    let device_id = device(&state, &headers)?;
    let at_ms = now_ms()?;
    let mut host = capability_host(&state)?;
    Ok(Json(
        orbit_runtime::capability_server::reconcile(
            &mut host,
            &device_id,
            &body.command_id,
            body.decision,
            &body.detail,
            at_ms,
        )
        .map_err(capability_error)?,
    ))
}

async fn authoritative_capability_install(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<CapabilityInstallBody>,
) -> Result<Json<orbit_runtime::capability_server::CapabilityInstallResponse>, ApiError> {
    operator_device(&state, &headers)?;
    capability_command_envelope(&body.command_id)?;
    let at_ms = now_ms()?;
    let mut host = capability_host(&state)?;
    Ok(Json(
        orbit_runtime::capability_server::install(&mut host, &body.capability_id, at_ms)
            .map_err(capability_error)?,
    ))
}

async fn authoritative_capability_authorize(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<CapabilityAuthorizeBody>,
) -> Result<Json<orbit_runtime::capability_server::CapabilityGrantResponse>, ApiError> {
    let actor = operator_device(&state, &headers)?;
    capability_command_envelope(&body.command_id)?;
    let at_ms = now_ms()?;
    let mut host = capability_host(&state)?;
    Ok(Json(
        orbit_runtime::capability_server::authorize(
            &mut host,
            &body.capability_id,
            &body.project_id,
            &actor,
            body.confirm,
            at_ms,
        )
        .map_err(capability_error)?,
    ))
}

async fn authoritative_capability_revoke(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<CapabilityRevokeBody>,
) -> Result<Json<orbit_runtime::capability_server::CapabilityRevokeResponse>, ApiError> {
    operator_device(&state, &headers)?;
    capability_command_envelope(&body.command_id)?;
    let mut host = capability_host(&state)?;
    Ok(Json(
        orbit_runtime::capability_server::revoke(&mut host, &body.capability_id, &body.project_id)
            .map_err(capability_error)?,
    ))
}

fn routes(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/console", get(console))
        .route(
            "/v1/web/session",
            post(create_web_session)
                .get(get_web_session)
                .delete(delete_web_session),
        )
        .route("/v1/devices", post(enroll))
        .route("/v1/devices/{id}", axum::routing::delete(revoke))
        .route("/v1/devices/{id}/token", post(rotate_token))
        .route("/v1/authoritative/snapshot", get(authoritative_snapshot))
        .route(
            "/v1/authoritative/history-prefix",
            get(authoritative_history_prefix),
        )
        .route("/v1/authoritative/evidence", get(authoritative_evidence))
        .route(
            "/v1/authoritative/evidence-commands",
            post(authoritative_evidence_command),
        )
        .route("/v1/authoritative/projects", get(authoritative_projects))
        .route(
            "/v1/authoritative/task-transitions",
            get(authoritative_task_transitions),
        )
        .route(
            "/v1/authoritative/project-commands",
            post(authoritative_project_command),
        )
        .route(
            "/v1/authoritative/project-statuses",
            get(authoritative_project_statuses),
        )
        .route("/v1/authoritative/search", get(authoritative_search))
        .route(
            "/v1/authoritative/saved-views",
            get(authoritative_saved_views),
        )
        .route(
            "/v1/authoritative/saved-views/commands",
            post(authoritative_saved_view_command),
        )
        .route(
            "/v1/authoritative/knowledge-tree",
            get(authoritative_knowledge_tree),
        )
        .route(
            "/v1/authoritative/markdown-document",
            get(authoritative_markdown_document),
        )
        .route(
            "/v1/authoritative/markdown-by-id",
            get(authoritative_markdown_by_id),
        )
        .route(
            "/v1/authoritative/markdown-inventory",
            get(authoritative_markdown_inventory),
        )
        .route(
            "/v1/authoritative/markdown-manifest",
            get(authoritative_markdown_manifest),
        )
        .route(
            "/v1/authoritative/markdown-search",
            post(authoritative_markdown_search),
        )
        .route(
            "/v1/authoritative/markdown-commands",
            post(authoritative_markdown_command),
        )
        .route(
            "/v1/authoritative/folder-moves",
            post(authoritative_folder_move),
        )
        .route(
            "/v1/authoritative/folder-creates",
            post(authoritative_folder_create),
        )
        .route(
            "/v1/authoritative/markdown-relations",
            get(authoritative_markdown_relations),
        )
        .route(
            "/v1/authoritative/markdown-references",
            get(authoritative_markdown_references),
        )
        .route(
            "/v1/authoritative/markdown-reference-commands",
            post(authoritative_markdown_reference_command),
        )
        .route(
            "/v1/authoritative/project-knowledge",
            get(authoritative_project_knowledge),
        )
        .route(
            "/v1/authoritative/strategies",
            get(authoritative_strategies),
        )
        .route(
            "/v1/authoritative/strategy-commands",
            post(authoritative_strategy_command),
        )
        .route(
            "/v1/authoritative/milestones",
            get(authoritative_milestones),
        )
        .route(
            "/v1/authoritative/milestone-commands",
            post(authoritative_milestone_command),
        )
        .route("/v1/authoritative/routines", get(authoritative_routines))
        .route(
            "/v1/authoritative/routine-commands",
            post(authoritative_routine_command),
        )
        .route("/v1/authoritative/schedule", get(authoritative_schedule))
        .route(
            "/v1/authoritative/schedule-conflicts",
            get(authoritative_schedule_conflicts),
        )
        .route(
            "/v1/authoritative/schedule-commands",
            post(authoritative_schedule_command),
        )
        .route("/v1/authoritative/tasks", get(authoritative_tasks))
        .route(
            "/v1/authoritative/tasks/{id}",
            get(authoritative_task_detail),
        )
        .route(
            "/v1/authoritative/tasks/{id}/acceptance-criteria",
            get(authoritative_task_acceptance_criteria),
        )
        .route(
            "/v1/authoritative/tasks/{id}/activity",
            get(authoritative_task_activity),
        )
        .route(
            "/v1/authoritative/attachment-import",
            // Route-scoped limit: only attachment bytes bypass axum's 2 MiB
            // default, and they are still bounded before buffering.
            post(authoritative_attachment_import)
                .layer(DefaultBodyLimit::max(state.attachment_body_limit)),
        )
        .route(
            "/v1/authoritative/attachments",
            get(authoritative_attachment_list),
        )
        .route(
            "/v1/authoritative/attachments/{id}/content",
            get(authoritative_attachment_content),
        )
        .route(
            "/v1/authoritative/attachment-remove",
            post(authoritative_attachment_remove),
        )
        .route(
            "/v1/authoritative/attachment-associate",
            post(authoritative_attachment_associate),
        )
        .route(
            "/v1/authoritative/task-dependencies",
            get(authoritative_task_dependencies),
        )
        .route(
            "/v1/authoritative/task-readiness",
            get(authoritative_task_readiness),
        )
        .route(
            "/v1/authoritative/task-graph",
            get(authoritative_task_graph),
        )
        .route(
            "/v1/authoritative/project-dependencies",
            get(authoritative_project_dependencies),
        )
        .route(
            "/v1/authoritative/project-handoffs",
            get(authoritative_project_handoffs),
        )
        .route(
            "/v1/authoritative/next-actions",
            get(authoritative_next_actions),
        )
        .route(
            "/v1/authoritative/portfolio-actions",
            get(authoritative_portfolio_actions),
        )
        .route(
            "/v1/authoritative/portfolio-capacity",
            get(authoritative_portfolio_capacity),
        )
        .route(
            "/v1/authoritative/planning-scenarios",
            post(authoritative_compare_scenarios),
        )
        .route(
            "/v1/authoritative/plan-review",
            post(authoritative_review_schedule_plan),
        )
        .route(
            "/v1/authoritative/scenario-execution-review",
            post(authoritative_review_scenario_execution),
        )
        .route(
            "/v1/authoritative/scenario-options-review",
            post(authoritative_review_scenario_options),
        )
        .route(
            "/v1/authoritative/plan-apply-approved",
            post(authoritative_apply_approved_schedule),
        )
        .route("/v1/authoritative/day-plan", get(authoritative_day_plan))
        .route(
            "/v1/authoritative/work-brief",
            get(authoritative_work_brief),
        )
        .route(
            "/v1/authoritative/planning-focus",
            get(authoritative_planning_focus),
        )
        .route(
            "/v1/authoritative/planning-task-actions",
            get(authoritative_planning_task_actions),
        )
        .route(
            "/v1/authoritative/planning-handoff-task",
            post(authoritative_handoff_task_action),
        )
        .route(
            "/v1/authoritative/planning-handoff-group",
            post(authoritative_handoff_group_action),
        )
        .route(
            "/v1/authoritative/planning-resolve-task-action",
            post(authoritative_resolve_task_action),
        )
        .route(
            "/v1/authoritative/planning-resolve-group-action",
            post(authoritative_resolve_group_action),
        )
        .route(
            "/v1/authoritative/planning-group-actions",
            get(authoritative_planning_group_actions),
        )
        .route(
            "/v1/authoritative/planning-group",
            get(authoritative_planning_group),
        )
        .route(
            "/v1/authoritative/completion-impact",
            get(authoritative_completion_impact),
        )
        .route(
            "/v1/authoritative/schedule-plan",
            get(authoritative_schedule_plan),
        )
        .route("/v1/authoritative/workload", get(authoritative_workload))
        .route(
            "/v1/authoritative/task-commands",
            post(authoritative_task_command),
        )
        .route(
            "/v1/authoritative/agent-machines/prompts",
            get(authoritative_machine_prompts),
        )
        .route(
            "/v1/authoritative/agent-machines/prompt-claim",
            post(authoritative_machine_claim_prompt),
        )
        .route(
            "/v1/authoritative/agent-machines/prompt-result",
            post(authoritative_machine_report_prompt),
        )
        .route(
            "/v1/authoritative/agent-jobs/continue",
            post(authoritative_agent_continue),
        )
        .route(
            "/v1/authoritative/agent-machines/inbox",
            get(authoritative_machine_agent_inbox),
        )
        .route(
            "/v1/authoritative/agent-machines/brief",
            get(authoritative_machine_agent_brief),
        )
        .route(
            "/v1/authoritative/agent-machines/claim",
            post(authoritative_machine_agent_claim),
        )
        .route(
            "/v1/authoritative/agent-machines/launch-result",
            post(authoritative_machine_agent_launch_result),
        )
        .route(
            "/v1/authoritative/agent-machines/observe",
            post(authoritative_machine_agent_observe),
        )
        .route(
            "/v1/authoritative/agent-machines/handoff",
            post(authoritative_machine_agent_handoff),
        )
        .route(
            "/v1/authoritative/capabilities",
            get(authoritative_capabilities),
        )
        .route(
            "/v1/authoritative/capabilities/install",
            post(authoritative_capability_install),
        )
        .route(
            "/v1/authoritative/capabilities/authorize",
            post(authoritative_capability_authorize),
        )
        .route(
            "/v1/authoritative/capabilities/revoke",
            post(authoritative_capability_revoke),
        )
        .route(
            "/v1/authoritative/capability-executors",
            get(authoritative_capability_executors),
        )
        .route(
            "/v1/authoritative/capability-executors/announce",
            post(authoritative_capability_announce),
        )
        .route(
            "/v1/authoritative/capability-actions",
            get(authoritative_capability_actions),
        )
        .route(
            "/v1/authoritative/capability-actions/request",
            post(authoritative_capability_request),
        )
        .route(
            "/v1/authoritative/capability-actions/claim",
            post(authoritative_capability_claim),
        )
        .route(
            "/v1/authoritative/capability-actions/result",
            post(authoritative_capability_result),
        )
        .route(
            "/v1/authoritative/capability-actions/show",
            get(authoritative_capability_show),
        )
        .route(
            "/v1/authoritative/capability-actions/list",
            get(authoritative_capability_list),
        )
        .route(
            "/v1/authoritative/capability-actions/reconcile",
            post(authoritative_capability_reconcile),
        )
        .route(
            "/v1/authoritative/capability-effects",
            get(authoritative_capability_effects),
        )
        .route(
            "/v1/authoritative/blueprints/commands",
            post(blueprints::accept_command),
        )
        .route(
            "/v1/authoritative/blueprints",
            get(blueprints::list).post(blueprints::publish),
        )
        .route("/v1/authoritative/blueprints/show", get(blueprints::show))
        .route(
            "/v1/authoritative/blueprints/bundled",
            get(blueprints::bundled),
        )
        .route(
            "/v1/authoritative/blueprints/install",
            post(blueprints::install),
        )
        .route("/v1/authoritative/blueprints/bind", post(blueprints::bind))
        .route(
            "/v1/authoritative/blueprints/binding",
            get(blueprints::binding),
        )
        .route(
            "/v1/authoritative/blueprints/items",
            get(blueprints::items).post(blueprints::create_item),
        )
        .route(
            "/v1/authoritative/blueprints/upgrade-preview",
            get(blueprints::upgrade_preview),
        )
        .route(
            "/v1/authoritative/blueprints/upgrade",
            post(blueprints::upgrade),
        )
        .route(
            "/v1/authoritative/blueprints/transition",
            post(blueprints::transition),
        )
        .route(
            "/v1/authoritative/blueprints/properties",
            post(blueprints::properties),
        )
        .route(
            "/v1/authoritative/blueprints/template",
            post(blueprints::template),
        )
        .route(
            "/v1/authoritative/agent-jobs/start",
            post(authoritative_agent_start),
        )
        .route(
            "/v1/authoritative/agent-jobs/reserve",
            post(authoritative_agent_job_reserve),
        )
        .route(
            "/v1/authoritative/agent-jobs/command",
            post(authoritative_agent_job_command),
        )
        .route("/v1/authoritative/agent-jobs", get(authoritative_agent_job))
        .route(
            "/v1/authoritative/agent-jobs/attention",
            get(authoritative_agent_attention),
        )
        .route(
            "/v1/authoritative/agent-jobs/handoffs",
            get(authoritative_agent_handoffs).post(authoritative_agent_submit_handoff),
        )
        .route(
            "/v1/authoritative/agent-jobs/review",
            get(authoritative_agent_review).post(authoritative_agent_submit_review),
        )
        .route(
            "/v1/authoritative/agent-jobs/overview",
            get(authoritative_agent_overview),
        )
        .route(
            "/v1/authoritative/agent-executors",
            get(authoritative_agent_executors),
        )
        .route(
            "/v1/authoritative/agent-jobs/resume-context",
            get(authoritative_agent_resume_context),
        )
        .route(
            "/v1/authoritative/agent-jobs/resume",
            post(authoritative_agent_resume),
        )
        .route("/mcp", post(remote_mcp::endpoint))
        .route(
            "/v1/agent-proposals",
            get(list_proposals).post(propose_action),
        )
        .route("/v1/admin/agent-proposals", post(propose_admin_action))
        .route("/v1/agent-proposals/{id}/decision", post(decide_proposal))
        .route("/v1/agent-proposals/{id}/audit", get(proposal_audit))
        .route("/v1/agent-proposals/claim", post(claim_proposal))
        .route("/v1/agent-proposals/{id}/settle", post(settle_proposal))
        .route("/v1/automations", get(list_jobs).post(put_job))
        .route("/v1/automations/{id}", axum::routing::delete(disable_job))
        .route("/v1/automations/{id}/refresh", post(refresh_job))
        .route("/v1/automation-runs", get(list_runs))
        .route("/v1/automation-runs/claim", post(claim_run))
        .route("/v1/automation-runs/{id}/settle", post(settle_run))
        // Browser app routes come last: the catch-all never shadows an API
        // route, and it answers API-shaped paths with JSON 404s.
        .route("/", get(web::index))
        .route("/{*path}", get(web::fallback))
        .layer(middleware::from_fn_with_state(state.clone(), csrf_guard))
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{BTreeMap, BTreeSet};
    use std::io::{Read, Write};
    use std::net::TcpStream;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::Duration;

    const DEVICE_TOKEN: &str = "device-secret-0123456789abcdefghij";

    static SEQUENCE: AtomicU64 = AtomicU64::new(0);

    /// Stand-in for a built `web/dist`, removed when the test ends.
    struct TempDist {
        root: PathBuf,
    }
    impl TempDist {
        fn built() -> Self {
            let root = std::env::temp_dir().join(format!(
                "orbit-server-web-{}-{}",
                std::process::id(),
                SEQUENCE.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir_all(root.join("assets")).unwrap();
            std::fs::write(
                root.join("index.html"),
                "<!doctype html><title>Orbit</title>",
            )
            .unwrap();
            std::fs::write(root.join("assets/app.js"), "console.log(1)").unwrap();
            Self { root }
        }
        fn absent() -> Self {
            Self {
                root: std::env::temp_dir().join(format!(
                    "orbit-server-web-absent-{}-{}",
                    std::process::id(),
                    SEQUENCE.fetch_add(1, Ordering::Relaxed)
                )),
            }
        }
        fn dist(&self) -> web::WebDist {
            web::WebDist::new(self.root.clone())
        }
    }
    impl Drop for TempDist {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    fn test_state(dist: &TempDist) -> AppState {
        let state = AppState {
            store: Arc::new(Mutex::new(AuthoritativeHost::open(":memory:").unwrap())),
            automation: Arc::new(Mutex::new(
                AutomationStore::open(Path::new(":memory:")).unwrap(),
            )),
            proposals: Arc::new(Mutex::new(
                ProposalStore::open(Path::new(":memory:")).unwrap(),
            )),
            sessions: Arc::new(Mutex::new(
                SessionStore::open_with_ttl(Path::new(":memory:"), 60_000, 3_600_000).unwrap(),
            )),
            login_attempts: Arc::new(Mutex::new(LoginThrottle::new())),
            web_dist: Arc::new(dist.dist()),
            cookie_secure: false,
            admin_key_hash: Sha256::digest(b"test-admin-key-0123456789abcdef").to_vec(),
            attachment_body_limit: 1024 * 1024,
        };
        state
            .store
            .lock()
            .unwrap()
            .enroll_device("device-1", DEVICE_TOKEN)
            .unwrap();
        state
    }

    async fn spawn(app: Router) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        address.to_string()
    }

    /// Minimal HTTP/1.1 client: the crate has no test-only HTTP dependencies.
    fn send(address: &str, request: &str) -> (u16, String) {
        let mut stream = TcpStream::connect(address).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        stream.write_all(request.as_bytes()).unwrap();
        let mut raw = Vec::new();
        stream.read_to_end(&mut raw).unwrap();
        let raw = String::from_utf8_lossy(&raw).into_owned();
        let status = raw
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .and_then(|code| code.parse().ok())
            .unwrap_or(0);
        (status, raw)
    }
    fn header_value(raw: &str, name: &str) -> Option<String> {
        raw.split("\r\n\r\n")
            .next()
            .unwrap_or_default()
            .lines()
            .filter_map(|line| line.split_once(':'))
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.trim().to_owned())
    }
    fn body_of(raw: &str) -> &str {
        raw.split_once("\r\n\r\n")
            .map(|(_, body)| body)
            .unwrap_or("")
    }
    fn session_token(raw: &str) -> String {
        header_value(raw, "set-cookie")
            .and_then(|cookie| {
                cookie
                    .strip_prefix("orbit_session=")
                    .and_then(|rest| rest.split(';').next().map(str::to_owned))
            })
            .expect("login must set an orbit_session cookie")
    }
    fn cookie_max_age(cookie: &str) -> Option<i64> {
        cookie
            .split(';')
            .map(str::trim)
            .find_map(|attribute| attribute.strip_prefix("Max-Age="))
            .and_then(|value| value.parse().ok())
    }

    fn post(address: &str, path: &str, headers: &str, body: &str) -> (u16, String) {
        send(
            address,
            &format!(
                "POST {path} HTTP/1.1\r\nHost: orbit.test\r\nConnection: close\r\n\
                 Content-Type: application/json\r\nContent-Length: {}\r\n{headers}\r\n{body}",
                body.len()
            ),
        )
    }
    fn get(address: &str, path: &str, headers: &str) -> (u16, String) {
        send(
            address,
            &format!(
                "GET {path} HTTP/1.1\r\nHost: orbit.test\r\nConnection: close\r\n{headers}\r\n"
            ),
        )
    }

    const CAPABILITY_MANIFEST: &str =
        include_str!("../../../blueprints/examples/capability-reference.json");
    const REFERENCE_CAPABILITY: &str = "orbit.reference.work-item-note";
    const EXECUTOR_TOKEN: &str = "executor-secret-0123456789abcdefghij";
    const SECOND_EXECUTOR_TOKEN: &str = "executor-two-secret-0123456789abcdefghij";

    fn capability_bearer(device_id: &str, token: &str) -> String {
        format!("Authorization: Bearer {token}\r\nx-orbit-device-id: {device_id}\r\n")
    }
    fn json_of(raw: &str) -> serde_json::Value {
        serde_json::from_str(body_of(raw))
            .unwrap_or_else(|error| panic!("invalid JSON body {error}: {}", body_of(raw)))
    }
    fn capability_status_of<'a>(
        document: &'a serde_json::Value,
        capability: &str,
    ) -> &'a serde_json::Value {
        document["capabilities"]
            .as_array()
            .expect("capabilities array")
            .iter()
            .find(|row| row["capability_id"] == capability)
            .unwrap_or_else(|| panic!("capability row for {capability}: {document}"))
    }

    /// A workspace with one installed capability, a bound capability-bearing
    /// workflow and two enrolled capabilities devices, created entirely over HTTP
    /// except for the device enrollment the admin surface owns.
    struct CapabilityFixture {
        address: String,
        state: AppState,
        operator: String,
        executor: String,
        _dist: TempDist,
    }

    impl CapabilityFixture {
        async fn new() -> Self {
            let dist = TempDist::built();
            let state = test_state(&dist);
            {
                let mut host = state.store.lock().unwrap();
                host.enroll_device("executor-1", EXECUTOR_TOKEN).unwrap();
                host.enroll_device("executor-2", SECOND_EXECUTOR_TOKEN)
                    .unwrap();
            }
            let address = spawn(routes(state.clone())).await;
            let operator = capability_bearer("device-1", DEVICE_TOKEN);
            // C25: an uninstalled capability is still refused, with the message
            // this transport already used for a manifest validation refusal.
            let (status, raw) = post(
                &address,
                "/v1/authoritative/blueprints",
                &operator,
                &format!(r#"{{"definition":{CAPABILITY_MANIFEST}}}"#),
            );
            assert_eq!(status, 400, "{}", body_of(&raw));
            assert!(
                body_of(&raw).contains("blueprint operation rejected"),
                "{}",
                body_of(&raw)
            );
            let (status, raw) = post(
                &address,
                "/v1/authoritative/capabilities/install",
                &operator,
                &format!(
                    r#"{{"command_id":"install-1","capability_id":"{REFERENCE_CAPABILITY}"}}"#
                ),
            );
            assert_eq!(status, 200, "{}", body_of(&raw));
            // C25: the durable installed set now makes the same manifest publishable.
            let (status, raw) = post(
                &address,
                "/v1/authoritative/blueprints",
                &operator,
                &format!(r#"{{"definition":{CAPABILITY_MANIFEST}}}"#),
            );
            assert_eq!(status, 200, "{}", body_of(&raw));
            // The project is created already bound to this workflow: the first
            // binding wins, so a create is the one chance to pin it.
            let (status, raw) = post(
                &address,
                "/v1/authoritative/project-commands",
                &operator,
                r#"{"command_id":"project-create-1","expected_revision":0,"project_id":"project-1","name":"Reference","workflow_id":"capability-reference"}"#,
            );
            assert_eq!(status, 200, "{}", body_of(&raw));
            let (status, raw) = get(
                &address,
                "/v1/authoritative/blueprints/binding?project_id=project-1",
                &operator,
            );
            assert_eq!(status, 200, "{}", body_of(&raw));
            assert_eq!(json_of(&raw)["blueprint_id"], "capability-reference");
            for item in ["item-1", "item-2"] {
                let (status, raw) = post(
                    &address,
                    "/v1/authoritative/blueprints/items",
                    &operator,
                    &format!(r#"{{"id":"{item}","project_id":"project-1","kind":"note-task"}}"#),
                );
                assert_eq!(status, 200, "{}", body_of(&raw));
            }
            let executor = capability_bearer("executor-1", EXECUTOR_TOKEN);
            Self {
                address,
                state,
                operator,
                executor,
                _dist: dist,
            }
        }

        fn announce(&self, headers: &str) -> (u16, String) {
            post(
                &self.address,
                "/v1/authoritative/capability-executors/announce",
                headers,
                &format!(r#"{{"capability_ids":["{REFERENCE_CAPABILITY}"]}}"#),
            )
        }

        fn authorize(&self) -> (u16, String) {
            post(
                &self.address,
                "/v1/authoritative/capabilities/authorize",
                &self.operator,
                &format!(
                    r#"{{"command_id":"authorize-1","capability_id":"{REFERENCE_CAPABILITY}","project_id":"project-1","confirm":true}}"#
                ),
            )
        }

        fn claim(&self, headers: &str, command_id: &str, token: &str) -> (u16, String) {
            post(
                &self.address,
                "/v1/authoritative/capability-actions/claim",
                headers,
                &format!(r#"{{"command_id":"{command_id}","claim_token":"{token}"}}"#),
            )
        }

        fn result(
            &self,
            headers: &str,
            command_id: &str,
            token: &str,
            outcome: &str,
        ) -> (u16, String) {
            self.result_with_effect(
                headers,
                command_id,
                token,
                outcome,
                &serde_json::json!({
                    "kind": "work_item_note",
                    "item_id": "item-1",
                    "note": "hello",
                    "tag": "demo",
                }),
            )
        }

        /// The same report, with whatever effect document the caller wants (or
        /// none at all), so every refusal can be exercised over real HTTP.
        fn result_with_effect(
            &self,
            headers: &str,
            command_id: &str,
            token: &str,
            outcome: &str,
            effect: &serde_json::Value,
        ) -> (u16, String) {
            post(
                &self.address,
                "/v1/authoritative/capability-actions/result",
                headers,
                &serde_json::json!({
                    "command_id": command_id,
                    "claim_token": token,
                    "outcome": outcome,
                    "detail": "ran it",
                    "effect": effect,
                })
                .to_string(),
            )
        }

        /// A report that declares no effect at all, which is always allowed.
        fn result_without_effect(
            &self,
            headers: &str,
            command_id: &str,
            token: &str,
            outcome: &str,
        ) -> (u16, String) {
            post(
                &self.address,
                "/v1/authoritative/capability-actions/result",
                headers,
                &format!(
                    r#"{{"command_id":"{command_id}","claim_token":"{token}","outcome":"{outcome}","detail":"ran it"}}"#
                ),
            )
        }

        fn effects(&self, item_id: &str, headers: &str) -> (u16, String) {
            get(
                &self.address,
                &format!("/v1/authoritative/capability-effects?item_id={item_id}"),
                headers,
            )
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn capability_flow_is_fenced_durable_and_reported_by_the_executor() {
        let fixture = CapabilityFixture::new().await;
        let address = &fixture.address;
        let (status, raw) = fixture.announce(&fixture.executor);
        assert_eq!(status, 200, "{}", body_of(&raw));
        let announced = json_of(&raw);
        assert_eq!(announced["device_id"], "executor-1");
        assert_eq!(
            announced["capabilities"][0]["capability_id"],
            REFERENCE_CAPABILITY
        );

        let (status, raw) = get(
            address,
            "/v1/authoritative/capability-executors",
            &fixture.operator,
        );
        assert_eq!(status, 200, "{}", body_of(&raw));
        assert_eq!(json_of(&raw)["executors"][0]["device_id"], "executor-1");

        let (status, raw) = get(
            address,
            "/v1/authoritative/capabilities?project_id=project-1",
            &fixture.operator,
        );
        assert_eq!(status, 200, "{}", body_of(&raw));
        let status_document = json_of(&raw);
        let reference = capability_status_of(&status_document, REFERENCE_CAPABILITY);
        assert_eq!(reference["installed"], true);
        assert_eq!(reference["granted"], false, "{status_document}");
        assert_eq!(reference["executors"][0]["device_id"], "executor-1");
        assert_eq!(reference["actions"][0]["id"], "append_note");

        // Unauthorized: the offer is listed with its code and human reason.
        let (status, raw) = get(
            address,
            "/v1/authoritative/capability-actions?item_id=item-1",
            &fixture.operator,
        );
        assert_eq!(status, 200, "{}", body_of(&raw));
        let offers = json_of(&raw);
        assert_eq!(offers["item_revision"], 0);
        assert_eq!(offers["offers"][0]["action_id"], "append_note");
        assert_eq!(offers["offers"][0]["capability_id"], REFERENCE_CAPABILITY);
        assert_eq!(offers["offers"][0]["from"], "open");
        assert_eq!(offers["offers"][0]["available"], false);
        assert_eq!(offers["offers"][0]["code"], "not_authorized");
        assert!(
            offers["offers"][0]["reason"]
                .as_str()
                .expect("reason")
                .contains("is not authorized for project project-1"),
            "{}",
            body_of(&raw)
        );
        let unauthorized = r#"{"command_id":"cap-unauthorized","item_id":"item-1","expected_revision":0,"action_id":"append_note","inputs":{"note":"hello"}}"#;
        let (status, raw) = post(
            address,
            "/v1/authoritative/capability-actions/request",
            &fixture.operator,
            unauthorized,
        );
        assert_eq!(status, 400, "{}", body_of(&raw));
        assert!(
            body_of(&raw).contains("capability action unavailable: not_authorized:"),
            "{}",
            body_of(&raw)
        );

        let (status, raw) = fixture.authorize();
        assert_eq!(status, 200, "{}", body_of(&raw));
        let grant = json_of(&raw);
        assert_eq!(grant["project_id"], "project-1");
        assert_eq!(grant["granted_by"], "device-1");

        let (status, raw) = get(
            address,
            "/v1/authoritative/capability-actions?item_id=item-1",
            &fixture.operator,
        );
        assert_eq!(status, 200, "{}", body_of(&raw));
        let offers = json_of(&raw);
        assert_eq!(offers["offers"][0]["available"], true, "{}", body_of(&raw));
        assert_eq!(offers["offers"][0]["code"], serde_json::Value::Null);

        // A request persists an authorized, fenced intent.
        let request_body = r#"{"command_id":"cap-1","item_id":"item-1","expected_revision":0,"action_id":"append_note","inputs":{"note":"hello"}}"#;
        let (status, raw) = post(
            address,
            "/v1/authoritative/capability-actions/request",
            &fixture.operator,
            request_body,
        );
        assert_eq!(status, 200, "{}", body_of(&raw));
        let request = json_of(&raw)["request"].clone();
        assert_eq!(request["command_id"], "cap-1");
        assert_eq!(request["state"], "pending");
        assert_eq!(request["from"], "open");
        assert_eq!(request["to"], "closed");
        assert_eq!(request["capability_id"], REFERENCE_CAPABILITY);
        assert_eq!(request["inputs"]["note"], "hello");
        assert_eq!(request["executor_device_id"], serde_json::Value::Null);
        assert_eq!(request["claim_token"], serde_json::Value::Null);
        // Replaying the identical request is idempotent, not a second intent.
        let (status, raw) = post(
            address,
            "/v1/authoritative/capability-actions/request",
            &fixture.operator,
            request_body,
        );
        assert_eq!(status, 200, "{}", body_of(&raw));
        assert_eq!(json_of(&raw)["request"], request);

        // A browser session is not an executor, even with a valid cookie.
        let login = format!(r#"{{"device_id":"device-1","token":"{DEVICE_TOKEN}"}}"#);
        let (status, raw) = post(address, "/v1/web/session", "", &login);
        assert_eq!(status, 201);
        let cookie = format!(
            "Cookie: orbit_session={}\r\nx-orbit-csrf: 1\r\n",
            session_token(&raw)
        );
        let (status, raw) = fixture.claim(&cookie, "cap-1", "claim-1");
        assert_eq!(status, 403, "{}", body_of(&raw));
        assert!(
            body_of(&raw).contains("executor credential required"),
            "{}",
            body_of(&raw)
        );
        // An authenticated device that never announced cannot claim either.
        let (status, raw) = fixture.claim(&fixture.operator, "cap-1", "claim-1");
        assert_eq!(status, 403, "{}", body_of(&raw));
        assert!(
            body_of(&raw).contains("capability not announced"),
            "{}",
            body_of(&raw)
        );

        // The enrolled executor claims and replays the same claim.
        let (status, raw) = fixture.claim(&fixture.executor, "cap-1", "claim-1");
        assert_eq!(status, 200, "{}", body_of(&raw));
        let claim = json_of(&raw);
        assert_eq!(claim["request"]["state"], "claimed");
        assert_eq!(claim["request"]["executor_device_id"], "executor-1");
        assert_eq!(claim["request"]["claim_token"], "claim-1");
        assert_eq!(claim["execution_plan"]["capability"], REFERENCE_CAPABILITY);
        assert_eq!(claim["execution_plan"]["action_id"], "append_note");
        assert_eq!(claim["execution_plan"]["parameters"]["note"], "hello");
        assert_eq!(claim["execution_plan"]["command_id"], "cap-1");
        let (status, raw) = fixture.claim(&fixture.executor, "cap-1", "claim-1");
        assert_eq!(status, 200, "{}", body_of(&raw));
        assert_eq!(json_of(&raw), claim);

        // A second enrolled executor cannot take the same intent.
        let second = capability_bearer("executor-2", SECOND_EXECUTOR_TOKEN);
        let (status, raw) = fixture.announce(&second);
        assert_eq!(status, 200, "{}", body_of(&raw));
        let (status, raw) = fixture.claim(&second, "cap-1", "claim-2");
        assert_eq!(status, 409, "{}", body_of(&raw));
        assert!(
            body_of(&raw).contains("capability action already claimed"),
            "{}",
            body_of(&raw)
        );

        // The result is recorded from the executor's report, in one savepoint.
        let (status, raw) = fixture.result(&second, "cap-1", "claim-2", "succeeded");
        assert_eq!(status, 409, "{}", body_of(&raw));
        assert!(
            body_of(&raw).contains("capability action already claimed"),
            "{}",
            body_of(&raw)
        );
        let (status, raw) = fixture.result(&fixture.executor, "cap-1", "claim-1", "succeeded");
        assert_eq!(status, 200, "{}", body_of(&raw));
        let settled = json_of(&raw);
        assert_eq!(settled["request"]["state"], "succeeded");
        assert_eq!(settled["receipt"]["outcome"], "succeeded");
        assert_eq!(settled["receipt"]["reported_by"], "executor-1");
        assert_eq!(settled["receipt"]["detail"]["detail"], "ran it");
        // The authority persisted the declared effect in the same settlement, and
        // the receipt says so with the kind and the persisted identity.
        assert_eq!(
            settled["receipt"]["detail"]["effect"],
            serde_json::json!({
                "applied": true,
                "kind": "work_item_note",
                "item_id": "item-1",
                "command_id": "cap-1",
            })
        );
        // …and it is readable as a durable result, without reading the database.
        let (status, raw) = fixture.effects("item-1", &fixture.operator);
        assert_eq!(status, 200, "{}", body_of(&raw));
        let effects = json_of(&raw);
        assert_eq!(effects["item_id"], "item-1");
        assert_eq!(effects["notes"].as_array().expect("notes").len(), 1);
        assert_eq!(effects["notes"][0]["command_id"], "cap-1");
        assert_eq!(effects["notes"][0]["note"], "hello");
        assert_eq!(effects["notes"][0]["tag"], "demo");
        assert!(
            effects["notes"][0]["created_at_ms"]
                .as_i64()
                .is_some_and(|at| at > 0),
            "{effects}"
        );
        assert!(
            json_of(&fixture.effects("item-2", &fixture.operator).1)["notes"]
                .as_array()
                .expect("notes")
                .is_empty(),
            "another item is untouched"
        );
        // Replaying the same outcome returns the original receipt.
        let (status, raw) = fixture.result(&fixture.executor, "cap-1", "claim-1", "succeeded");
        assert_eq!(status, 200, "{}", body_of(&raw));
        assert_eq!(json_of(&raw)["receipt"], settled["receipt"]);
        // A different outcome on a settled intent is refused.
        let (status, raw) = fixture.result(&fixture.executor, "cap-1", "claim-1", "failed");
        assert_eq!(status, 409, "{}", body_of(&raw));
        assert!(
            body_of(&raw).contains("capability action already settled"),
            "{}",
            body_of(&raw)
        );

        let (status, raw) = get(
            address,
            "/v1/authoritative/capability-actions/show?command_id=cap-1",
            &fixture.operator,
        );
        assert_eq!(status, 200, "{}", body_of(&raw));
        let shown = json_of(&raw);
        assert_eq!(shown["request"]["state"], "succeeded");
        assert_eq!(shown["receipt"]["reported_by"], "executor-1");
        let (status, raw) = get(
            address,
            "/v1/authoritative/capability-actions/list?item_id=item-1",
            &fixture.operator,
        );
        assert_eq!(status, 200, "{}", body_of(&raw));
        assert_eq!(json_of(&raw)["requests"][0]["command_id"], "cap-1");

        // A pending intent cannot be reconciled: it never executed.
        let second_request = r#"{"command_id":"cap-2","item_id":"item-2","expected_revision":0,"action_id":"append_note","inputs":{"note":"second"}}"#;
        let (status, raw) = post(
            address,
            "/v1/authoritative/capability-actions/request",
            &fixture.operator,
            second_request,
        );
        assert_eq!(status, 200, "{}", body_of(&raw));
        let (status, raw) = post(
            address,
            "/v1/authoritative/capability-actions/reconcile",
            &fixture.operator,
            r#"{"command_id":"cap-2","decision":"confirm_failed","detail":"nothing ran"}"#,
        );
        assert_eq!(status, 409, "{}", body_of(&raw));
        assert!(
            body_of(&raw).contains("capability action cannot be reconciled"),
            "{}",
            body_of(&raw)
        );
        // A claimed intent is reconcilable, and the actor is recorded.
        let (status, raw) = fixture.claim(&fixture.executor, "cap-2", "claim-3");
        assert_eq!(status, 200, "{}", body_of(&raw));
        let (status, raw) = post(
            address,
            "/v1/authoritative/capability-actions/reconcile",
            &fixture.operator,
            r#"{"command_id":"cap-2","decision":"confirm_failed","detail":"nothing ran"}"#,
        );
        assert_eq!(status, 200, "{}", body_of(&raw));
        let reconciled = json_of(&raw);
        assert_eq!(reconciled["request"]["state"], "failed");
        assert_eq!(reconciled["receipt"]["reported_by"], "device-1");
        assert_eq!(reconciled["receipt"]["detail"]["note"], "nothing ran");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn capability_effects_are_validated_persisted_once_and_read_back() {
        let fixture = CapabilityFixture::new().await;
        let address = &fixture.address;
        let (status, raw) = fixture.announce(&fixture.executor);
        assert_eq!(status, 200, "{}", body_of(&raw));
        let (status, raw) = fixture.authorize();
        assert_eq!(status, 200, "{}", body_of(&raw));
        let (status, raw) = post(
            address,
            "/v1/authoritative/capability-actions/request",
            &fixture.operator,
            r#"{"command_id":"cap-e","item_id":"item-1","expected_revision":0,"action_id":"append_note","inputs":{"note":"hello"}}"#,
        );
        assert_eq!(status, 200, "{}", body_of(&raw));
        let (status, raw) = fixture.claim(&fixture.executor, "cap-e", "claim-e");
        assert_eq!(status, 200, "{}", body_of(&raw));

        // Every schema violation is a 400 with its own fixed text, and none of
        // them settles the intent or writes anything.
        let violations = [
            (
                serde_json::json!({"kind": "work_item_note", "item_id": "item-1"}),
                "invalid capability effect",
            ),
            (
                serde_json::json!({"kind": "work_item_note", "item_id": "item-1", "note": 7}),
                "invalid capability effect",
            ),
            // The target must be this request's own item.
            (
                serde_json::json!({"kind": "work_item_note", "item_id": "item-2", "note": "x"}),
                "invalid capability effect",
            ),
            // Neither the kind nor an invented key may be supplied by a body.
            (
                serde_json::json!({"kind": "file_write", "item_id": "item-1", "note": "x"}),
                "invalid capability effect",
            ),
            (
                serde_json::json!({"kind": "work_item_note", "item_id": "item-1", "note": "x", "table": "devices"}),
                "invalid capability effect",
            ),
            (
                serde_json::json!({"kind": "work_item_note", "item_id": "item-1", "note": "x".repeat(2_001)}),
                "capability effect exceeds its declared length",
            ),
            (
                serde_json::json!({"kind": "work_item_note", "item_id": "item-1", "note": "x", "tag": "t".repeat(65)}),
                "capability effect exceeds its declared length",
            ),
        ];
        for (payload, message) in violations {
            let (status, raw) = fixture.result_with_effect(
                &fixture.executor,
                "cap-e",
                "claim-e",
                "succeeded",
                &payload,
            );
            assert_eq!(status, 400, "{}: {}", payload, body_of(&raw));
            assert!(
                body_of(&raw).contains(message),
                "expected {message:?} for {payload}: {}",
                body_of(&raw)
            );
            assert!(
                !body_of(&raw).contains("capability storage error"),
                "a refusal is never an internal failure: {}",
                body_of(&raw)
            );
        }
        let (status, raw) = get(
            address,
            "/v1/authoritative/capability-actions/show?command_id=cap-e",
            &fixture.operator,
        );
        assert_eq!(status, 200, "{}", body_of(&raw));
        let shown = json_of(&raw);
        assert_eq!(shown["request"]["state"], "claimed", "{shown}");
        assert_eq!(shown["receipt"], serde_json::Value::Null, "{shown}");
        assert!(
            json_of(&fixture.effects("item-1", &fixture.operator).1)["notes"]
                .as_array()
                .expect("notes")
                .is_empty(),
            "a refused effect writes nothing"
        );

        // A valid effect settles the request and is persisted with it.
        let (status, raw) = fixture.result_with_effect(
            &fixture.executor,
            "cap-e",
            "claim-e",
            "succeeded",
            &serde_json::json!({
                "kind": "work_item_note",
                "item_id": "item-1",
                "note": "hello",
                "tag": "demo",
            }),
        );
        assert_eq!(status, 200, "{}", body_of(&raw));
        let settled = json_of(&raw);
        assert_eq!(settled["request"]["state"], "succeeded");
        assert_eq!(settled["receipt"]["detail"]["effect"]["applied"], true);
        // Replaying the identical report is idempotent, including the effect.
        let (status, raw) = fixture.result_with_effect(
            &fixture.executor,
            "cap-e",
            "claim-e",
            "succeeded",
            &serde_json::json!({
                "kind": "work_item_note",
                "item_id": "item-1",
                "note": "hello",
                "tag": "demo",
            }),
        );
        assert_eq!(status, 200, "{}", body_of(&raw));
        assert_eq!(json_of(&raw)["receipt"], settled["receipt"]);
        // A different effect for a settled command id is refused, and the stored
        // note is untouched.
        let (status, raw) = fixture.result_with_effect(
            &fixture.executor,
            "cap-e",
            "claim-e",
            "succeeded",
            &serde_json::json!({
                "kind": "work_item_note",
                "item_id": "item-1",
                "note": "a second note for the same command",
            }),
        );
        assert_eq!(status, 409, "{}", body_of(&raw));
        assert!(
            body_of(&raw).contains("capability action already settled"),
            "{}",
            body_of(&raw)
        );
        let notes = json_of(&fixture.effects("item-1", &fixture.operator).1);
        assert_eq!(
            notes["notes"].as_array().expect("notes").len(),
            1,
            "{notes}"
        );
        assert_eq!(notes["notes"][0]["note"], "hello");
        assert_eq!(notes["notes"][0]["tag"], "demo");

        // An absent effect is allowed and recorded honestly.
        let (status, raw) = post(
            address,
            "/v1/authoritative/capability-actions/request",
            &fixture.operator,
            r#"{"command_id":"cap-f","item_id":"item-2","expected_revision":0,"action_id":"append_note","inputs":{"note":"second"}}"#,
        );
        assert_eq!(status, 200, "{}", body_of(&raw));
        let (status, raw) = fixture.claim(&fixture.executor, "cap-f", "claim-f");
        assert_eq!(status, 200, "{}", body_of(&raw));
        let (status, raw) =
            fixture.result_without_effect(&fixture.executor, "cap-f", "claim-f", "succeeded");
        assert_eq!(status, 200, "{}", body_of(&raw));
        assert_eq!(
            json_of(&raw)["receipt"]["detail"]["effect"],
            serde_json::json!({"applied": false, "reason": "no effect declared by the executor"})
        );
        assert!(
            json_of(&fixture.effects("item-2", &fixture.operator).1)["notes"]
                .as_array()
                .expect("notes")
                .is_empty(),
            "an absent effect never becomes a row"
        );

        // The read is authenticated like every other read, and an unknown item is
        // a 404 rather than an empty list.
        let (status, _) = fixture.effects("item-1", "");
        assert_eq!(status, 401);
        let login = format!(r#"{{"device_id":"device-1","token":"{DEVICE_TOKEN}"}}"#);
        let (status, raw) = post(address, "/v1/web/session", "", &login);
        assert_eq!(status, 201);
        let cookie = format!("Cookie: orbit_session={}\r\n", session_token(&raw));
        let (status, raw) = fixture.effects("item-1", &cookie);
        assert_eq!(status, 200, "{}", body_of(&raw));
        assert_eq!(json_of(&raw)["notes"].as_array().expect("notes").len(), 1);
        let (status, raw) = fixture.effects("missing", &fixture.operator);
        assert_eq!(status, 404, "{}", body_of(&raw));
        assert!(
            body_of(&raw).contains("work item not found"),
            "{}",
            body_of(&raw)
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn capability_refusals_keep_their_codes_and_credentials() {
        let fixture = CapabilityFixture::new().await;
        let address = &fixture.address;
        let executor = &fixture.executor;

        // Executor and operator routes refuse a cookie session explicitly.
        let login = format!(r#"{{"device_id":"device-1","token":"{DEVICE_TOKEN}"}}"#);
        let (status, raw) = post(address, "/v1/web/session", "", &login);
        assert_eq!(status, 201);
        let cookie = format!(
            "Cookie: orbit_session={}\r\nx-orbit-csrf: 1\r\n",
            session_token(&raw)
        );
        let (status, raw) = fixture.announce(&cookie);
        assert_eq!(status, 403, "{}", body_of(&raw));
        assert!(body_of(&raw).contains("executor credential required"));
        let (status, raw) = post(
            address,
            "/v1/authoritative/capabilities/install",
            &cookie,
            &format!(
                r#"{{"command_id":"install-cookie","capability_id":"{REFERENCE_CAPABILITY}"}}"#
            ),
        );
        assert_eq!(status, 403, "{}", body_of(&raw));
        assert!(body_of(&raw).contains("operator credential required"));

        // Unknown capabilities are named as such.
        let (status, raw) = post(
            address,
            "/v1/authoritative/capability-executors/announce",
            executor,
            r#"{"capability_ids":["orbit.not.a.capability"]}"#,
        );
        assert_eq!(status, 400, "{}", body_of(&raw));
        assert!(body_of(&raw).contains("unknown capability"));
        let (status, raw) = post(
            address,
            "/v1/authoritative/capabilities/install",
            &fixture.operator,
            r#"{"command_id":"install-unknown","capability_id":"orbit.not.a.capability"}"#,
        );
        assert_eq!(status, 400, "{}", body_of(&raw));
        assert!(body_of(&raw).contains("unknown capability"));

        // A body can never supply a provider, a target state or a status.
        let (status, raw) = post(
            address,
            "/v1/authoritative/capability-actions/request",
            &fixture.operator,
            r#"{"command_id":"cap-forged","item_id":"item-1","expected_revision":0,"action_id":"append_note","to":"closed","provider":"evil","inputs":{"note":"x"}}"#,
        );
        assert_eq!(status, 422, "{}", body_of(&raw));

        // No announced executor: the offer says exactly that.
        let (status, raw) = get(
            address,
            "/v1/authoritative/capability-actions?item_id=item-1",
            &fixture.operator,
        );
        assert_eq!(status, 200, "{}", body_of(&raw));
        let offers = json_of(&raw);
        assert_eq!(offers["offers"][0]["code"], "executor_missing");
        assert!(
            offers["offers"][0]["reason"]
                .as_str()
                .expect("reason")
                .contains("no active enrolled executor announced"),
            "{offers}"
        );

        let (status, raw) = fixture.announce(executor);
        assert_eq!(status, 200, "{}", body_of(&raw));
        let (status, raw) = fixture.authorize();
        assert_eq!(status, 200, "{}", body_of(&raw));

        // Invalid input: a declared key with an invalid value, and an invented key.
        let (status, raw) = post(
            address,
            "/v1/authoritative/capability-actions/request",
            &fixture.operator,
            r#"{"command_id":"cap-blank","item_id":"item-1","expected_revision":0,"action_id":"append_note","inputs":{"note":""}}"#,
        );
        assert_eq!(status, 400, "{}", body_of(&raw));
        assert!(
            body_of(&raw).contains("invalid capability action input: invalid value for note"),
            "{}",
            body_of(&raw)
        );
        let (status, raw) = post(
            address,
            "/v1/authoritative/capability-actions/request",
            &fixture.operator,
            r#"{"command_id":"cap-injected","item_id":"item-1","expected_revision":0,"action_id":"append_note","inputs":{"note":"x","shell":"rm -rf /"}}"#,
        );
        assert_eq!(status, 400, "{}", body_of(&raw));
        assert!(
            body_of(&raw).contains("invalid capability action input: undeclared input: shell"),
            "{}",
            body_of(&raw)
        );

        // A mixed-case action id is not case-folded onto the real action.
        let (status, raw) = post(
            address,
            "/v1/authoritative/capability-actions/request",
            &fixture.operator,
            r#"{"command_id":"cap-case","item_id":"item-1","expected_revision":0,"action_id":"Append_Note","inputs":{"note":"x"}}"#,
        );
        assert_eq!(status, 409, "{}", body_of(&raw));
        assert!(
            body_of(&raw).contains("capability request conflict"),
            "{}",
            body_of(&raw)
        );

        // Stale revision and duplicate intent both keep the request conflict.
        let (status, raw) = post(
            address,
            "/v1/authoritative/capability-actions/request",
            &fixture.operator,
            r#"{"command_id":"cap-stale","item_id":"item-1","expected_revision":7,"action_id":"append_note","inputs":{"note":"x"}}"#,
        );
        assert_eq!(status, 409, "{}", body_of(&raw));
        assert!(body_of(&raw).contains("capability request conflict"));
        let (status, raw) = post(
            address,
            "/v1/authoritative/capability-actions/request",
            &fixture.operator,
            r#"{"command_id":"cap-ok","item_id":"item-1","expected_revision":0,"action_id":"append_note","inputs":{"note":"x"}}"#,
        );
        assert_eq!(status, 200, "{}", body_of(&raw));
        let (status, raw) = post(
            address,
            "/v1/authoritative/capability-actions/request",
            &fixture.operator,
            r#"{"command_id":"cap-dup","item_id":"item-1","expected_revision":0,"action_id":"append_note","inputs":{"note":"x"}}"#,
        );
        assert_eq!(status, 409, "{}", body_of(&raw));
        assert!(body_of(&raw).contains("capability request conflict"));

        // An uninstalled capability is reported as such, even for a bound item.
        fixture
            .state
            .store
            .lock()
            .unwrap()
            .uninstall_trusted_capability(REFERENCE_CAPABILITY)
            .unwrap();
        let (status, raw) = get(
            address,
            "/v1/authoritative/capability-actions?item_id=item-1",
            &fixture.operator,
        );
        assert_eq!(status, 200, "{}", body_of(&raw));
        let offers = json_of(&raw);
        assert_eq!(offers["offers"][0]["available"], false);
        assert_eq!(offers["offers"][0]["code"], "capability_not_installed");
        assert!(
            offers["offers"][0]["reason"]
                .as_str()
                .expect("reason")
                .contains("is not installed"),
            "{offers}"
        );
        let (status, raw) = post(
            address,
            "/v1/authoritative/capability-actions/request",
            &fixture.operator,
            r#"{"command_id":"cap-gone","item_id":"item-1","expected_revision":0,"action_id":"append_note","inputs":{"note":"x"}}"#,
        );
        assert_eq!(status, 400, "{}", body_of(&raw));
        assert!(
            body_of(&raw).contains("capability action unavailable: capability_not_installed:"),
            "{}",
            body_of(&raw)
        );

        // Reads still need an authenticated device.
        let (status, _) = get(
            address,
            "/v1/authoritative/capability-actions?item_id=item-1",
            "",
        );
        assert_eq!(status, 401);
        // Claim/result need a bearer credential at all, not just a device header.
        let (status, _) = post(
            address,
            "/v1/authoritative/capability-actions/claim",
            "",
            r#"{"command_id":"cap-ok","claim_token":"t"}"#,
        );
        assert_eq!(status, 401);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn login_cookie_authentication_and_csrf_over_http() {
        let dist = TempDist::built();
        let address = spawn(routes(test_state(&dist))).await;
        let login = format!(r#"{{"device_id":"device-1","token":"{DEVICE_TOKEN}"}}"#);
        let (status, raw) = post(&address, "/v1/web/session", "", &login);
        assert_eq!(status, 201);
        assert_eq!(header_value(&raw, "cache-control").unwrap(), "no-store");
        let cookie = header_value(&raw, "set-cookie").unwrap();
        assert!(cookie.contains("HttpOnly"));
        assert!(cookie.contains("SameSite=Strict"));
        assert!(!cookie.contains("Secure"));
        assert!(cookie_max_age(&cookie).is_some_and(|seconds| seconds > 0));
        assert!(body_of(&raw).contains("\"device_id\":\"device-1\""));

        let token = session_token(&raw);
        let cookie_header = format!("Cookie: orbit_session={token}\r\n");
        let (status, _) = get(&address, "/v1/authoritative/projects", &cookie_header);
        assert_eq!(status, 200);
        // Time passes, then a read renews the window. The reissued Max-Age is
        // the new lifetime, not the original one minus the elapsed time.
        tokio::time::sleep(Duration::from_millis(1_100)).await;
        let (status, raw) = get(&address, "/v1/web/session", &cookie_header);
        assert_eq!(status, 200);
        let reissued = header_value(&raw, "set-cookie").expect("GET must reissue the cookie");
        assert!(reissued.starts_with(&format!("orbit_session={token}")));
        let renewed = cookie_max_age(&reissued).expect("reissued cookie carries Max-Age");
        assert!(renewed > 0);
        assert!(
            renewed >= cookie_max_age(&cookie).unwrap(),
            "a sliding session reports its renewed lifetime, not the decremented one"
        );

        // A cookie-authenticated mutation needs the CSRF header.
        let (status, raw) = post(
            &address,
            "/v1/authoritative/project-commands",
            &cookie_header,
            "{}",
        );
        assert_eq!(status, 403);
        assert!(body_of(&raw).contains("csrf check failed"));
        let csrf = format!("{cookie_header}x-orbit-csrf: 1\r\n");
        let (status, _) = post(&address, "/v1/authoritative/project-commands", &csrf, "{}");
        // 422 is the body extractor rejecting `{}`; the point is CSRF passed.
        assert_ne!(status, 403);

        // Cross-site browser mutations are refused even with the header.
        let cross_site =
            format!("{cookie_header}x-orbit-csrf: 1\r\nSec-Fetch-Site: cross-site\r\n");
        let (status, _) = post(
            &address,
            "/v1/authoritative/project-commands",
            &cross_site,
            "{}",
        );
        assert_eq!(status, 403);

        // Logout clears the cookie (and, like every cookie mutation, needs the
        // CSRF header).
        let (status, raw) = send(
            &address,
            &format!(
                "DELETE /v1/web/session HTTP/1.1\r\nHost: orbit.test\r\nConnection: close\r\n\
                 {cookie_header}x-orbit-csrf: 1\r\n\r\n"
            ),
        );
        assert_eq!(status, 204);
        assert!(
            header_value(&raw, "set-cookie")
                .unwrap()
                .contains("Max-Age=0")
        );
        let (status, raw) = get(&address, "/v1/web/session", &cookie_header);
        assert_eq!(status, 401);
        // A dead cookie is cleared, so the browser stops presenting it.
        assert!(
            header_value(&raw, "set-cookie")
                .unwrap()
                .contains("Max-Age=0")
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn bearer_clients_are_unaffected_by_cookie_csrf() {
        let dist = TempDist::built();
        let address = spawn(routes(test_state(&dist))).await;
        let bearer =
            format!("Authorization: Bearer {DEVICE_TOKEN}\r\nx-orbit-device-id: device-1\r\n");
        let (status, _) = post(
            &address,
            "/v1/authoritative/project-commands",
            &bearer,
            "{}",
        );
        assert_ne!(status, 403);
        // A stale cookie next to a bearer token still takes the bearer path.
        let mixed = format!("{bearer}Cookie: orbit_session=stale\r\n");
        let (status, _) = post(&address, "/v1/authoritative/project-commands", &mixed, "{}");
        assert_ne!(status, 403);
        let (status, _) = get(&address, "/v1/authoritative/projects", &bearer);
        assert_eq!(status, 200);
        // Workflow upgrades require explicit confirmation before the authority
        // is asked to re-pin anything.
        let (status, raw) = post(
            &address,
            "/v1/authoritative/blueprints/upgrade",
            &bearer,
            r#"{"project_id":"project-1","id":"b","version":1}"#,
        );
        assert_eq!(status, 400);
        assert!(body_of(&raw).contains("confirmation required"));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn validation_reasons_are_quoted_and_stale_revisions_stay_generic() {
        let dist = TempDist::built();
        let address = spawn(routes(test_state(&dist))).await;
        let bearer =
            format!("Authorization: Bearer {DEVICE_TOKEN}\r\nx-orbit-device-id: device-1\r\n");
        let created = post(
            &address,
            "/v1/authoritative/task-commands",
            &bearer,
            r#"{"command_id":"create-1","expected_revision":0,"action":{"type":"create","task_id":"task-1","title":"Ship it"}}"#,
        );
        assert_eq!(created.0, 200, "{}", body_of(&created.1));

        // The browser must be able to quote the actual refusal, not just learn
        // that "something" was invalid.
        let (status, raw) = post(
            &address,
            "/v1/authoritative/task-commands",
            &bearer,
            r#"{"command_id":"status-1","expected_revision":1,"action":{"type":"change_status","task_id":"task-1","status":"done"}}"#,
        );
        assert_eq!(status, 400, "{}", body_of(&raw));
        assert!(
            body_of(&raw).contains("invalid task status transition: inbox -> done"),
            "{}",
            body_of(&raw)
        );

        // Staleness keeps its pre-existing generic 409: retrying is the fix,
        // and the body must not grow server-side detail.
        let (status, raw) = post(
            &address,
            "/v1/authoritative/task-commands",
            &bearer,
            r#"{"command_id":"status-2","expected_revision":99,"action":{"type":"change_status","task_id":"task-1","status":"planned"}}"#,
        );
        assert_eq!(status, 409, "{}", body_of(&raw));
        assert_eq!(
            body_of(&raw),
            r#"{"error":"stale server revision"}"#,
            "{}",
            body_of(&raw)
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn task_transitions_read_returns_the_domain_edges_for_every_status() {
        let dist = TempDist::built();
        let address = spawn(routes(test_state(&dist))).await;
        let bearer =
            format!("Authorization: Bearer {DEVICE_TOKEN}\r\nx-orbit-device-id: device-1\r\n");
        for status in orbit_domain::TaskStatus::ALL {
            let (code, raw) = get(
                &address,
                &format!(
                    "/v1/authoritative/task-transitions?status={}",
                    status.as_str()
                ),
                &bearer,
            );
            assert_eq!(code, 200, "{}", body_of(&raw));
            let body: serde_json::Value = serde_json::from_str(body_of(&raw)).unwrap();
            assert_eq!(body["status"], status.as_str());
            let next: Vec<&str> = body["next"]
                .as_array()
                .expect("next is an array")
                .iter()
                .map(|value| value.as_str().expect("status is a string"))
                .collect();
            // The read is exactly the write path's rule, for every pair.
            for other in orbit_domain::TaskStatus::ALL {
                let allowed = other != status && status.can_transition_to(other);
                assert_eq!(
                    next.contains(&other.as_str()),
                    allowed,
                    "{} -> {} disagree with the domain",
                    status.as_str(),
                    other.as_str()
                );
            }
            assert!(!next.contains(&status.as_str()), "self is not an edge");
        }
        // Terminal states offer nothing.
        for status in [
            orbit_domain::TaskStatus::Done,
            orbit_domain::TaskStatus::Cancelled,
        ] {
            let (_, raw) = get(
                &address,
                &format!(
                    "/v1/authoritative/task-transitions?status={}",
                    status.as_str()
                ),
                &bearer,
            );
            let body: serde_json::Value = serde_json::from_str(body_of(&raw)).unwrap();
            assert_eq!(body["next"].as_array().map(Vec::len), Some(0));
        }
        let (code, _) = get(
            &address,
            "/v1/authoritative/task-transitions?status=not-a-status",
            &bearer,
        );
        assert_eq!(code, 400);
    }

    #[test]
    fn internal_failures_stay_opaque_while_validation_is_quoted() {
        use orbit_application::RepositoryError;
        use orbit_domain::DomainError;
        let (status, Json(body)) = map_error(AuthorityError::Sql(rusqlite::Error::InvalidQuery));
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(body.error, "storage error");

        let unavailable = application_error_status(&ApplicationError::Repository(
            RepositoryError::Other("SELECT token FROM devices at /var/lib/orbit.db".into()),
        ));
        assert_eq!(unavailable.0, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(unavailable.1, "storage error");
        assert!(!unavailable.1.contains("SELECT"));
        assert!(!unavailable.1.contains("orbit.db"));

        let validation = application_error_status(&ApplicationError::Domain(
            DomainError::InvalidTaskStatusTransition {
                from: "inbox",
                to: "done",
            },
        ));
        assert_eq!(validation.0, StatusCode::BAD_REQUEST);
        assert_eq!(
            validation.1,
            "invalid task status transition: inbox -> done"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn project_statuses_read_returns_the_domain_edges_for_every_status() {
        let dist = TempDist::built();
        let address = spawn(routes(test_state(&dist))).await;
        let bearer =
            format!("Authorization: Bearer {DEVICE_TOKEN}\r\nx-orbit-device-id: device-1\r\n");
        let (code, raw) = get(&address, "/v1/authoritative/project-statuses", &bearer);
        assert_eq!(code, 200, "{}", body_of(&raw));
        let body: serde_json::Value = serde_json::from_str(body_of(&raw)).unwrap();
        let statuses = body["statuses"].as_array().expect("statuses is an array");
        assert_eq!(statuses.len(), orbit_domain::ProjectStatus::ALL.len());
        for (entry, status) in statuses.iter().zip(orbit_domain::ProjectStatus::ALL) {
            assert_eq!(entry["status"], status.as_str());
            let next: Vec<&str> = entry["next"]
                .as_array()
                .expect("next is an array")
                .iter()
                .map(|value| value.as_str().expect("status is a string"))
                .collect();
            assert!(!next.contains(&status.as_str()), "self is not an edge");
            for other in orbit_domain::ProjectStatus::ALL {
                assert_eq!(
                    next.contains(&other.as_str()),
                    other != status && status.can_transition_to(other),
                    "{} -> {} disagree with the domain",
                    status.as_str(),
                    other.as_str()
                );
            }
        }
        // A terminal state offers nothing at all.
        let archived = statuses
            .iter()
            .find(|entry| entry["status"] == "archived")
            .expect("archived is listed");
        assert_eq!(archived["next"].as_array().map(Vec::len), Some(0));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn project_updates_rename_move_status_and_quote_domain_refusals() {
        let dist = TempDist::built();
        let address = spawn(routes(test_state(&dist))).await;
        let bearer =
            format!("Authorization: Bearer {DEVICE_TOKEN}\r\nx-orbit-device-id: device-1\r\n");
        // The historical create payload still works untouched.
        let (status, raw) = post(
            &address,
            "/v1/authoritative/project-commands",
            &bearer,
            r#"{"command_id":"p-create","expected_revision":0,"project_id":"orbit","name":"Orbit"}"#,
        );
        assert_eq!(status, 200, "{}", body_of(&raw));
        let created: serde_json::Value = serde_json::from_str(body_of(&raw)).unwrap();
        assert_eq!(created["revision"], 1);
        assert_eq!(created["duplicate"], false);
        assert_eq!(created["project"]["status"], "proposed");
        assert_eq!(created["project"]["name"], "Orbit");

        let (status, raw) = post(
            &address,
            "/v1/authoritative/project-commands",
            &bearer,
            r#"{"command_id":"p-rename","expected_revision":1,"action":"update","project_id":"orbit","name":"Orbit Web"}"#,
        );
        assert_eq!(status, 200, "{}", body_of(&raw));
        let renamed: serde_json::Value = serde_json::from_str(body_of(&raw)).unwrap();
        assert_eq!(renamed["revision"], 2);
        assert_eq!(renamed["project"]["name"], "Orbit Web");
        assert_eq!(renamed["project"]["status"], "proposed");

        let (status, raw) = post(
            &address,
            "/v1/authoritative/project-commands",
            &bearer,
            r#"{"command_id":"p-status","expected_revision":2,"action":"update","project_id":"orbit","status":"active"}"#,
        );
        assert_eq!(status, 200, "{}", body_of(&raw));
        let moved: serde_json::Value = serde_json::from_str(body_of(&raw)).unwrap();
        assert_eq!(moved["revision"], 3);
        assert_eq!(moved["project"]["name"], "Orbit Web");
        assert_eq!(moved["project"]["status"], "active");

        // A domain refusal quotes the actual from/to states.
        let (status, raw) = post(
            &address,
            "/v1/authoritative/project-commands",
            &bearer,
            r#"{"command_id":"p-illegal","expected_revision":3,"action":"update","project_id":"orbit","status":"proposed"}"#,
        );
        assert_eq!(status, 400, "{}", body_of(&raw));
        assert!(
            body_of(&raw).contains("invalid project status transition: active -> proposed"),
            "{}",
            body_of(&raw)
        );

        // A stale fence keeps the pre-existing generic 409.
        let (status, raw) = post(
            &address,
            "/v1/authoritative/project-commands",
            &bearer,
            r#"{"command_id":"p-stale","expected_revision":1,"action":"update","project_id":"orbit","name":"Stale"}"#,
        );
        assert_eq!(status, 409, "{}", body_of(&raw));
        assert_eq!(body_of(&raw), r#"{"error":"stale server revision"}"#);

        // An update that changes nothing reports the current record at the tip.
        let (status, raw) = post(
            &address,
            "/v1/authoritative/project-commands",
            &bearer,
            r#"{"command_id":"p-noop","expected_revision":3,"action":"update","project_id":"orbit","name":"Orbit Web","status":"active"}"#,
        );
        assert_eq!(status, 200, "{}", body_of(&raw));
        let noop: serde_json::Value = serde_json::from_str(body_of(&raw)).unwrap();
        assert_eq!(noop["revision"], 3);
        assert_eq!(noop["duplicate"], false);

        let (_, raw) = get(&address, "/v1/authoritative/projects", &bearer);
        let projects: serde_json::Value = serde_json::from_str(body_of(&raw)).unwrap();
        assert_eq!(projects["revision"], 3, "a no-op must not advance history");
        assert_eq!(projects["projects"][0]["name"], "Orbit Web");
        assert_eq!(projects["projects"][0]["status"], "active");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn search_reads_three_groups_and_refuses_an_empty_query() {
        let dist = TempDist::built();
        let address = spawn(routes(test_state(&dist))).await;
        let bearer =
            format!("Authorization: Bearer {DEVICE_TOKEN}\r\nx-orbit-device-id: device-1\r\n");
        let (status, raw) = post(
            &address,
            "/v1/authoritative/project-commands",
            &bearer,
            r#"{"command_id":"p-create","expected_revision":0,"project_id":"orbit","name":"Orbit"}"#,
        );
        assert_eq!(status, 200, "{}", body_of(&raw));
        let (status, raw) = post(
            &address,
            "/v1/authoritative/task-commands",
            &bearer,
            r#"{"command_id":"t-create","expected_revision":1,"action":{"type":"create","task_id":"task-1","title":"Fix the Orbit login","project_id":"orbit"}}"#,
        );
        assert_eq!(status, 200, "{}", body_of(&raw));

        let (status, raw) = get(&address, "/v1/authoritative/search?q=orbit", &bearer);
        assert_eq!(status, 200, "{}", body_of(&raw));
        let body: serde_json::Value = serde_json::from_str(body_of(&raw)).unwrap();
        assert_eq!(body["query"], "orbit");
        assert_eq!(body["limit"], 10);
        assert_eq!(body["revision"], 2);
        let groups = body["groups"].as_array().expect("groups is an array");
        assert_eq!(
            groups
                .iter()
                .map(|group| group["kind"].as_str().unwrap_or_default())
                .collect::<Vec<_>>(),
            vec!["project", "task", "note"]
        );
        assert_eq!(groups[0]["truncated"], false);
        assert_eq!(groups[0]["results"][0]["kind"], "project");
        assert_eq!(groups[0]["results"][0]["title"], "Orbit");
        assert_eq!(groups[0]["results"][0]["subtitle"], "proposed");
        assert_eq!(groups[0]["results"][0]["project_id"], "orbit");
        assert_eq!(groups[0]["results"][0]["path"], serde_json::Value::Null);
        assert_eq!(groups[1]["results"][0]["kind"], "task");
        assert_eq!(groups[1]["results"][0]["title"], "Fix the Orbit login");
        assert_eq!(groups[1]["results"][0]["subtitle"], "inbox · Orbit");
        assert_eq!(groups[2]["results"], serde_json::json!([]));

        // The project filter narrows both entity groups.
        let (status, raw) = get(
            &address,
            "/v1/authoritative/search?q=orbit&project_id=missing",
            &bearer,
        );
        assert_eq!(status, 200, "{}", body_of(&raw));
        let filtered: serde_json::Value = serde_json::from_str(body_of(&raw)).unwrap();
        assert_eq!(filtered["groups"][0]["results"], serde_json::json!([]));
        assert_eq!(filtered["groups"][1]["results"], serde_json::json!([]));

        // The page size is clamped to the contract window.
        let (_, raw) = get(
            &address,
            "/v1/authoritative/search?q=orbit&limit=999",
            &bearer,
        );
        let clamped: serde_json::Value = serde_json::from_str(body_of(&raw)).unwrap();
        assert_eq!(clamped["limit"], 20);

        for (query, expected) in [
            ("", "search query is required"),
            ("%20%20", "search query is required"),
            (&"a".repeat(201), "search query is too long"),
        ] {
            let (status, raw) = get(
                &address,
                &format!("/v1/authoritative/search?q={query}"),
                &bearer,
            );
            assert_eq!(status, 400, "{}", body_of(&raw));
            assert_eq!(body_of(&raw), format!(r#"{{"error":"{expected}"}}"#));
        }
    }

    /// The three reference routes, their exact wire shapes and their refusals.
    /// Runtime tests cover behaviour; this one proves the paths and the status
    /// mapping a browser actually sees.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn reference_commands_and_reads_keep_their_contract_over_http() {
        let dist = TempDist::built();
        let address = spawn(routes(test_state(&dist))).await;
        let bearer =
            format!("Authorization: Bearer {DEVICE_TOKEN}\r\nx-orbit-device-id: device-1\r\n");
        for (path, body) in [
            (
                "/v1/authoritative/project-commands",
                r#"{"command_id":"p-create","expected_revision":0,"project_id":"orbit","name":"Orbit"}"#,
            ),
            (
                "/v1/authoritative/task-commands",
                r#"{"command_id":"t-create","expected_revision":1,"action":{"type":"create","task_id":"task-1","title":"Fix the login","project_id":"orbit"}}"#,
            ),
            (
                "/v1/authoritative/folder-creates",
                r#"{"command_id":"f-create","command":{"path":"Work"}}"#,
            ),
            (
                "/v1/authoritative/markdown-commands",
                r##"{"command_id":"n-create","command":{"kind":"Create","note_id":"note-a","path":"Work/a.md","body":"# A\n"}}"##,
            ),
        ] {
            let (status, raw) = post(&address, path, &bearer, body);
            assert_eq!(status, 200, "{path}: {}", body_of(&raw));
        }

        let (status, raw) = post(
            &address,
            "/v1/authoritative/markdown-reference-commands",
            &bearer,
            r#"{"command_id":"ref-1","note_id":"note-a","target":"project:orbit","action":"add"}"#,
        );
        assert_eq!(status, 200, "{}", body_of(&raw));
        let added: serde_json::Value = serde_json::from_str(body_of(&raw)).unwrap();
        assert_eq!(
            added,
            serde_json::json!({"command_id":"ref-1","note_id":"note-a","revision":2,
                "changed":true,"target":"project:orbit","action":"add"})
        );

        // A replay answers with the stored receipt, not a second revision.
        let (status, raw) = post(
            &address,
            "/v1/authoritative/markdown-reference-commands",
            &bearer,
            r#"{"command_id":"ref-1","note_id":"note-a","target":"project:orbit","action":"add"}"#,
        );
        assert_eq!(status, 200, "{}", body_of(&raw));
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(body_of(&raw)).unwrap(),
            added
        );

        let (status, raw) = get(
            &address,
            "/v1/authoritative/markdown-references?target=project:orbit",
            &bearer,
        );
        assert_eq!(status, 200, "{}", body_of(&raw));
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(body_of(&raw)).unwrap(),
            serde_json::json!({"target":"project:orbit","notes":[
                {"id":"note-a","path":"Work/a.md","title":"a"}]})
        );

        let (status, raw) = get(
            &address,
            "/v1/authoritative/project-knowledge?project_id=orbit",
            &bearer,
        );
        assert_eq!(status, 200, "{}", body_of(&raw));
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(body_of(&raw)).unwrap(),
            serde_json::json!({"project_id":"orbit","notes":[
                {"id":"note-a","path":"Work/a.md","title":"a",
                 "references":[{"kind":"project","id":"orbit"}]}],
                "tasks":["task-1"]})
        );

        // Every refusal keeps its fixed text and its 400/409 status.
        for (body, status_code, expected) in [
            (
                r#"{"command_id":"ref-2","note_id":"note-a","target":"habit:stretch","action":"add"}"#,
                400,
                "unsupported reference target kind",
            ),
            (
                r#"{"command_id":"ref-2","note_id":"note-a","target":"project:missing","action":"add"}"#,
                400,
                "unknown reference target",
            ),
            (
                r#"{"command_id":"ref-2","note_id":"note-a","target":"project","action":"add"}"#,
                400,
                "invalid reference command",
            ),
            (
                r#"{"command_id":"ref-2","note_id":"note-a","target":"project:orbit","action":"replace"}"#,
                400,
                "invalid reference command",
            ),
            (
                r#"{"command_id":"ref-2","note_id":"bad id","target":"project:orbit","action":"add"}"#,
                400,
                "invalid note id",
            ),
            (
                r#"{"command_id":"ref-2","note_id":"note-missing","target":"project:orbit","action":"add"}"#,
                400,
                "note does not exist",
            ),
            (
                r#"{"command_id":"ref-1","note_id":"note-a","target":"task:task-1","action":"add"}"#,
                409,
                "immutable sequence already used",
            ),
        ] {
            let (status, raw) = post(
                &address,
                "/v1/authoritative/markdown-reference-commands",
                &bearer,
                body,
            );
            assert_eq!(status, status_code, "{body}: {}", body_of(&raw));
            assert_eq!(
                body_of(&raw),
                format!(r#"{{"error":"{expected}"}}"#),
                "{body}"
            );
        }

        // A read refuses an unsupported or malformed target instead of
        // answering with an empty note list.
        for (path, expected) in [
            (
                "/v1/authoritative/markdown-references?target=habit:stretch",
                "unsupported reference target kind",
            ),
            (
                "/v1/authoritative/markdown-references?target=orbit",
                "invalid reference target",
            ),
            (
                "/v1/authoritative/project-knowledge?project_id=missing",
                "unknown reference target",
            ),
        ] {
            let (status, raw) = get(&address, path, &bearer);
            assert_eq!(status, 400, "{path}: {}", body_of(&raw));
            assert_eq!(body_of(&raw), format!(r#"{{"error":"{expected}"}}"#));
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn saved_views_round_trip_without_moving_the_revision() {
        let dist = TempDist::built();
        let address = spawn(routes(test_state(&dist))).await;
        let bearer =
            format!("Authorization: Bearer {DEVICE_TOKEN}\r\nx-orbit-device-id: device-1\r\n");
        let (_, raw) = get(&address, "/v1/authoritative/snapshot", &bearer);
        let before: serde_json::Value = serde_json::from_str(body_of(&raw)).unwrap();
        let revision = before["revision"].clone();

        let (status, raw) = post(
            &address,
            "/v1/authoritative/saved-views/commands",
            &bearer,
            r#"{"type":"save","command_id":"c-1","view_id":"sv-1","expected_version":0,"name":"Ready work","scope":"project","project_id":"orbit","query":"status=ready&sort=due","display":"list"}"#,
        );
        assert_eq!(status, 200, "{}", body_of(&raw));
        let saved: serde_json::Value = serde_json::from_str(body_of(&raw)).unwrap();
        assert_eq!(saved["duplicate"], false);
        assert_eq!(saved["view"]["version"], 1);
        assert_eq!(saved["view"]["display"], "list");

        let (status, raw) = get(&address, "/v1/authoritative/saved-views", &bearer);
        assert_eq!(status, 200, "{}", body_of(&raw));
        let listed: serde_json::Value = serde_json::from_str(body_of(&raw)).unwrap();
        assert_eq!(listed["views"].as_array().map(Vec::len), Some(1));
        assert_eq!(listed["views"][0]["name"], "Ready work");
        assert!(listed["views"][0].get("deleted").is_none());

        // A replayed command id answers from the receipt.
        let (status, raw) = post(
            &address,
            "/v1/authoritative/saved-views/commands",
            &bearer,
            r#"{"type":"save","command_id":"c-1","view_id":"sv-1","expected_version":0,"name":"Ready work","scope":"project","project_id":"orbit","query":"status=ready&sort=due","display":"list"}"#,
        );
        assert_eq!(status, 200, "{}", body_of(&raw));
        let replay: serde_json::Value = serde_json::from_str(body_of(&raw)).unwrap();
        assert_eq!(replay["duplicate"], true);

        // A stale per-row version is a fixed-text refusal.
        let (status, raw) = post(
            &address,
            "/v1/authoritative/saved-views/commands",
            &bearer,
            r#"{"type":"rename","command_id":"c-2","view_id":"sv-1","expected_version":9,"name":"Ready queue"}"#,
        );
        assert_eq!(status, 400, "{}", body_of(&raw));
        assert_eq!(
            body_of(&raw),
            r#"{"error":"saved view changed on the server"}"#
        );

        let (status, raw) = post(
            &address,
            "/v1/authoritative/saved-views/commands",
            &bearer,
            r#"{"type":"rename","command_id":"c-3","view_id":"sv-1","expected_version":1,"name":"Ready queue"}"#,
        );
        assert_eq!(status, 200, "{}", body_of(&raw));
        let renamed: serde_json::Value = serde_json::from_str(body_of(&raw)).unwrap();
        assert_eq!(renamed["view"]["version"], 2);
        assert_eq!(renamed["view"]["name"], "Ready queue");

        let (status, raw) = post(
            &address,
            "/v1/authoritative/saved-views/commands",
            &bearer,
            r#"{"type":"delete","command_id":"c-4","view_id":"sv-1","expected_version":2}"#,
        );
        assert_eq!(status, 200, "{}", body_of(&raw));
        let deleted: serde_json::Value = serde_json::from_str(body_of(&raw)).unwrap();
        assert_eq!(deleted["view"], serde_json::Value::Null);

        let (_, raw) = get(&address, "/v1/authoritative/saved-views", &bearer);
        let listed: serde_json::Value = serde_json::from_str(body_of(&raw)).unwrap();
        assert_eq!(listed["views"], serde_json::json!([]));

        // Unknown tags and unknown fields are refused by the tagged union.
        let (status, _) = post(
            &address,
            "/v1/authoritative/saved-views/commands",
            &bearer,
            r#"{"type":"archive","command_id":"c-5","view_id":"sv-1","expected_version":1}"#,
        );
        assert_eq!(status, 422);

        // None of this moved the authority revision or the ledger.
        let (_, raw) = get(&address, "/v1/authoritative/snapshot", &bearer);
        let after: serde_json::Value = serde_json::from_str(body_of(&raw)).unwrap();
        assert_eq!(after["revision"], revision);
        assert_eq!(after["history_fingerprint"], before["history_fingerprint"]);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn task_detail_reports_history_derived_timestamps() {
        let dist = TempDist::built();
        let state = test_state(&dist);
        let address = spawn(routes(state.clone())).await;
        let bearer =
            format!("Authorization: Bearer {DEVICE_TOKEN}\r\nx-orbit-device-id: device-1\r\n");
        let (status, raw) = post(
            &address,
            "/v1/authoritative/task-commands",
            &bearer,
            r#"{"command_id":"t-create","expected_revision":0,"action":{"type":"create","task_id":"task-1","title":"Ship it"}}"#,
        );
        assert_eq!(status, 200, "{}", body_of(&raw));
        let (status, raw) = get(&address, "/v1/authoritative/tasks/task-1", &bearer);
        assert_eq!(status, 200, "{}", body_of(&raw));
        let body: serde_json::Value = serde_json::from_str(body_of(&raw)).unwrap();
        let created = body["created_at_ms"]
            .as_i64()
            .expect("a created activity exists");
        assert_eq!(body["updated_at_ms"].as_i64(), Some(created));

        // The unknown case (a task row without history) is covered by the
        // runtime read; the HTTP projection only ever serialises what it finds.
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn failed_logins_are_uniform_and_throttled() {
        let dist = TempDist::built();
        let address = spawn(routes(test_state(&dist))).await;
        let bad = r#"{"device_id":"device-1","token":"wrong-token-0123456789abcdef"}"#;
        for _ in 0..5 {
            let (status, raw) = post(&address, "/v1/web/session", "", bad);
            assert_eq!(status, 401);
            assert!(body_of(&raw).contains("invalid device credentials"));
        }
        let (status, raw) = post(&address, "/v1/web/session", "", bad);
        assert_eq!(status, 429);
        assert!(body_of(&raw).contains("too many attempts"));
        // An unknown device id is indistinguishable from a wrong token.
        let unknown = r#"{"device_id":"device-unknown","token":"wrong-token-0123456789abcdef"}"#;
        let (status, raw) = post(&address, "/v1/web/session", "", unknown);
        assert_eq!(status, 401);
        assert!(body_of(&raw).contains("invalid device credentials"));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn padded_device_ids_share_the_login_throttle_bucket() {
        let dist = TempDist::built();
        let address = spawn(routes(test_state(&dist))).await;
        let padded = r#"{"device_id":"device-1 ","token":"wrong-token-0123456789abcdef"}"#;
        for _ in 0..5 {
            let (status, _) = post(&address, "/v1/web/session", "", padded);
            assert_eq!(status, 401);
        }
        // The plain id is the same identity after trimming, so the failure
        // series blocks it instead of starting a fresh bucket.
        let correct = format!(r#"{{"device_id":"device-1","token":"{DEVICE_TOKEN}"}}"#);
        let (status, raw) = post(&address, "/v1/web/session", "", &correct);
        assert_eq!(status, 429);
        assert!(body_of(&raw).contains("too many attempts"));
    }

    const FEATURE_KIND: &str = r#"{"id":"feature","name":"Feature","initial_state":"backlog",
        "states":["backlog","ready","done"],"fields":[{"id":"notes","kind":{"type":"text"}}],
        "transitions":[{"from":"backlog","to":"ready"},{"from":"ready","to":"done"}]}"#;
    const CHORE_KIND: &str = r#"{"id":"chore","name":"Chore","initial_state":"todo",
        "states":["todo","done"],"transitions":[{"from":"todo","to":"done"}]}"#;
    const UPGRADE_BLUEPRINT: &str = "upgrade-demo";

    fn manifest(version: u32, kinds: &[&str]) -> orbit_blueprint::Blueprint {
        serde_json::from_str(&format!(
            r#"{{"id":"{UPGRADE_BLUEPRINT}","version":{version},"name":"Upgrade demo","work_items":[{}]}}"#,
            kinds.join(",")
        ))
        .expect("valid manifest")
    }

    /// A project pinned to version 1 with version 2 published. `with_live_item`
    /// creates a work item that version 2 removes, which is a blocker.
    fn upgrade_state(dist: &TempDist, with_live_item: bool) -> AppState {
        let state = test_state(dist);
        {
            let mut host = state.store.lock().unwrap();
            host.publish_project_blueprint(&manifest(1, &[FEATURE_KIND]), &BTreeSet::new())
                .unwrap();
            // Creating the project with an explicit workflow pins version 1 in
            // the same transaction; a second bind is not allowed.
            host.project_command(
                "device-1",
                domain_commands::ProjectCommand {
                    command_id: "project-1-create".into(),
                    expected_revision: 0,
                    project_id: "project-1".into(),
                    name: "Project 1".into(),
                    workflow_id: Some(UPGRADE_BLUEPRINT.into()),
                },
            )
            .unwrap();
            let target = if with_live_item {
                host.create_project_work_item("item-1", "project-1", "feature", BTreeMap::new())
                    .unwrap();
                vec![CHORE_KIND]
            } else {
                vec![FEATURE_KIND, CHORE_KIND]
            };
            host.publish_project_blueprint(&manifest(2, &target), &BTreeSet::new())
                .unwrap();
        }
        state
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn upgrade_apply_requires_a_fingerprint() {
        let dist = TempDist::built();
        let state = upgrade_state(&dist, false);
        let address = spawn(routes(state)).await;
        let bearer =
            format!("Authorization: Bearer {DEVICE_TOKEN}\r\nx-orbit-device-id: device-1\r\n");
        // Confirmed but no fingerprint: the server never derives one.
        let (status, raw) = post(
            &address,
            "/v1/authoritative/blueprints/upgrade",
            &bearer,
            r#"{"project_id":"project-1","id":"upgrade-demo","version":2,"confirm":true}"#,
        );
        assert_eq!(status, 400);
        assert!(body_of(&raw).contains("fingerprint required"));
        // An empty fingerprint is equally unacceptable.
        let (status, raw) = post(
            &address,
            "/v1/authoritative/blueprints/upgrade",
            &bearer,
            r#"{"project_id":"project-1","id":"upgrade-demo","version":2,"confirm":true,"fingerprint":"  "}"#,
        );
        assert_eq!(status, 400);
        assert!(body_of(&raw).contains("fingerprint required"));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn upgrade_apply_rejects_a_stale_fingerprint() {
        let dist = TempDist::built();
        let state = upgrade_state(&dist, false);
        let address = spawn(routes(state)).await;
        let bearer =
            format!("Authorization: Bearer {DEVICE_TOKEN}\r\nx-orbit-device-id: device-1\r\n");
        let (status, raw) = post(
            &address,
            "/v1/authoritative/blueprints/upgrade",
            &bearer,
            r#"{"project_id":"project-1","id":"upgrade-demo","version":2,"confirm":true,"fingerprint":"not-the-preview"}"#,
        );
        assert_eq!(status, 409);
        assert!(body_of(&raw).contains("upgrade fingerprint mismatch"));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn upgrade_apply_reports_blockers_distinctly() {
        let dist = TempDist::built();
        let state = upgrade_state(&dist, true);
        let address = spawn(routes(state)).await;
        let bearer =
            format!("Authorization: Bearer {DEVICE_TOKEN}\r\nx-orbit-device-id: device-1\r\n");
        let preview_path = "/v1/authoritative/blueprints/upgrade-preview?project_id=project-1&id=upgrade-demo&version=2";
        let (status, raw) = get(&address, preview_path, &bearer);
        assert_eq!(status, 200);
        let preview: serde_json::Value = serde_json::from_str(body_of(&raw)).unwrap();
        assert!(!preview["blockers"].as_array().unwrap().is_empty());
        let fingerprint = preview["fingerprint"].as_str().unwrap();
        let (status, raw) = post(
            &address,
            "/v1/authoritative/blueprints/upgrade",
            &bearer,
            &format!(
                r#"{{"project_id":"project-1","id":"upgrade-demo","version":2,"confirm":true,"fingerprint":"{fingerprint}"}}"#
            ),
        );
        assert_eq!(status, 409);
        assert!(body_of(&raw).contains("workflow upgrade blocked"));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn upgrade_apply_accepts_the_previewed_fingerprint() {
        let dist = TempDist::built();
        let state = upgrade_state(&dist, false);
        let address = spawn(routes(state)).await;
        let bearer =
            format!("Authorization: Bearer {DEVICE_TOKEN}\r\nx-orbit-device-id: device-1\r\n");
        let preview_path = "/v1/authoritative/blueprints/upgrade-preview?project_id=project-1&id=upgrade-demo&version=2";
        let (status, raw) = get(&address, preview_path, &bearer);
        assert_eq!(status, 200);
        let preview: serde_json::Value = serde_json::from_str(body_of(&raw)).unwrap();
        assert!(preview["blockers"].as_array().unwrap().is_empty());
        let fingerprint = preview["fingerprint"].as_str().unwrap();
        let (status, raw) = post(
            &address,
            "/v1/authoritative/blueprints/upgrade",
            &bearer,
            &format!(
                r#"{{"project_id":"project-1","id":"upgrade-demo","version":2,"confirm":true,"fingerprint":"{fingerprint}"}}"#
            ),
        );
        assert_eq!(status, 200, "{}", body_of(&raw));
        assert!(body_of(&raw).contains("\"already_applied\":false"));
        // The accepted binding is the version whose preview was approved.
        let (status, raw) = get(
            &address,
            "/v1/authoritative/blueprints/binding?project_id=project-1",
            &bearer,
        );
        assert_eq!(status, 200);
        assert!(body_of(&raw).contains("\"version\":2"));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn upgrade_preview_requires_a_live_device() {
        let dist = TempDist::built();
        let address = spawn(routes(test_state(&dist))).await;
        let unknown = format!(
            "Authorization: Bearer {DEVICE_TOKEN}\r\nx-orbit-device-id: device-unknown\r\n"
        );
        let (status, raw) = get(
            &address,
            "/v1/authoritative/blueprints/upgrade-preview?project_id=project-1&id=upgrade-demo&version=2",
            &unknown,
        );
        assert_eq!(status, 401);
        assert!(body_of(&raw).contains("unauthorized"));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn static_app_and_api_404s() {
        let dist = TempDist::built();
        let address = spawn(routes(test_state(&dist))).await;
        let (status, raw) = get(&address, "/", "");
        assert_eq!(status, 200);
        assert_eq!(
            header_value(&raw, "content-type").unwrap(),
            "text/html; charset=utf-8"
        );
        assert!(
            header_value(&raw, "content-security-policy")
                .unwrap()
                .contains("frame-ancestors 'none'")
        );
        assert_eq!(header_value(&raw, "cache-control").unwrap(), "no-store");
        assert!(body_of(&raw).contains("Orbit"));

        let (status, raw) = get(&address, "/workflows/123", "");
        assert_eq!(status, 200);
        assert!(body_of(&raw).contains("Orbit"));

        let (status, raw) = get(&address, "/assets/app.js", "");
        assert_eq!(status, 200);
        assert_eq!(
            header_value(&raw, "cache-control").unwrap(),
            "public, max-age=31536000, immutable"
        );

        let (status, raw) = get(&address, "/v1/nope", "");
        assert_eq!(status, 404);
        assert_eq!(
            header_value(&raw, "content-type").unwrap(),
            "application/json"
        );
        assert!(body_of(&raw).contains("not found"));
        // `/mcp` is an existing POST-only route, so a GET stays a 405; the
        // static handler guards it with a JSON 404 if it is ever reached.
        let (status, _) = get(&address, "/mcp", "");
        assert_eq!(status, 405);

        let (status, _) = get(&address, "/console", "");
        assert_eq!(status, 200);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn missing_build_keeps_the_console_and_api_working() {
        let dist = TempDist::absent();
        let address = spawn(routes(test_state(&dist))).await;
        let (status, raw) = get(&address, "/", "");
        assert_eq!(status, 503);
        assert_eq!(
            header_value(&raw, "content-type").unwrap(),
            "text/plain; charset=utf-8"
        );
        assert!(body_of(&raw).contains("npm run build"));
        let (status, _) = get(&address, "/console", "");
        assert_eq!(status, 200);
        let (status, raw) = get(&address, "/v1/nope", "");
        assert_eq!(status, 404);
        assert!(body_of(&raw).contains("not found"));
    }

    #[test]
    fn cookie_parsing_is_tolerant() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::COOKIE,
            "theme=dark; malformed; orbit_session=abc123"
                .parse()
                .unwrap(),
        );
        assert_eq!(session_cookie(&headers).as_deref(), Some("abc123"));
        let mut spaced = HeaderMap::new();
        spaced.insert(
            header::COOKIE,
            "orbit_session=abc 123; theme=dark".parse().unwrap(),
        );
        assert!(session_cookie(&spaced).is_none());
        let mut empty = HeaderMap::new();
        empty.insert(header::COOKIE, "orbit_session=".parse().unwrap());
        assert!(session_cookie(&empty).is_none());
        let mut other = HeaderMap::new();
        other.insert(header::COOKIE, "theme=dark".parse().unwrap());
        assert!(session_cookie(&other).is_none());
    }

    #[test]
    fn csrf_applies_only_to_cookie_mutations() {
        let mut cookie = HeaderMap::new();
        cookie.insert(header::COOKIE, "orbit_session=abc".parse().unwrap());
        assert!(csrf_violation(
            &Method::POST,
            &cookie,
            "/v1/authoritative/projects"
        ));
        assert!(!csrf_violation(
            &Method::GET,
            &cookie,
            "/v1/authoritative/projects"
        ));
        assert!(!csrf_violation(&Method::POST, &cookie, "/v1/web/session"));
        // Logout is a cookie mutation; the browser client sends the header.
        assert!(csrf_violation(&Method::DELETE, &cookie, "/v1/web/session"));
        let mut with_header = cookie.clone();
        with_header.insert("x-orbit-csrf", "1".parse().unwrap());
        assert!(!csrf_violation(
            &Method::POST,
            &with_header,
            "/v1/authoritative/projects"
        ));
        let mut cross_site = with_header;
        cross_site.insert("sec-fetch-site", "cross-site".parse().unwrap());
        assert!(csrf_violation(
            &Method::POST,
            &cross_site,
            "/v1/authoritative/projects"
        ));
        let bearer = HeaderMap::new();
        assert!(!csrf_violation(
            &Method::POST,
            &bearer,
            "/v1/authoritative/projects"
        ));
    }

    /// The browser needs an honest executor roster before it can offer a launch,
    /// and the two start refusals are wire contracts the client quotes verbatim.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn agent_executor_read_and_start_refusals_over_http() {
        let dist = TempDist::built();
        let state = test_state(&dist);
        {
            let mut store = state.store.lock().unwrap();
            store
                .enroll_device("executor-1", "executor-secret-0123456789abcdefgh")
                .unwrap();
            store
                .enroll_device("executor-2", "executor-secret-9876543210abcdefgh")
                .unwrap();
            store.revoke_device("executor-2").unwrap();
        }
        let address = spawn(routes(state)).await;
        let bearer =
            format!("Authorization: Bearer {DEVICE_TOKEN}\r\nx-orbit-device-id: device-1\r\n");

        let (status, raw) = get(&address, "/v1/authoritative/agent-executors", &bearer);
        assert_eq!(status, 200, "{}", body_of(&raw));
        let body: serde_json::Value = serde_json::from_str(body_of(&raw)).unwrap();
        let executors = body["executors"]
            .as_array()
            .expect("executors is always an array");
        let ids: Vec<&str> = executors
            .iter()
            .map(|executor| executor["device_id"].as_str().unwrap_or_default())
            .collect();
        assert_eq!(
            ids,
            ["device-1", "executor-1", "executor-2"],
            "ordered by device id, including revoked but not the authority identity"
        );
        let enrolled = &executors[1];
        assert_eq!(enrolled["active"], true);
        assert_eq!(enrolled["last_claim_at_ms"], serde_json::Value::Null);
        assert_eq!(enrolled["claimed_jobs"], 0);
        assert_eq!(enrolled["preparing_jobs"], 0);
        // A revoked executor is reported, so the client can explain why it is
        // unavailable instead of the device silently vanishing.
        assert_eq!(executors[2]["active"], false);

        // An unknown executor is refused before any job is created.
        let (status, raw) = post(
            &address,
            "/v1/authoritative/agent-jobs/start",
            &bearer,
            r#"{"job_id":"job-1","task_id":"task-1","agent_kind":"codex",
                "machine_ref":"ghost","repository_ref":"/srv/repo",
                "worktree":false,"at_ms":1000}"#,
        );
        assert_eq!(status, 400, "{}", body_of(&raw));
        assert_eq!(
            body_of(&raw),
            r#"{"error":"unknown agent executor"}"#,
            "{}",
            body_of(&raw)
        );
        let (status, raw) = get(&address, "/v1/authoritative/agent-jobs?id=job-1", &bearer);
        assert_eq!(status, 403, "{}", body_of(&raw));
        assert!(body_of(&raw).contains("device identity mismatch"));

        // The revoked executor is refused with its own fixed text.
        let (status, raw) = post(
            &address,
            "/v1/authoritative/agent-jobs/start",
            &bearer,
            r#"{"job_id":"job-2","task_id":"task-1","agent_kind":"codex",
                "machine_ref":"executor-2","repository_ref":"/srv/repo",
                "worktree":false,"at_ms":1000}"#,
        );
        assert_eq!(status, 400, "{}", body_of(&raw));
        assert_eq!(body_of(&raw), r#"{"error":"inactive agent executor"}"#);

        // A real project and a task that is still in the inbox: the storage
        // refusal must survive to the wire instead of the generic body.
        let (status, raw) = post(
            &address,
            "/v1/authoritative/project-commands",
            &bearer,
            r#"{"command_id":"p-create","expected_revision":0,"project_id":"orbit","name":"Orbit"}"#,
        );
        assert_eq!(status, 200, "{}", body_of(&raw));
        let (status, raw) = post(
            &address,
            "/v1/authoritative/task-commands",
            &bearer,
            r#"{"command_id":"t-create","expected_revision":1,"action":{"type":"create","task_id":"task-1","title":"Fix the login","project_id":"orbit"}}"#,
        );
        assert_eq!(status, 200, "{}", body_of(&raw));
        let (status, raw) = post(
            &address,
            "/v1/authoritative/agent-jobs/start",
            &bearer,
            r#"{"job_id":"job-3","task_id":"task-1","agent_kind":"codex",
                "machine_ref":"executor-1","repository_ref":"/srv/repo",
                "worktree":false,"at_ms":1000,
                "objective":"Fix the login redirect"}"#,
        );
        assert_eq!(status, 400, "{}", body_of(&raw));
        assert_eq!(
            body_of(&raw),
            r#"{"error":"task not eligible for connected start"}"#,
            "{}",
            body_of(&raw)
        );
        // No job was created by any refusal.
        let (status, raw) = get(&address, "/v1/authoritative/agent-jobs/overview", &bearer);
        assert_eq!(status, 200, "{}", body_of(&raw));
        let overview: serde_json::Value = serde_json::from_str(body_of(&raw)).unwrap();
        assert_eq!(overview.as_array().map(Vec::len), Some(0));
    }

    /// An objective longer than the cap is refused with a fixed message, and the
    /// additive field keeps old payloads (no objective) working.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn agent_start_accepts_an_objective_and_refuses_an_oversized_one() {
        let dist = TempDist::built();
        let state = test_state(&dist);
        state
            .store
            .lock()
            .unwrap()
            .enroll_device("executor-1", "executor-secret-0123456789abcdefgh")
            .unwrap();
        let address = spawn(routes(state)).await;
        let bearer =
            format!("Authorization: Bearer {DEVICE_TOKEN}\r\nx-orbit-device-id: device-1\r\n");
        let (status, raw) = post(
            &address,
            "/v1/authoritative/project-commands",
            &bearer,
            r#"{"command_id":"p-create","expected_revision":0,"project_id":"orbit","name":"Orbit"}"#,
        );
        assert_eq!(status, 200, "{}", body_of(&raw));
        let (status, raw) = post(
            &address,
            "/v1/authoritative/task-commands",
            &bearer,
            r#"{"command_id":"t-create","expected_revision":1,"action":{"type":"create","task_id":"task-1","title":"Fix the login","project_id":"orbit"}}"#,
        );
        assert_eq!(status, 200, "{}", body_of(&raw));
        let (status, raw) = post(
            &address,
            "/v1/authoritative/task-commands",
            &bearer,
            r#"{"command_id":"t-ready","expected_revision":2,"action":{"type":"change_status","task_id":"task-1","status":"ready"}}"#,
        );
        assert_eq!(status, 200, "{}", body_of(&raw));

        let objective = "o".repeat(4001);
        let (status, raw) = post(
            &address,
            "/v1/authoritative/agent-jobs/start",
            &bearer,
            &format!(
                r#"{{"job_id":"job-long","task_id":"task-1","agent_kind":"codex",
                    "machine_ref":"executor-1","repository_ref":"/srv/repo",
                    "worktree":false,"at_ms":1000,"objective":"{objective}"}}"#
            ),
        );
        assert_eq!(status, 400, "{}", body_of(&raw));
        assert_eq!(body_of(&raw), r#"{"error":"objective is too long"}"#);

        // Without an objective the same payload shape still works, and the
        // response reports the claim state the browser must display.
        let (status, raw) = post(
            &address,
            "/v1/authoritative/agent-jobs/start",
            &bearer,
            r#"{"job_id":"job-1","task_id":"task-1","agent_kind":"codex",
                "machine_ref":"executor-1","repository_ref":"/srv/repo",
                "worktree":false,"at_ms":1000}"#,
        );
        assert_eq!(status, 200, "{}", body_of(&raw));
        let job: serde_json::Value = serde_json::from_str(body_of(&raw)).unwrap();
        assert_eq!(job["state"], "preparing");
        assert_eq!(job["claim_state"], serde_json::Value::Null);

        let (status, raw) = get(&address, "/v1/authoritative/agent-jobs/overview", &bearer);
        assert_eq!(status, 200, "{}", body_of(&raw));
        let overview: serde_json::Value = serde_json::from_str(body_of(&raw)).unwrap();
        assert_eq!(overview[0]["next_action"], "await_executor");
        assert_eq!(overview[0]["needs_attention"], false);
    }
}
