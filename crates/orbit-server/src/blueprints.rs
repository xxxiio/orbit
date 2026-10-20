//! Authenticated blueprint transport. Business rules stay in AuthoritativeHost.
use super::*;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Deserialize)]
pub(super) struct BlueprintKey {
    id: String,
    version: u32,
}
#[derive(Deserialize)]
pub(super) struct BlueprintList {
    id: Option<String>,
}
#[derive(Deserialize)]
pub(super) struct ProjectKey {
    project_id: String,
}
#[derive(Deserialize)]
pub(super) struct ItemKey {
    id: String,
}
#[derive(Deserialize)]
pub(super) struct PublishBody {
    definition: orbit_blueprint::Blueprint,
}
#[derive(Deserialize)]
pub(super) struct BindBody {
    project_id: String,
    id: String,
    version: u32,
}
#[derive(Deserialize)]
pub(super) struct InstallBody {
    id: String,
}
#[derive(Deserialize)]
pub(super) struct CreateItemBody {
    id: String,
    project_id: String,
    kind: String,
    #[serde(default)]
    properties: BTreeMap<String, serde_json::Value>,
}
#[derive(Deserialize)]
pub(super) struct TransitionBody {
    id: String,
    to: String,
    expected_revision: u64,
}
#[derive(Deserialize)]
pub(super) struct PropertiesBody {
    id: String,
    expected_revision: u64,
    changes: BTreeMap<String, serde_json::Value>,
}
#[derive(Deserialize)]
pub(super) struct TemplateBody {
    project_id: String,
    template_id: String,
    namespace: String,
}

fn repo_error(err: orbit_application::RepositoryError) -> ApiError {
    match err {
        orbit_application::RepositoryError::Conflict => {
            error(StatusCode::CONFLICT, "blueprint conflict")
        }
        orbit_application::RepositoryError::Unavailable => error(
            StatusCode::SERVICE_UNAVAILABLE,
            "blueprint storage unavailable",
        ),
        orbit_application::RepositoryError::CorruptData { .. } => {
            error(StatusCode::INTERNAL_SERVER_ERROR, "blueprint storage error")
        }
        // RepositoryError::Other carries both validation failures and storage errors;
        // keep diagnostic details out of HTTP responses.
        orbit_application::RepositoryError::Other(_) => {
            error(StatusCode::BAD_REQUEST, "blueprint operation rejected")
        }
    }
}
fn locked(state: &AppState) -> Result<std::sync::MutexGuard<'_, AuthoritativeHost>, ApiError> {
    state
        .store
        .lock()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "storage lock error"))
}

pub(super) async fn list(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<BlueprintList>,
) -> Result<Json<Vec<(String, u32)>>, ApiError> {
    device(&state, &headers)?;
    Ok(Json(
        locked(&state)?
            .available_blueprint_versions(q.id.as_deref())
            .map_err(repo_error)?,
    ))
}
pub(super) async fn show(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<BlueprintKey>,
) -> Result<Json<orbit_blueprint::Blueprint>, ApiError> {
    device(&state, &headers)?;
    let blueprint = locked(&state)?
        .project_blueprint(&q.id, q.version)
        .map_err(repo_error)?
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "blueprint not found"))?;
    Ok(Json(blueprint))
}
pub(super) async fn publish(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<PublishBody>,
) -> Result<Json<orbit_blueprint::Blueprint>, ApiError> {
    device(&state, &headers)?;
    // Remote manifests cannot declare themselves authorized or install capabilities.
    locked(&state)?
        .publish_project_blueprint(&body.definition, &BTreeSet::new())
        .map_err(repo_error)?;
    Ok(Json(body.definition))
}
pub(super) async fn install(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<InstallBody>,
) -> Result<Json<orbit_blueprint::Blueprint>, ApiError> {
    device(&state, &headers)?;
    Ok(Json(
        locked(&state)?
            .install_bundled_project_blueprint(&body.id)
            .map_err(repo_error)?,
    ))
}
pub(super) async fn bundled(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ApiError> {
    device(&state, &headers)?;
    // Serialize descriptions by public IDs without exposing internal provider types.
    Ok(Json(serde_json::json!(
        locked(&state)?
            .bundled_project_blueprints()
            .iter()
            .map(|b| b.id)
            .collect::<Vec<_>>()
    )))
}
pub(super) async fn bind(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<BindBody>,
) -> Result<Json<orbit_blueprint::BlueprintBinding>, ApiError> {
    device(&state, &headers)?;
    Ok(Json(
        locked(&state)?
            .activate_project_blueprint(&body.project_id, &body.id, body.version)
            .map_err(repo_error)?,
    ))
}
pub(super) async fn binding(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<ProjectKey>,
) -> Result<Json<Option<orbit_blueprint::BlueprintBinding>>, ApiError> {
    device(&state, &headers)?;
    Ok(Json(
        locked(&state)?
            .active_project_blueprint(&q.project_id)
            .map_err(repo_error)?,
    ))
}
pub(super) async fn create_item(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<CreateItemBody>,
) -> Result<Json<orbit_blueprint::BlueprintWorkItem>, ApiError> {
    device(&state, &headers)?;
    Ok(Json(
        locked(&state)?
            .create_project_work_item(&body.id, &body.project_id, &body.kind, body.properties)
            .map_err(repo_error)?,
    ))
}
pub(super) async fn item(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<ItemKey>,
) -> Result<Json<orbit_blueprint::BlueprintWorkItem>, ApiError> {
    device(&state, &headers)?;
    let item = locked(&state)?
        .project_work_item(&q.id)
        .map_err(repo_error)?
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "work item not found"))?;
    Ok(Json(item))
}
pub(super) async fn transition(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<TransitionBody>,
) -> Result<Json<orbit_blueprint::BlueprintWorkItem>, ApiError> {
    device(&state, &headers)?;
    // No capability grants from remote callers; external effects require trusted execution.
    Ok(Json(
        locked(&state)?
            .transition_project_work_item(
                &body.id,
                &body.to,
                body.expected_revision,
                &BTreeSet::new(),
            )
            .map_err(repo_error)?,
    ))
}
pub(super) async fn properties(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<PropertiesBody>,
) -> Result<Json<orbit_blueprint::BlueprintWorkItem>, ApiError> {
    device(&state, &headers)?;
    Ok(Json(
        locked(&state)?
            .update_project_work_item_properties(&body.id, body.expected_revision, body.changes)
            .map_err(repo_error)?,
    ))
}
pub(super) async fn template(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<TemplateBody>,
) -> Result<Json<orbit_blueprint::TemplatePlan>, ApiError> {
    device(&state, &headers)?;
    Ok(Json(
        locked(&state)?
            .apply_project_blueprint_template(&body.project_id, &body.template_id, &body.namespace)
            .map_err(repo_error)?,
    ))
}

#[derive(Deserialize)]
pub(super) struct AcceptedCommandBody {
    command_id: String,
    route: String,
    body: serde_json::Value,
}
/// Durable authoritative acceptance endpoint for canonical offline replay.
/// The command ID and authenticated device are bound to the result atomically.
pub(super) async fn accept_command(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<AcceptedCommandBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let owner = device(&state, &headers)?;
    let result = locked(&state)?
        .accept_blueprint_command(&request.command_id, &owner, &request.route, &request.body)
        .map_err(repo_error)?;
    Ok(Json(
        serde_json::json!({"command_id":request.command_id,"response":result}),
    ))
}
