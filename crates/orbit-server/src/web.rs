//! Static hosting for the optional browser app.
//!
//! The server is API-first: when no build is present the API and the operator
//! console keep working, and only the browser entry point explains how to build
//! it. Files are resolved against one canonical dist root captured at startup.
use super::*;
use axum::{body::Body, response::Response};
use std::path::PathBuf;

const NOT_BUILT: &str =
    "Orbit web app is not built. Run: cd web && npm ci && npm run build (or set ORBIT_WEB_DIST).";
const APP_CSP: &str = "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; \
img-src 'self' data: blob:; font-src 'self' data:; connect-src 'self'; frame-ancestors 'none'; \
base-uri 'none'; form-action 'none'";
const IMMUTABLE_CACHE: &str = "public, max-age=31536000, immutable";
const STATIC_CACHE: &str = "public, max-age=300";

/// Canonical build output directory, or `None` when the build is absent.
#[derive(Debug, Clone)]
pub(super) struct WebDist {
    root: Option<PathBuf>,
}
impl WebDist {
    pub(super) fn from_env() -> Self {
        let configured = std::env::var("ORBIT_WEB_DIST")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("web/dist"));
        Self::new(configured)
    }
    pub(super) fn new(root: PathBuf) -> Self {
        let root = std::fs::canonicalize(&root)
            .ok()
            .filter(|path| path.is_dir());
        Self { root }
    }
}

pub(super) async fn index(State(state): State<AppState>) -> Response {
    serve(&state.web_dist, "").await
}
pub(super) async fn fallback(
    State(state): State<AppState>,
    AxumPath(path): AxumPath<String>,
) -> Response {
    serve(&state.web_dist, &path).await
}

async fn serve(dist: &WebDist, raw_path: &str) -> Response {
    let path = raw_path.trim_start_matches('/');
    // API paths must never receive the app shell: a JSON 404 is truthful and
    // keeps client error handling generic.
    if is_api_path(path) {
        return error(StatusCode::NOT_FOUND, "not found").into_response();
    }
    if let (Some(root), Some(relative)) = (dist.root.as_deref(), relative_path(path)) {
        let candidate = root.join(&relative);
        if let Ok(canonical) = tokio::fs::canonicalize(&candidate).await
            && canonical.starts_with(root)
            && let Ok(metadata) = tokio::fs::metadata(&canonical).await
            && metadata.is_file()
            && let Ok(bytes) = tokio::fs::read(&canonical).await
        {
            return file_response(path, bytes);
        }
    }
    index_response(dist).await
}

fn is_api_path(path: &str) -> bool {
    path == "mcp" || path.starts_with("v1/")
}

/// Reject anything that could escape the dist root before touching the disk.
fn relative_path(raw: &str) -> Option<PathBuf> {
    if raw.is_empty() || raw.starts_with('/') || raw.contains('\\') || raw.contains('\0') {
        return None;
    }
    let mut relative = PathBuf::new();
    for segment in raw.split('/') {
        match segment {
            "" | "." => continue,
            ".." => return None,
            _ => relative.push(segment),
        }
    }
    if relative.as_os_str().is_empty() {
        None
    } else {
        Some(relative)
    }
}

