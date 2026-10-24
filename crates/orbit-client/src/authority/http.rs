//! Device-authenticated authority HTTP transport.
use super::*;

/// Every JSON request uses one bounded timeout so a hung server can never wedge
/// a synchronous frontend.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);
/// Binary uploads and downloads move up to the attachment limit in one body, so
/// they get a longer, still bounded timeout instead of the interactive JSON
/// request budget. A large attachment download must not fail at 15 s while the
/// same bytes upload successfully.
const TRANSFER_TIMEOUT: Duration = Duration::from_secs(120);

/// One binary response whose HTTP status and integrity headers are part of the
/// contract the caller must validate. Only transport failures are errors.
pub struct BinaryResponse {
    pub status: u16,
    pub content_type: Option<String>,
    pub checksum: Option<String>,
    pub size: Option<u64>,
    pub bytes: Vec<u8>,
}

impl BinaryResponse {
    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.status)
    }
    /// Error text from a JSON `{"error": "..."}` failure body, if present.
    pub fn error_message(&self) -> Option<String> {
        serde_json::from_slice::<Value>(&self.bytes)
            .ok()
            .and_then(|value| {
                value
                    .get("error")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
            })
    }
}

pub struct AuthorityHttp {
    pub(super) base: String,
    token: String,
    device: String,
    client: Client,
    transfer_client: Client,
}
impl AuthorityHttp {
    pub fn new(base: &str, token: &str, device: &str) -> Result<Self, AuthorityError> {
        if device.trim().is_empty()
            || device.len() > 256
            || !device
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | ':'))
        {
            return Err(AuthorityError::Invalid(
                "valid enrolled device ID required".into(),
            ));
        }
        validate_url(base)?;
        if token.is_empty() {
            return Err(AuthorityError::Invalid("missing token".into()));
        }
        let client = Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .redirect(reqwest::redirect::Policy::none())
            .build()?;
        let upload_client = Client::builder()
            .timeout(TRANSFER_TIMEOUT)
            .redirect(reqwest::redirect::Policy::none())
            .build()?;
        Ok(Self {
            base: base.trim_end_matches('/').into(),
            token: token.into(),
            device: device.into(),
            client,
            transfer_client: upload_client,
        })
    }
    /// Authenticated JSON read. A missing resource is reported as a typed
    /// `Invalid` naming the path, so a frontend can explain a 404 instead of
    /// showing a generic "server returned 404 Not Found".
    pub fn get_json(&self, path: &str) -> Result<Value, AuthorityError> {
        Ok(self.read(path)?.json()?)
    }
    /// Authenticated raw-bytes read for future binary projections. The same
    /// bearer/device headers and timeout apply as every other request.
    pub fn get_bytes(&self, path: &str) -> Result<Vec<u8>, AuthorityError> {
        Ok(self.read(path)?.bytes()?.to_vec())
    }
    /// Authenticated binary read that preserves the response status and the
    /// integrity headers (`x-orbit-checksum`, `x-orbit-size`, `content-type`).
    /// Callers must verify those headers against the expected metadata; a
    /// successful transport alone proves nothing about content integrity.
    /// Downloads use the longer transfer budget because attachment content can
    /// be as large as an upload.
    pub fn get_binary(&self, path: &str) -> Result<BinaryResponse, AuthorityError> {
        let response = self
            .transfer_client
            .get(format!("{}{path}", self.base))
            .bearer_auth(&self.token)
            .header("x-orbit-device-id", &self.device)
            .send()?;
        binary_response(response)
    }
    /// Upload raw bytes with a longer bounded timeout. Exactly one
    /// content-type is sent; no multipart framing or client path is involved.
    pub fn post_bytes_with_query(
        &self,
        path: &str,
        content_type: &str,
        body: &[u8],
    ) -> Result<BinaryResponse, AuthorityError> {
        let response = self
            .transfer_client
            .post(format!("{}{path}", self.base))
            .bearer_auth(&self.token)
            .header("x-orbit-device-id", &self.device)
            .header(reqwest::header::CONTENT_TYPE, content_type)
            .body(body.to_vec())
            .send()?;
        binary_response(response)
    }
    pub fn get(&self, path: &str) -> Result<Value, AuthorityError> {
        self.get_json(path)
    }
    pub fn post_json(&self, path: &str, payload: &Value) -> Result<Value, AuthorityError> {
        self.post(path, payload)
    }
    pub fn post(&self, path: &str, payload: &Value) -> Result<Value, AuthorityError> {
        self.send(reqwest::Method::POST, path, Some(payload))
    }
    fn read(&self, path: &str) -> Result<reqwest::blocking::Response, AuthorityError> {
        let response = self.authorized(reqwest::Method::GET, path).send()?;
        let status = response.status();
        if !status.is_success() {
            let body = response.text()?;
            return Err(http_status_error(status, &body, path));
        }
        Ok(response)
    }
    fn send(
        &self,
        method: reqwest::Method,
        path: &str,
        payload: Option<&Value>,
    ) -> Result<Value, AuthorityError> {
        let mut req = self.authorized(method, path);
        if let Some(body) = payload {
            req = req.json(body);
        }
        let response = req.send()?;
        let status = response.status();
        let body = response.text()?;
        if !status.is_success() {
            return Err(http_status_error(status, &body, path));
        }
        serde_json::from_str(&body)
            .map_err(|error| AuthorityError::Invalid(format!("invalid server JSON: {error}")))
    }
    fn authorized(&self, method: reqwest::Method, path: &str) -> reqwest::blocking::RequestBuilder {
        self.client
            .request(method, format!("{}{path}", self.base))
            .bearer_auth(&self.token)
            .header("x-orbit-device-id", &self.device)
    }
}

