//! Provider-neutral AI integration contracts for Orbit.
//!
//! Orbit remains fully functional without this crate. The AI layer proposes or
//! explains; it does not bypass deterministic application/planner validation.

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AiProviderKind {
    OpenAi,
    Anthropic,
    Gemini,
    OpenAiCompatible,
    Ollama,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AiCapabilities {
    pub chat: bool,
    pub tools: bool,
    pub structured_output: bool,
    pub embeddings: bool,
    pub vision: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiSecretRef {
    pub namespace: String,
    pub key: String,
}

impl AiSecretRef {
    pub fn new(
        namespace: impl Into<String>,
        key: impl Into<String>,
    ) -> Result<Self, AiConfigError> {
        let namespace = namespace.into();
        let key = key.into();
        if namespace.trim().is_empty() || key.trim().is_empty() {
            return Err(AiConfigError::InvalidSecretRef);
        }
        Ok(Self { namespace, key })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AiAuthMode {
    None,
    Bearer { secret: AiSecretRef },
    ApiKeyHeader { header: String, secret: AiSecretRef },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiProviderConfig {
    pub id: String,
    pub kind: AiProviderKind,
    pub endpoint: String,
    pub model: String,
    pub auth: AiAuthMode,
    pub capabilities: AiCapabilities,
    pub local_only: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AiConfigError {
    BlankId,
    BlankEndpoint,
    BlankModel,
    InvalidSecretRef,
    InvalidHeader,
    LocalOnlyRemoteEndpoint,
}

impl AiProviderConfig {
    pub fn new(
        id: impl Into<String>,
        kind: AiProviderKind,
        endpoint: impl Into<String>,
        model: impl Into<String>,
        auth: AiAuthMode,
        capabilities: AiCapabilities,
        local_only: bool,
    ) -> Result<Self, AiConfigError> {
        let id = id.into();
        let endpoint = endpoint.into();
        let model = model.into();
        if id.trim().is_empty() {
            return Err(AiConfigError::BlankId);
        }
        if endpoint.trim().is_empty() {
            return Err(AiConfigError::BlankEndpoint);
        }
        if model.trim().is_empty() {
            return Err(AiConfigError::BlankModel);
        }
        if let AiAuthMode::ApiKeyHeader { header, .. } = &auth
            && header.trim().is_empty()
        {
            return Err(AiConfigError::InvalidHeader);
        }
        if local_only && !is_local_endpoint(&endpoint) {
            return Err(AiConfigError::LocalOnlyRemoteEndpoint);
        }
        Ok(Self {
            id,
            kind,
            endpoint,
            model,
            auth,
            capabilities,
            local_only,
        })
    }

    pub fn ollama(
        id: impl Into<String>,
        endpoint: impl Into<String>,
        model: impl Into<String>,
    ) -> Result<Self, AiConfigError> {
        Self::new(
            id,
            AiProviderKind::Ollama,
            endpoint,
            model,
            AiAuthMode::None,
            AiCapabilities {
                chat: true,
                tools: true,
                structured_output: true,
                embeddings: true,
                vision: false,
            },
            true,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiRequest {
    pub system: Option<String>,
    pub user: String,
    pub require_structured_output: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiResponse {
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AiProviderError {
    UnsupportedCapability(&'static str),
    MissingSecret,
    Transport(String),
    InvalidResponse(String),
}

/// Provider adapter implemented by concrete HTTP/local-runtime integrations.
/// Implementations receive resolved secret bytes from composition code; secret
/// material must never be persisted in normal Orbit state or semantic sync.
pub trait AiProvider {
    fn config(&self) -> &AiProviderConfig;
    fn complete(
        &self,
        request: AiRequest,
        resolved_secret: Option<&[u8]>,
    ) -> Result<AiResponse, AiProviderError>;
}

pub fn validate_request(
    config: &AiProviderConfig,
    request: &AiRequest,
) -> Result<(), AiProviderError> {
    if !config.capabilities.chat {
        return Err(AiProviderError::UnsupportedCapability("chat"));
    }
    if request.require_structured_output && !config.capabilities.structured_output {
        return Err(AiProviderError::UnsupportedCapability("structured_output"));
    }
    let needs_secret = !matches!(config.auth, AiAuthMode::None);
    if needs_secret {
        // Resolution is checked by the concrete invocation path; this function
        // documents that provider configuration itself never contains a secret.
    }
    Ok(())
}

fn is_local_endpoint(endpoint: &str) -> bool {
    let lower = endpoint.trim().to_ascii_lowercase();
    lower.starts_with("http://localhost")
        || lower.starts_with("https://localhost")
        || lower.starts_with("http://127.0.0.1")
        || lower.starts_with("https://127.0.0.1")
        || lower.starts_with("http://[::1]")
        || lower.starts_with("https://[::1]")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ollama_config_is_local_and_requires_no_secret() {
        let config =
            AiProviderConfig::ollama("local", "http://localhost:11434/v1", "qwen3").unwrap();
        assert!(config.local_only);
        assert!(matches!(config.auth, AiAuthMode::None));
        assert!(config.capabilities.chat);
    }

    #[test]
    fn cloud_byok_config_stores_only_secret_reference() {
        let secret = AiSecretRef::new("ai-provider", "anthropic-default").unwrap();
        let config = AiProviderConfig::new(
            "anthropic",
            AiProviderKind::Anthropic,
            "https://api.anthropic.com",
            "claude-model",
            AiAuthMode::ApiKeyHeader {
                header: "x-api-key".into(),
                secret: secret.clone(),
            },
            AiCapabilities {
                chat: true,
                tools: true,
                structured_output: true,
                embeddings: false,
                vision: true,
            },
            false,
        )
        .unwrap();
        assert!(matches!(config.auth, AiAuthMode::ApiKeyHeader { .. }));
        assert_eq!(secret.key, "anthropic-default");
    }

    #[test]
    fn local_only_provider_rejects_remote_endpoint() {
        let result = AiProviderConfig::new(
            "private",
            AiProviderKind::OpenAiCompatible,
            "https://example.com/v1",
            "model",
            AiAuthMode::None,
            AiCapabilities {
                chat: true,
                ..AiCapabilities::default()
            },
            true,
        );
        assert_eq!(result, Err(AiConfigError::LocalOnlyRemoteEndpoint));
    }

    #[test]
    fn structured_output_requirement_is_capability_checked() {
        let config = AiProviderConfig::new(
            "basic",
            AiProviderKind::OpenAiCompatible,
            "http://localhost:8000/v1",
            "model",
            AiAuthMode::None,
            AiCapabilities {
                chat: true,
                structured_output: false,
                ..AiCapabilities::default()
            },
            true,
        )
        .unwrap();
        let error = validate_request(
            &config,
            &AiRequest {
                system: None,
                user: "plan today".into(),
                require_structured_output: true,
            },
        )
        .unwrap_err();
        assert_eq!(
            error,
            AiProviderError::UnsupportedCapability("structured_output")
        );
    }
}

// ---------------------------------------------------------------------------
// Privacy/access policy and disclosure boundary.

use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AiDataCategory {
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

impl AiDataCategory {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ProjectMetadata => "project_metadata",
            Self::TaskTitles => "task_titles",
            Self::TaskPlanning => "task_planning",
            Self::Schedule => "schedule",
            Self::HabitMetadata => "habit_metadata",
            Self::KnowledgeMetadata => "knowledge_metadata",
            Self::KnowledgeBodies => "knowledge_bodies",
            Self::ResourceMetadata => "resource_metadata",
            Self::ActivityHistory => "activity_history",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AiDestination {
    Local,
    Remote,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AiProjectAccessMode {
    Disabled,
    LocalOnly,
    SelectedProviders,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiProjectPolicy {
    pub mode: AiProjectAccessMode,
    pub allowed_remote_providers: BTreeSet<AiProviderKind>,
    pub allowed_categories: BTreeSet<AiDataCategory>,
    pub require_remote_confirmation: bool,
}

impl AiProjectPolicy {
    pub fn disabled() -> Self {
        Self {
            mode: AiProjectAccessMode::Disabled,
            allowed_remote_providers: BTreeSet::new(),
            allowed_categories: BTreeSet::new(),
            require_remote_confirmation: true,
        }
    }

    pub fn local_only(categories: impl IntoIterator<Item = AiDataCategory>) -> Self {
        Self {
            mode: AiProjectAccessMode::LocalOnly,
            allowed_remote_providers: BTreeSet::new(),
            allowed_categories: categories.into_iter().collect(),
            require_remote_confirmation: false,
        }
    }

    pub fn selected_remote(
        providers: impl IntoIterator<Item = AiProviderKind>,
        categories: impl IntoIterator<Item = AiDataCategory>,
        require_remote_confirmation: bool,
    ) -> Self {
        Self {
            mode: AiProjectAccessMode::SelectedProviders,
            allowed_remote_providers: providers.into_iter().collect(),
            allowed_categories: categories.into_iter().collect(),
            require_remote_confirmation,
        }
    }

    fn allows(
        &self,
        provider: AiProviderKind,
        destination: AiDestination,
        category: AiDataCategory,
    ) -> bool {
        if !self.allowed_categories.contains(&category) {
            return false;
        }
        match (self.mode, destination) {
            (AiProjectAccessMode::Disabled, _) => false,
            (AiProjectAccessMode::LocalOnly, AiDestination::Local) => true,
            (AiProjectAccessMode::LocalOnly, AiDestination::Remote) => false,
            (AiProjectAccessMode::SelectedProviders, AiDestination::Local) => true,
            (AiProjectAccessMode::SelectedProviders, AiDestination::Remote) => {
                self.allowed_remote_providers.contains(&provider)
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiAccessPolicy {
    pub default_policy: AiProjectPolicy,
    pub project_policies: BTreeMap<String, AiProjectPolicy>,
}

impl AiAccessPolicy {
    pub fn new(default_policy: AiProjectPolicy) -> Self {
        Self {
            default_policy,
            project_policies: BTreeMap::new(),
        }
    }

    pub fn set_project_policy(&mut self, project_id: impl Into<String>, policy: AiProjectPolicy) {
        self.project_policies.insert(project_id.into(), policy);
    }

    pub fn policy_for(&self, project_id: Option<&str>) -> &AiProjectPolicy {
        project_id
            .and_then(|id| self.project_policies.get(id))
            .unwrap_or(&self.default_policy)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiContextFragment {
    pub project_id: Option<String>,
    pub source_id: String,
    pub category: AiDataCategory,
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AiDisclosureOmissionReason {
    CategoryDenied,
    ProjectAiDisabled,
    RemoteProviderDenied,
    LocalOnlyProject,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiDisclosureOmission {
    pub source_id: String,
    pub category: AiDataCategory,
    pub reason: AiDisclosureOmissionReason,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiDisclosure {
    pub provider_id: String,
    pub provider_kind: AiProviderKind,
    pub destination: AiDestination,
    pub included: Vec<AiContextFragment>,
    pub omitted: Vec<AiDisclosureOmission>,
    pub requires_confirmation: bool,
}

impl AiDisclosure {
    pub fn included_categories(&self) -> BTreeSet<AiDataCategory> {
        self.included
            .iter()
            .map(|fragment| fragment.category)
            .collect()
    }

    pub fn included_project_ids(&self) -> BTreeSet<String> {
        self.included
            .iter()
            .filter_map(|fragment| fragment.project_id.clone())
            .collect()
    }
}

pub fn provider_destination(config: &AiProviderConfig) -> AiDestination {
    if is_local_endpoint(&config.endpoint) {
        AiDestination::Local
    } else {
        AiDestination::Remote
    }
}

pub fn build_disclosure(
    config: &AiProviderConfig,
    policy: &AiAccessPolicy,
    candidates: impl IntoIterator<Item = AiContextFragment>,
) -> AiDisclosure {
    let destination = provider_destination(config);
    let mut included = Vec::new();
    let mut omitted = Vec::new();
    let mut requires_confirmation = false;

    for fragment in candidates {
        let project_policy = policy.policy_for(fragment.project_id.as_deref());
        if project_policy.allows(config.kind, destination, fragment.category) {
            if destination == AiDestination::Remote && project_policy.require_remote_confirmation {
                requires_confirmation = true;
            }
            included.push(fragment);
            continue;
        }

        let reason = if !project_policy
            .allowed_categories
            .contains(&fragment.category)
        {
            AiDisclosureOmissionReason::CategoryDenied
        } else {
            match (project_policy.mode, destination) {
                (AiProjectAccessMode::Disabled, _) => AiDisclosureOmissionReason::ProjectAiDisabled,
                (AiProjectAccessMode::LocalOnly, AiDestination::Remote) => {
                    AiDisclosureOmissionReason::LocalOnlyProject
                }
                (AiProjectAccessMode::SelectedProviders, AiDestination::Remote) => {
                    AiDisclosureOmissionReason::RemoteProviderDenied
                }
                _ => AiDisclosureOmissionReason::CategoryDenied,
            }
        };
        omitted.push(AiDisclosureOmission {
            source_id: fragment.source_id,
            category: fragment.category,
            reason,
        });
    }

    AiDisclosure {
        provider_id: config.id.clone(),
        provider_kind: config.kind,
        destination,
        included,
        omitted,
        requires_confirmation,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AiInvocationError {
    ConfirmationRequired,
    EmptyAllowedContext,
    Provider(AiProviderError),
}

/// Builds the user-visible disclosure and invokes a provider only after policy
/// filtering. A remote call requiring confirmation is impossible until the
/// caller explicitly supplies `confirmed=true`.
pub fn invoke_with_disclosure(
    provider: &dyn AiProvider,
    policy: &AiAccessPolicy,
    candidates: impl IntoIterator<Item = AiContextFragment>,
    user_instruction: impl Into<String>,
    confirmed: bool,
    resolved_secret: Option<&[u8]>,
) -> Result<(AiDisclosure, AiResponse), AiInvocationError> {
    let disclosure = build_disclosure(provider.config(), policy, candidates);
    if disclosure.requires_confirmation && !confirmed {
        return Err(AiInvocationError::ConfirmationRequired);
    }
    if disclosure.included.is_empty() {
        return Err(AiInvocationError::EmptyAllowedContext);
    }
    let context = disclosure
        .included
        .iter()
        .map(|fragment| {
            format!(
                "[{}:{}]\n{}",
                fragment.category.as_str(),
                fragment.source_id,
                fragment.text
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    let request = AiRequest {
        system: Some(
            "Use only the Orbit context supplied below. Do not assume omitted private context."
                .into(),
        ),
        user: format!(
            "Orbit context:\n{}\n\nUser request:\n{}",
            context,
            user_instruction.into()
        ),
        require_structured_output: false,
    };
    validate_request(provider.config(), &request).map_err(AiInvocationError::Provider)?;
    let response = provider
        .complete(request, resolved_secret)
        .map_err(AiInvocationError::Provider)?;
    Ok((disclosure, response))
}

// ---------------------------------------------------------------------------
// Direct provider HTTP adapter contracts.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiHttpRequest {
    pub method: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiHttpResponse {
    pub status: u16,
    pub body: Vec<u8>,
}

pub trait AiHttpTransport: Send + Sync {
    fn send(&self, request: AiHttpRequest) -> Result<AiHttpResponse, AiProviderError>;
}

pub struct DirectHttpAiProvider<T> {
    config: AiProviderConfig,
    transport: T,
}

impl<T> DirectHttpAiProvider<T> {
    pub fn new(config: AiProviderConfig, transport: T) -> Self {
        Self { config, transport }
    }
}

impl<T: AiHttpTransport> AiProvider for DirectHttpAiProvider<T> {
    fn config(&self) -> &AiProviderConfig {
        &self.config
    }

    fn complete(
        &self,
        request: AiRequest,
        resolved_secret: Option<&[u8]>,
    ) -> Result<AiResponse, AiProviderError> {
        validate_request(&self.config, &request)?;
        let http_request = build_provider_http_request(&self.config, &request, resolved_secret)?;
        let response = self.transport.send(http_request)?;
        if !(200..300).contains(&response.status) {
            return Err(AiProviderError::Transport(format!(
                "provider returned HTTP {}",
                response.status
            )));
        }
        parse_provider_http_response(self.config.kind, &response.body)
    }
}

fn build_provider_http_request(
    config: &AiProviderConfig,
    request: &AiRequest,
    resolved_secret: Option<&[u8]>,
) -> Result<AiHttpRequest, AiProviderError> {
    let mut headers = vec![("content-type".into(), "application/json".into())];
    apply_auth_headers(&mut headers, &config.auth, resolved_secret)?;

    let system = request.system.as_deref().unwrap_or("");
    let body = match config.kind {
        AiProviderKind::OpenAi | AiProviderKind::OpenAiCompatible | AiProviderKind::Ollama => {
            serde_json::json!({
                "model": config.model,
                "messages": [
                    {"role": "system", "content": system},
                    {"role": "user", "content": request.user}
                ],
                "response_format": if request.require_structured_output { serde_json::json!({"type":"json_object"}) } else { serde_json::Value::Null }
            })
        }
        AiProviderKind::Anthropic => {
            headers.push(("anthropic-version".into(), "2023-06-01".into()));
            serde_json::json!({
                "model": config.model,
                "max_tokens": 4096,
                "system": system,
                "messages": [{"role":"user", "content": request.user}]
            })
        }
        AiProviderKind::Gemini => {
            serde_json::json!({
                "systemInstruction": {"parts":[{"text":system}]},
                "contents": [{"role":"user", "parts":[{"text":request.user}]}],
                "generationConfig": if request.require_structured_output { serde_json::json!({"responseMimeType":"application/json"}) } else { serde_json::json!({}) }
            })
        }
    };

    Ok(AiHttpRequest {
        method: "POST".into(),
        url: provider_chat_url(config),
        headers,
        body: serde_json::to_vec(&body)
            .map_err(|error| AiProviderError::InvalidResponse(error.to_string()))?,
    })
}

fn provider_chat_url(config: &AiProviderConfig) -> String {
    let endpoint = config.endpoint.trim_end_matches('/');
    match config.kind {
        AiProviderKind::OpenAi | AiProviderKind::OpenAiCompatible | AiProviderKind::Ollama => {
            if endpoint.ends_with("/chat/completions") {
                endpoint.to_string()
            } else {
                format!("{endpoint}/chat/completions")
            }
        }
        AiProviderKind::Anthropic => {
            if endpoint.ends_with("/v1/messages") {
                endpoint.to_string()
            } else {
                format!("{endpoint}/v1/messages")
            }
        }
        AiProviderKind::Gemini => {
            if endpoint.contains(":generateContent") {
                endpoint.to_string()
            } else {
                format!("{endpoint}/models/{}:generateContent", config.model)
            }
        }
    }
}

fn apply_auth_headers(
    headers: &mut Vec<(String, String)>,
    auth: &AiAuthMode,
    resolved_secret: Option<&[u8]>,
) -> Result<(), AiProviderError> {
    match auth {
        AiAuthMode::None => Ok(()),
        AiAuthMode::Bearer { .. } => {
            let secret = secret_as_header_value(resolved_secret)?;
            headers.push(("authorization".into(), format!("Bearer {secret}")));
            Ok(())
        }
        AiAuthMode::ApiKeyHeader { header, .. } => {
            let secret = secret_as_header_value(resolved_secret)?;
            headers.push((header.clone(), secret));
            Ok(())
        }
    }
}

fn secret_as_header_value(secret: Option<&[u8]>) -> Result<String, AiProviderError> {
    let bytes = secret.ok_or(AiProviderError::MissingSecret)?;
    String::from_utf8(bytes.to_vec()).map_err(|_| {
        AiProviderError::InvalidResponse("secret is not valid UTF-8 header data".into())
    })
}

fn parse_provider_http_response(
    kind: AiProviderKind,
    body: &[u8],
) -> Result<AiResponse, AiProviderError> {
    let value: serde_json::Value = serde_json::from_slice(body)
        .map_err(|error| AiProviderError::InvalidResponse(error.to_string()))?;
    let text = match kind {
        AiProviderKind::OpenAi | AiProviderKind::OpenAiCompatible | AiProviderKind::Ollama => value
            .pointer("/choices/0/message/content")
            .and_then(|value| value.as_str()),
        AiProviderKind::Anthropic => value
            .get("content")
            .and_then(|value| value.as_array())
            .and_then(|items| {
                items
                    .iter()
                    .find_map(|item| item.get("text").and_then(|value| value.as_str()))
            }),
        AiProviderKind::Gemini => value
            .pointer("/candidates/0/content/parts/0/text")
            .and_then(|value| value.as_str()),
    }
    .ok_or_else(|| {
        AiProviderError::InvalidResponse("provider response did not contain text".into())
    })?;
    Ok(AiResponse {
        text: text.to_string(),
    })
}

/// Default native HTTP transport for embedded/CLI Orbit composition. It uses
/// Rustls so cloud-provider HTTPS does not depend on Flutter networking code.
#[derive(Debug, Clone)]
pub struct ReqwestAiHttpTransport {
    client: reqwest::blocking::Client,
}

impl ReqwestAiHttpTransport {
    pub fn new() -> Result<Self, AiProviderError> {
        let client = reqwest::blocking::Client::builder()
            .build()
            .map_err(|error| AiProviderError::Transport(error.to_string()))?;
        Ok(Self { client })
    }
}

impl AiHttpTransport for ReqwestAiHttpTransport {
    fn send(&self, request: AiHttpRequest) -> Result<AiHttpResponse, AiProviderError> {
        let method = reqwest::Method::from_bytes(request.method.as_bytes())
            .map_err(|error| AiProviderError::Transport(error.to_string()))?;
        let mut builder = self.client.request(method, &request.url).body(request.body);
        for (name, value) in request.headers {
            builder = builder.header(name.as_str(), value);
        }
        let response = builder
            .send()
            .map_err(|error| AiProviderError::Transport(error.to_string()))?;
        let status = response.status().as_u16();
        let body = response
            .bytes()
            .map_err(|error| AiProviderError::Transport(error.to_string()))?
            .to_vec();
        Ok(AiHttpResponse { status, body })
    }
}

impl DirectHttpAiProvider<ReqwestAiHttpTransport> {
    pub fn native(config: AiProviderConfig) -> Result<Self, AiProviderError> {
        Ok(Self::new(config, ReqwestAiHttpTransport::new()?))
    }
}

// ---------------------------------------------------------------------------
// Structured planning output. AI may propose; orbit-planner validates.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AiProposalError {
    InvalidJson(String),
    InvalidTaskId(String),
    MissingField(&'static str),
    InvalidField(&'static str),
    PlannerViolations(Vec<orbit_planner::PlanViolation>),
}

pub fn parse_task_plan_proposal(
    text: &str,
) -> Result<orbit_planner::PlanProposal, AiProposalError> {
    let value: serde_json::Value = serde_json::from_str(text)
        .map_err(|error| AiProposalError::InvalidJson(error.to_string()))?;
    let scheduled = value
        .get("scheduled")
        .and_then(|value| value.as_array())
        .ok_or(AiProposalError::MissingField("scheduled"))?;

    let mut proposal = orbit_planner::PlanProposal::default();
    for item in scheduled {
        let task_id = item
            .get("task_id")
            .and_then(|value| value.as_str())
            .ok_or(AiProposalError::MissingField("task_id"))?;
        let start_ms = item
            .get("start_ms")
            .and_then(|value| value.as_i64())
            .ok_or(AiProposalError::MissingField("start_ms"))?;
        let end_ms = item
            .get("end_ms")
            .and_then(|value| value.as_i64())
            .ok_or(AiProposalError::MissingField("end_ms"))?;
        if end_ms <= start_ms {
            return Err(AiProposalError::InvalidField("end_ms"));
        }
        let task_id = orbit_domain::TaskId::new(task_id)
            .map_err(|_| AiProposalError::InvalidTaskId(task_id.to_string()))?;
        proposal.scheduled.push(orbit_planner::ProposedTaskBlock {
            task_id,
            start_ms,
            end_ms,
        });
    }
    Ok(proposal)
}

pub fn validate_ai_task_plan(
    response: &AiResponse,
    window: orbit_planner::PlanningWindow,
    existing_blocks: &[orbit_domain::ScheduleBlock],
    tasks: &[orbit_domain::Task],
    policy: orbit_planner::CapacityPolicy,
) -> Result<orbit_planner::ValidatedPlanProposal, AiProposalError> {
    let proposal = parse_task_plan_proposal(&response.text)?;
    orbit_planner::validate_proposal(window, existing_blocks, tasks, policy, proposal)
        .map_err(AiProposalError::PlannerViolations)
}

/// Produces an AI request that asks only for a proposal document. The response
/// must still pass `validate_ai_task_plan` before any application command may
/// persist schedule state.
pub fn task_planning_request(instruction: impl Into<String>) -> AiRequest {
    AiRequest {
        system: Some(
            "Return JSON only. Propose schedule blocks; do not claim to have modified Orbit. \
             Shape: {\"scheduled\":[{\"task_id\":\"...\",\"start_ms\":0,\"end_ms\":0}]}"
                .into(),
        ),
        user: instruction.into(),
        require_structured_output: true,
    }
}

#[cfg(test)]
mod privacy_and_transport_tests {
    use super::*;
    use std::sync::Mutex;

    fn categories(items: &[AiDataCategory]) -> BTreeSet<AiDataCategory> {
        items.iter().copied().collect()
    }

    fn anthropic_config() -> AiProviderConfig {
        AiProviderConfig::new(
            "anthropic-main",
            AiProviderKind::Anthropic,
            "https://api.anthropic.com",
            "claude-model",
            AiAuthMode::ApiKeyHeader {
                header: "x-api-key".into(),
                secret: AiSecretRef::new("ai", "anthropic-main").unwrap(),
            },
            AiCapabilities {
                chat: true,
                structured_output: true,
                tools: true,
                embeddings: false,
                vision: true,
            },
            false,
        )
        .unwrap()
    }

    #[test]
    fn local_only_project_never_discloses_context_to_remote_provider() {
        let mut policy = AiAccessPolicy::new(AiProjectPolicy::disabled());
        policy.set_project_policy(
            "employer",
            AiProjectPolicy::local_only([
                AiDataCategory::TaskTitles,
                AiDataCategory::ProjectMetadata,
            ]),
        );
        let disclosure = build_disclosure(
            &anthropic_config(),
            &policy,
            [AiContextFragment {
                project_id: Some("employer".into()),
                source_id: "task-1".into(),
                category: AiDataCategory::TaskTitles,
                text: "Private task".into(),
            }],
        );
        assert!(disclosure.included.is_empty());
        assert_eq!(
            disclosure.omitted[0].reason,
            AiDisclosureOmissionReason::LocalOnlyProject
        );
    }

    #[test]
    fn selected_remote_provider_discloses_only_allowed_categories() {
        let mut policy = AiAccessPolicy::new(AiProjectPolicy::disabled());
        policy.set_project_policy(
            "personal",
            AiProjectPolicy::selected_remote(
                [AiProviderKind::Anthropic],
                [
                    AiDataCategory::TaskTitles,
                    AiDataCategory::KnowledgeMetadata,
                ],
                true,
            ),
        );
        let disclosure = build_disclosure(
            &anthropic_config(),
            &policy,
            [
                AiContextFragment {
                    project_id: Some("personal".into()),
                    source_id: "task-1".into(),
                    category: AiDataCategory::TaskTitles,
                    text: "Plan trip".into(),
                },
                AiContextFragment {
                    project_id: Some("personal".into()),
                    source_id: "knowledge-1".into(),
                    category: AiDataCategory::KnowledgeBodies,
                    text: "private body".into(),
                },
            ],
        );
        assert_eq!(disclosure.included.len(), 1);
        assert_eq!(disclosure.included[0].source_id, "task-1");
        assert!(disclosure.requires_confirmation);
        assert_eq!(
            disclosure.omitted[0].reason,
            AiDisclosureOmissionReason::CategoryDenied
        );
    }

    #[derive(Default)]
    struct RecordingTransport {
        requests: Mutex<Vec<AiHttpRequest>>,
        response: Mutex<Option<AiHttpResponse>>,
    }
    impl AiHttpTransport for RecordingTransport {
        fn send(&self, request: AiHttpRequest) -> Result<AiHttpResponse, AiProviderError> {
            self.requests.lock().unwrap().push(request);
            self.response
                .lock()
                .unwrap()
                .take()
                .ok_or_else(|| AiProviderError::Transport("no response".into()))
        }
    }

    #[test]
    fn anthropic_adapter_builds_direct_request_and_parses_response() {
        let transport = RecordingTransport {
            requests: Mutex::new(Vec::new()),
            response: Mutex::new(Some(AiHttpResponse {
                status: 200,
                body: br#"{"content":[{"type":"text","text":"proposal"}]}"#.to_vec(),
            })),
        };
        let provider = DirectHttpAiProvider::new(anthropic_config(), transport);
        let response = provider
            .complete(
                AiRequest {
                    system: Some("system".into()),
                    user: "hello".into(),
                    require_structured_output: false,
                },
                Some(b"secret"),
            )
            .unwrap();
        assert_eq!(response.text, "proposal");
        let requests = provider.transport.requests.lock().unwrap();
        assert_eq!(requests.len(), 1);
        assert!(requests[0].url.ends_with("/v1/messages"));
        assert!(
            requests[0]
                .headers
                .iter()
                .any(|(name, value)| name == "x-api-key" && value == "secret")
        );
    }

    #[test]
    fn remote_disclosure_requires_confirmation_before_provider_invocation() {
        struct PanicProvider {
            config: AiProviderConfig,
        }
        impl AiProvider for PanicProvider {
            fn config(&self) -> &AiProviderConfig {
                &self.config
            }
            fn complete(
                &self,
                _: AiRequest,
                _: Option<&[u8]>,
            ) -> Result<AiResponse, AiProviderError> {
                panic!("provider must not be called")
            }
        }
        let provider = PanicProvider {
            config: anthropic_config(),
        };
        let mut policy = AiAccessPolicy::new(AiProjectPolicy::disabled());
        policy.set_project_policy(
            "p",
            AiProjectPolicy::selected_remote(
                [AiProviderKind::Anthropic],
                [AiDataCategory::TaskTitles],
                true,
            ),
        );
        let result = invoke_with_disclosure(
            &provider,
            &policy,
            [AiContextFragment {
                project_id: Some("p".into()),
                source_id: "t".into(),
                category: AiDataCategory::TaskTitles,
                text: "task".into(),
            }],
            "plan",
            false,
            Some(b"secret"),
        );
        assert_eq!(result, Err(AiInvocationError::ConfirmationRequired));
    }

    #[test]
    fn plan_parser_produces_proposal_not_mutation() {
        let proposal = parse_task_plan_proposal(
            r#"{"scheduled":[{"task_id":"task-1","start_ms":1000,"end_ms":61000}]}"#,
        )
        .unwrap();
        assert_eq!(proposal.scheduled.len(), 1);
        assert_eq!(proposal.scheduled[0].task_id.as_str(), "task-1");
        assert_eq!(proposal.scheduled[0].start_ms, 1000);
    }

    #[test]
    fn category_helper_is_deterministic() {
        assert_eq!(
            categories(&[AiDataCategory::TaskTitles, AiDataCategory::TaskTitles]).len(),
            1
        );
    }
}

// ---- Mode-aware project agent ---------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectAgentPlaybook {
    pub mode: orbit_domain::ProjectMode,
    pub roadmap_focus: Vec<&'static str>,
    pub evidence_focus: Vec<&'static str>,
    pub review_questions: Vec<&'static str>,
}

impl ProjectAgentPlaybook {
    pub fn for_mode(mode: orbit_domain::ProjectMode) -> Self {
        use orbit_domain::ProjectMode::*;
        match mode {
            Standard => Self {
                mode,
                roadmap_focus: vec!["deliverables", "dependencies", "milestones", "risks"],
                evidence_focus: vec!["milestone acceptance", "task outcomes", "blockers"],
                review_questions: vec![
                    "what blocks the outcome?",
                    "is the forecast still credible?",
                ],
            },
            Learning => Self {
                mode,
                roadmap_focus: vec![
                    "curriculum",
                    "practice",
                    "assessment checkpoints",
                    "review cadence",
                ],
                evidence_focus: vec![
                    "mastery evidence",
                    "assessment results",
                    "practice consistency",
                ],
                review_questions: vec![
                    "what can the user demonstrate?",
                    "which weak areas lack evidence?",
                ],
            },
            HabitDriven => Self {
                mode,
                roadmap_focus: vec![
                    "behavior design",
                    "cadence",
                    "environment",
                    "review windows",
                ],
                evidence_focus: vec!["rolling adherence", "consistency", "recovery after misses"],
                review_questions: vec![
                    "is the behavior becoming consistent?",
                    "is the cadence sustainable?",
                ],
            },
            MetricTarget => Self {
                mode,
                roadmap_focus: vec!["baseline", "target", "intermediate checkpoints", "drivers"],
                evidence_focus: vec!["measured value", "trajectory", "forecast"],
                review_questions: vec![
                    "are we on trajectory?",
                    "which driver most affects the target?",
                ],
            },
            Maintenance => Self {
                mode,
                roadmap_focus: vec![
                    "acceptable range",
                    "monitoring cadence",
                    "intervention triggers",
                ],
                evidence_focus: vec!["range adherence", "exceptions", "trend"],
                review_questions: vec![
                    "are we staying inside the acceptable range?",
                    "does anything require intervention?",
                ],
            },
            Custom => Self {
                mode,
                roadmap_focus: vec![
                    "user objective",
                    "selected progress signals",
                    "explicit criteria",
                ],
                evidence_focus: vec!["user-selected evidence"],
                review_questions: vec![
                    "what evidence demonstrates progress?",
                    "what remains unsatisfied?",
                ],
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectRoadmapMilestoneProposal {
    pub title: String,
    pub rationale: String,
    pub success_criteria: Vec<String>,
    pub dependencies: Vec<String>,
    pub suggested_target_at_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectRoadmapProposal {
    pub mode: orbit_domain::ProjectMode,
    pub objective: String,
    pub assumptions: Vec<String>,
    pub project_success_criteria: Vec<String>,
    pub milestones: Vec<ProjectRoadmapMilestoneProposal>,
    pub suggested_habits: Vec<String>,
    pub suggested_metrics: Vec<String>,
    pub risks: Vec<String>,
    pub suggested_review_cadence: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CriterionAssessment {
    pub key: String,
    pub satisfied: bool,
    pub evidence: Vec<String>,
    pub rationale: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectCompletionAssessment {
    pub criteria: Vec<CriterionAssessment>,
    pub ai_recommends_completion: bool,
    pub deterministic_completion_ready: bool,
    pub missing_required_criteria: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectAgentParseError {
    InvalidJson(String),
    MissingField(&'static str),
    InvalidField(&'static str),
    UnknownCriterion(String),
}

pub fn parse_project_roadmap_proposal(
    text: &str,
) -> Result<ProjectRoadmapProposal, ProjectAgentParseError> {
    let value: serde_json::Value = serde_json::from_str(text)
        .map_err(|error| ProjectAgentParseError::InvalidJson(error.to_string()))?;
    let mode_text = value
        .get("mode")
        .and_then(|v| v.as_str())
        .ok_or(ProjectAgentParseError::MissingField("mode"))?;
    let mode = orbit_domain::ProjectMode::parse(mode_text)
        .map_err(|_| ProjectAgentParseError::InvalidField("mode"))?;
    let objective = required_nonempty_string(&value, "objective")?;
    let assumptions = string_array(&value, "assumptions")?;
    let project_success_criteria = string_array(&value, "success_criteria")?;
    if project_success_criteria.is_empty() {
        return Err(ProjectAgentParseError::InvalidField("success_criteria"));
    }
    let milestones_value = value
        .get("milestones")
        .and_then(|v| v.as_array())
        .ok_or(ProjectAgentParseError::MissingField("milestones"))?;
    let mut milestones = Vec::with_capacity(milestones_value.len());
    for milestone in milestones_value {
        let title = required_nonempty_string(milestone, "title")?;
        let rationale = required_nonempty_string(milestone, "rationale")?;
        let success_criteria = string_array(milestone, "success_criteria")?;
        if success_criteria.is_empty() {
            return Err(ProjectAgentParseError::InvalidField(
                "milestones.success_criteria",
            ));
        }
        milestones.push(ProjectRoadmapMilestoneProposal {
            title,
            rationale,
            success_criteria,
            dependencies: string_array(milestone, "dependencies")?,
            suggested_target_at_ms: milestone
                .get("suggested_target_at_ms")
                .and_then(|v| v.as_i64()),
        });
    }
    Ok(ProjectRoadmapProposal {
        mode,
        objective,
        assumptions,
        project_success_criteria,
        milestones,
        suggested_habits: string_array(&value, "habits")?,
        suggested_metrics: string_array(&value, "metrics")?,
        risks: string_array(&value, "risks")?,
        suggested_review_cadence: value
            .get("review_cadence")
            .and_then(|v| v.as_str())
            .map(ToOwned::to_owned),
    })
}

pub fn parse_project_completion_assessment(
    project: &orbit_domain::Project,
    text: &str,
) -> Result<ProjectCompletionAssessment, ProjectAgentParseError> {
    let value: serde_json::Value = serde_json::from_str(text)
        .map_err(|error| ProjectAgentParseError::InvalidJson(error.to_string()))?;
    let ai_recommends_completion = value
        .get("recommend_complete")
        .and_then(|v| v.as_bool())
        .ok_or(ProjectAgentParseError::MissingField("recommend_complete"))?;
    let assessments = value
        .get("criteria")
        .and_then(|v| v.as_array())
        .ok_or(ProjectAgentParseError::MissingField("criteria"))?;
    let mut parsed = Vec::with_capacity(assessments.len());
    for assessment in assessments {
        let key = required_nonempty_string(assessment, "key")?;
        if !project
            .success_criteria()
            .iter()
            .any(|criterion| criterion.key() == key)
        {
            return Err(ProjectAgentParseError::UnknownCriterion(key));
        }
        parsed.push(CriterionAssessment {
            key,
            satisfied: assessment
                .get("satisfied")
                .and_then(|v| v.as_bool())
                .ok_or(ProjectAgentParseError::MissingField("satisfied"))?,
            evidence: string_array(assessment, "evidence")?,
            rationale: required_nonempty_string(assessment, "rationale")?,
        });
    }
    let missing_required_criteria = project
        .success_criteria()
        .iter()
        .filter(|criterion| criterion.required())
        .filter_map(|criterion| {
            let satisfied = parsed
                .iter()
                .find(|assessment| assessment.key == criterion.key())
                .is_some_and(|assessment| assessment.satisfied && !assessment.evidence.is_empty());
            (!satisfied).then(|| criterion.key().to_owned())
        })
        .collect::<Vec<_>>();
    let deterministic_completion_ready = missing_required_criteria.is_empty();
    Ok(ProjectCompletionAssessment {
        criteria: parsed,
        ai_recommends_completion,
        deterministic_completion_ready,
        missing_required_criteria,
    })
}

fn required_nonempty_string(
    value: &serde_json::Value,
    field: &'static str,
) -> Result<String, ProjectAgentParseError> {
    let text = value
        .get(field)
        .and_then(|v| v.as_str())
        .ok_or(ProjectAgentParseError::MissingField(field))?;
    if text.trim().is_empty() {
        return Err(ProjectAgentParseError::InvalidField(field));
    }
    Ok(text.to_owned())
}

fn string_array(
    value: &serde_json::Value,
    field: &'static str,
) -> Result<Vec<String>, ProjectAgentParseError> {
    let Some(array) = value.get(field) else {
        return Ok(Vec::new());
    };
    let array = array
        .as_array()
        .ok_or(ProjectAgentParseError::InvalidField(field))?;
    array
        .iter()
        .map(|item| {
            item.as_str()
                .filter(|text| !text.trim().is_empty())
                .map(ToOwned::to_owned)
                .ok_or(ProjectAgentParseError::InvalidField(field))
        })
        .collect()
}

#[cfg(test)]
mod project_agent_tests {
    use super::*;
    use orbit_domain::{Project, ProjectId, ProjectSuccessCriterion};

    #[test]
    fn learning_playbook_focuses_on_mastery_evidence() {
        let playbook = ProjectAgentPlaybook::for_mode(orbit_domain::ProjectMode::Learning);
        assert!(playbook.evidence_focus.contains(&"mastery evidence"));
    }

    #[test]
    fn completion_assessment_cannot_be_ready_without_required_evidence() {
        let mut project = Project::new(ProjectId::new("p").unwrap(), "Learn").unwrap();
        project
            .replace_success_criteria(vec![
                ProjectSuccessCriterion::new("exam", "Pass assessment", true).unwrap(),
            ])
            .unwrap();
        let assessment = parse_project_completion_assessment(&project, r#"{"recommend_complete":true,"criteria":[{"key":"exam","satisfied":true,"evidence":[],"rationale":"looks good"}]}"#).unwrap();
        assert!(!assessment.deterministic_completion_ready);
        assert_eq!(assessment.missing_required_criteria, vec!["exam"]);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectAgentInvocationError {
    ConfirmationRequired,
    EmptyAllowedContext,
    Provider(AiProviderError),
    InvalidProposal(ProjectAgentParseError),
}

/// Invokes the selected provider through the normal Orbit disclosure boundary
/// and requires a structured roadmap response for the project's mode.
pub fn invoke_project_roadmap_with_disclosure(
    provider: &dyn AiProvider,
    policy: &AiAccessPolicy,
    candidates: impl IntoIterator<Item = AiContextFragment>,
    project: &orbit_domain::Project,
    confirmed: bool,
    resolved_secret: Option<&[u8]>,
) -> Result<(AiDisclosure, ProjectRoadmapProposal), ProjectAgentInvocationError> {
    let disclosure = build_disclosure(provider.config(), policy, candidates);
    if disclosure.requires_confirmation && !confirmed {
        return Err(ProjectAgentInvocationError::ConfirmationRequired);
    }
    if disclosure.included.is_empty() {
        return Err(ProjectAgentInvocationError::EmptyAllowedContext);
    }
    let playbook = ProjectAgentPlaybook::for_mode(project.mode());
    let context = disclosure
        .included
        .iter()
        .map(|fragment| {
            format!(
                "[{}:{}]\n{}",
                fragment.category.as_str(),
                fragment.source_id,
                fragment.text
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    let request = AiRequest {
        system: Some("You are Orbit's project-management agent. Return JSON only. You may propose a roadmap, but you never mutate Orbit state or declare completion outside explicit criteria.".into()),
        user: format!(
            "Project mode: {}\nObjective: {}\nRoadmap focus: {}\nEvidence focus: {}\n\nOrbit context:\n{}\n\nReturn JSON with: mode, objective, assumptions[], success_criteria[], milestones[] (title,rationale,success_criteria[],dependencies[],suggested_target_at_ms), habits[], metrics[], risks[], review_cadence.",
            project.mode().as_str(), project.objective().unwrap_or(project.name()), playbook.roadmap_focus.join(", "), playbook.evidence_focus.join(", "), context
        ),
        require_structured_output: true,
    };
    validate_request(provider.config(), &request).map_err(ProjectAgentInvocationError::Provider)?;
    let response = provider
        .complete(request, resolved_secret)
        .map_err(ProjectAgentInvocationError::Provider)?;
    let proposal = parse_project_roadmap_proposal(&response.text)
        .map_err(ProjectAgentInvocationError::InvalidProposal)?;
    Ok((disclosure, proposal))
}

/// Reviews explicit project completion criteria using only policy-approved
/// context. The model's recommendation is retained, but deterministic readiness
/// is recomputed from required criteria and evidence after parsing.
pub fn invoke_project_completion_review_with_disclosure(
    provider: &dyn AiProvider,
    policy: &AiAccessPolicy,
    candidates: impl IntoIterator<Item = AiContextFragment>,
    project: &orbit_domain::Project,
    confirmed: bool,
    resolved_secret: Option<&[u8]>,
) -> Result<(AiDisclosure, ProjectCompletionAssessment), ProjectAgentInvocationError> {
    let disclosure = build_disclosure(provider.config(), policy, candidates);
    if disclosure.requires_confirmation && !confirmed {
        return Err(ProjectAgentInvocationError::ConfirmationRequired);
    }
    if disclosure.included.is_empty() {
        return Err(ProjectAgentInvocationError::EmptyAllowedContext);
    }
    let context = disclosure
        .included
        .iter()
        .map(|fragment| {
            format!(
                "[{}:{}]\n{}",
                fragment.category.as_str(),
                fragment.source_id,
                fragment.text
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    let criteria = project
        .success_criteria()
        .iter()
        .map(|criterion| {
            format!(
                "{} | required={} | {}",
                criterion.key(),
                criterion.required(),
                criterion.description()
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let request = AiRequest {
        system: Some("You are Orbit's evidence reviewer. Return JSON only. Assess each supplied criterion against evidence in the allowed context. Never invent evidence and never mutate project state.".into()),
        user: format!(
            "Project: {}\nMode: {}\nCriteria:\n{}\n\nAllowed evidence context:\n{}\n\nReturn JSON {{\"recommend_complete\":bool,\"criteria\":[{{\"key\":string,\"satisfied\":bool,\"evidence\":[string],\"rationale\":string}}]}}.",
            project.name(), project.mode().as_str(), criteria, context
        ),
        require_structured_output: true,
    };
    validate_request(provider.config(), &request).map_err(ProjectAgentInvocationError::Provider)?;
    let response = provider
        .complete(request, resolved_secret)
        .map_err(ProjectAgentInvocationError::Provider)?;
    let assessment = parse_project_completion_assessment(project, &response.text)
        .map_err(ProjectAgentInvocationError::InvalidProposal)?;
    Ok((disclosure, assessment))
}