async fn index_response(dist: &WebDist) -> Response {
    let Some(root) = dist.root.as_deref() else {
        return not_built();
    };
    match tokio::fs::read(root.join("index.html")).await {
        Ok(bytes) => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
            .header(header::CACHE_CONTROL, "no-store")
            .header(header::CONTENT_SECURITY_POLICY, APP_CSP)
            .header(header::X_CONTENT_TYPE_OPTIONS, "nosniff")
            .header(header::REFERRER_POLICY, "same-origin")
            .body(Body::from(bytes))
            .unwrap_or_else(|_| not_built()),
        Err(_) => not_built(),
    }
}
fn not_built() -> Response {
    Response::builder()
        .status(StatusCode::SERVICE_UNAVAILABLE)
        .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
        .header(header::CACHE_CONTROL, "no-store")
        .header(header::X_CONTENT_TYPE_OPTIONS, "nosniff")
        .body(Body::from(NOT_BUILT))
        .expect("static response is valid")
}
fn file_response(path: &str, bytes: Vec<u8>) -> Response {
    let cache = if path.starts_with("assets/") {
        IMMUTABLE_CACHE
    } else {
        STATIC_CACHE
    };
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type(path))
        .header(header::CACHE_CONTROL, cache)
        .header(header::X_CONTENT_TYPE_OPTIONS, "nosniff")
        .header(header::REFERRER_POLICY, "same-origin")
        .body(Body::from(bytes))
        .expect("static response is valid")
}
fn content_type(path: &str) -> &'static str {
    let extension = path
        .rsplit('/')
        .next()
        .and_then(|name| name.rsplit_once('.').map(|(_, extension)| extension))
        .unwrap_or_default();
    match extension.to_ascii_lowercase().as_str() {
        "html" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" => "application/json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        "woff2" => "font/woff2",
        "txt" => "text/plain; charset=utf-8",
        "map" => "application/json",
        "wasm" => "application/wasm",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static SEQUENCE: AtomicU64 = AtomicU64::new(0);

    struct TempDist {
        root: PathBuf,
    }
    impl TempDist {
        fn new(index: Option<&str>) -> Self {
            let root = std::env::temp_dir().join(format!(
                "orbit-web-test-{}-{}",
                std::process::id(),
                SEQUENCE.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir_all(root.join("assets")).unwrap();
            if let Some(index) = index {
                std::fs::write(root.join("index.html"), index).unwrap();
            }
            std::fs::write(root.join("assets/app.js"), "console.log(1)").unwrap();
            Self { root }
        }
        fn dist(&self) -> WebDist {
            WebDist::new(self.root.clone())
        }
    }
    impl Drop for TempDist {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    async fn body_of(response: Response) -> String {
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        String::from_utf8_lossy(&bytes).into_owned()
    }

    #[tokio::test]
    async fn root_serves_the_app_shell_with_security_headers() {
        let temp = TempDist::new(Some("<!doctype html><title>Orbit</title>"));
        let response = serve(&temp.dist(), "").await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers()[header::CONTENT_TYPE],
            "text/html; charset=utf-8"
        );
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
        assert!(
            response.headers()[header::CONTENT_SECURITY_POLICY]
                .to_str()
                .unwrap()
                .contains("frame-ancestors 'none'")
        );
        assert_eq!(
            response.headers()[header::X_CONTENT_TYPE_OPTIONS],
            "nosniff"
        );
        assert!(body_of(response).await.contains("Orbit"));
    }

    #[tokio::test]
    async fn client_routes_fall_back_to_the_app_shell() {
        let temp = TempDist::new(Some("<html>shell</html>"));
        assert!(
            body_of(serve(&temp.dist(), "settings/profile").await)
                .await
                .contains("shell")
        );
        // Traversal attempts are not files: they degrade to the shell.
        assert!(
            body_of(serve(&temp.dist(), "../etc/passwd").await)
                .await
                .contains("shell")
        );
    }

    #[tokio::test]
    async fn api_paths_never_receive_the_app_shell() {
        let temp = TempDist::new(Some("<html>shell</html>"));
        for path in ["v1/nope", "mcp"] {
            let response = serve(&temp.dist(), path).await;
            assert_eq!(response.status(), StatusCode::NOT_FOUND);
            assert_eq!(response.headers()[header::CONTENT_TYPE], "application/json");
            assert!(body_of(response).await.contains("not found"));
        }
    }

    #[tokio::test]
    async fn assets_are_immutable_and_typed() {
        let temp = TempDist::new(Some("<html>shell</html>"));
        let response = serve(&temp.dist(), "assets/app.js").await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()[header::CACHE_CONTROL], IMMUTABLE_CACHE);
        assert_eq!(
            response.headers()[header::CONTENT_TYPE],
            "text/javascript; charset=utf-8"
        );
        assert_eq!(body_of(response).await, "console.log(1)");
    }

    #[tokio::test]
    async fn missing_build_reports_how_to_build_it() {
        let temp = TempDist::new(None);
        let response = serve(&temp.dist(), "").await;
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(
            response.headers()[header::CONTENT_TYPE],
            "text/plain; charset=utf-8"
        );
        assert_eq!(body_of(response).await, NOT_BUILT);
        // The API 404 path stays JSON even without a build.
        assert_eq!(
            serve(&WebDist::new(temp.root.join("absent")), "v1/nope")
                .await
                .status(),
            StatusCode::NOT_FOUND
        );
    }

    #[test]
    fn path_normalisation_rejects_escapes() {
        assert_eq!(
            relative_path("assets/app.js").unwrap(),
            PathBuf::from("assets/app.js")
        );
        assert_eq!(relative_path("a//./b").unwrap(), PathBuf::from("a/b"));
        assert!(relative_path("").is_none());
        assert!(relative_path("/etc/passwd").is_none());
        assert!(relative_path("..").is_none());
        assert!(relative_path("a/../../b").is_none());
        assert!(relative_path("a\\b").is_none());
        assert!(relative_path("a\0b").is_none());
    }

    #[test]
    fn content_types_cover_the_build_output() {
        assert_eq!(content_type("index.html"), "text/html; charset=utf-8");
        assert_eq!(
            content_type("assets/app.mjs"),
            "text/javascript; charset=utf-8"
        );
        assert_eq!(content_type("assets/app.css"), "text/css; charset=utf-8");
        assert_eq!(content_type("icon.svg"), "image/svg+xml");
        assert_eq!(content_type("font.woff2"), "font/woff2");
        assert_eq!(content_type("manifest.json"), "application/json");
        assert_eq!(content_type("asset.wasm"), "application/wasm");
        assert_eq!(content_type("LICENSE"), "application/octet-stream");
    }
}