/// Map one non-success HTTP status onto the typed authority failure the caller
/// must act on.
///
/// The distinction is not cosmetic: [`AuthorityError::Invalid`] is a *domain*
/// rejection. The outbox marks such a row `conflict` and moves on, because
/// retrying the identical request can never succeed. [`AuthorityError::Conflict`]
/// is a revision fence that a refresh can resolve, so the row stays pending.
/// A permanently misrouted command (404) is also a domain rejection.
fn http_status_error(status: reqwest::StatusCode, body: &str, path: &str) -> AuthorityError {
    let body = body.trim();
    let detail = serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|value| {
            value
                .get("error")
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .filter(|message| !message.trim().is_empty())
        .unwrap_or_else(|| body.to_owned());
    match status.as_u16() {
        400 => AuthorityError::Invalid(if detail.is_empty() {
            format!("server rejected the request for {path} (400 Bad Request)")
        } else {
            detail
        }),
        401 | 403 => AuthorityError::Unauthorized(if detail.is_empty() {
            format!("server refused the device credentials for {path} ({status})")
        } else {
            detail
        }),
        404 => AuthorityError::Invalid(format!("{path} not found (server returned 404)")),
        409 => AuthorityError::Conflict(if detail.is_empty() {
            format!("server reported a conflict for {path}")
        } else {
            detail
        }),
        413 => AuthorityError::Invalid(if detail.is_empty() {
            format!("server rejected {path} as too large (413 Payload Too Large)")
        } else {
            detail
        }),
        _ => AuthorityError::Unavailable(format!("server returned {status} for {path}")),
    }
}
fn binary_response(
    response: reqwest::blocking::Response,
) -> Result<BinaryResponse, AuthorityError> {
    let status = response.status().as_u16();
    let header = |name: &str| {
        response
            .headers()
            .get(name)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned)
    };
    let content_type = header("content-type");
    let checksum = header("x-orbit-checksum");
    let size = header("x-orbit-size").and_then(|value| value.parse::<u64>().ok());
    let bytes = response.bytes()?.to_vec();
    Ok(BinaryResponse {
        status,
        content_type,
        checksum,
        size,
        bytes,
    })
}
pub(super) fn validate_url(value: &str) -> Result<(), AuthorityError> {
    let url = reqwest::Url::parse(value).map_err(|e| AuthorityError::Invalid(e.to_string()))?;
    if url.username() != ""
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.path() != "/"
    {
        return Err(AuthorityError::Invalid(
            "authority URL cannot include credentials, path, query or fragment".into(),
        ));
    }
    let loopback = matches!(
        url.host_str(),
        Some("localhost" | "127.0.0.1" | "[::1]" | "::1")
    );
    if url.scheme() != "https" && !(url.scheme() == "http" && loopback) {
        return Err(AuthorityError::Invalid(
            "HTTPS required except on loopback".into(),
        ));
    }
    Ok(())
}
