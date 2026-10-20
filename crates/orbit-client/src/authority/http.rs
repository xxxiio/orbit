//! Device-authenticated authority HTTP transport.
use super::*;

pub struct AuthorityHttp {
    pub(super) base: String,
    token: String,
    device: String,
    client: Client,
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
            .timeout(Duration::from_secs(15))
            .redirect(reqwest::redirect::Policy::none())
            .build()?;
        Ok(Self {
            base: base.trim_end_matches('/').into(),
            token: token.into(),
            device: device.into(),
            client,
        })
    }
    pub fn get(&self, path: &str) -> Result<Value, AuthorityError> {
        self.send(reqwest::Method::GET, path, None)
    }
    pub fn post(&self, path: &str, payload: &Value) -> Result<Value, AuthorityError> {
        self.send(reqwest::Method::POST, path, Some(payload))
    }
    fn send(
        &self,
        method: reqwest::Method,
        path: &str,
        payload: Option<&Value>,
    ) -> Result<Value, AuthorityError> {
        let mut req = self
            .client
            .request(method, format!("{}{path}", self.base))
            .bearer_auth(&self.token)
            .header("x-orbit-device-id", &self.device);
        if let Some(body) = payload {
            req = req.json(body);
        }
        let response = req.send()?;
        if response.status() == reqwest::StatusCode::CONFLICT {
            return Err(AuthorityError::Conflict(response.text()?));
        }
        if !response.status().is_success() {
            return Err(AuthorityError::Unavailable(format!(
                "server returned {}",
                response.status()
            )));
        }
        Ok(response.json()?)
    }
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
