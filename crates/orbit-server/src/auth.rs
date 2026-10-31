//! Authentication mode selection and the principal abstraction it feeds.
//!
//! `authenticated` is the historical behaviour: every request carries an
//! enrolled device credential (bearer token or browser session cookie).
//! `local-trusted` is an explicit opt-in for a single-user machine: requests
//! without a bearer credential are attributed to a synthetic local principal.
//! The mode is never inferred from the peer address; it is configuration.

use axum::http::{HeaderMap, header};
use orbit_runtime::RESERVED_LOCAL_PRINCIPAL_ID;
use std::net::{IpAddr, Ipv6Addr, SocketAddr};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AuthenticationMode {
    Authenticated,
    LocalTrusted,
}

impl AuthenticationMode {
    pub(crate) fn parse(raw: Option<&str>) -> Result<Self, String> {
        match raw.map(str::trim) {
            None | Some("") | Some("authenticated") => Ok(Self::Authenticated),
            Some("local-trusted") => Ok(Self::LocalTrusted),
            Some(other) => Err(format!(
                "invalid authentication mode {other:?}; expected \"authenticated\" or \"local-trusted\""
            )),
        }
    }

    pub(crate) fn from_env() -> Result<Self, String> {
        let raw = std::env::var("ORBIT_AUTH_MODE").ok();
        Self::parse(raw.as_deref())
    }

    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            Self::Authenticated => "authenticated",
            Self::LocalTrusted => "local-trusted",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PrincipalKind {
    Device,
    LocalTrusted,
}

#[derive(Clone, Debug)]
pub(crate) struct Principal {
    device_id: String,
    kind: PrincipalKind,
}

impl Principal {
    pub(crate) fn device(id: String) -> Self {
        Self {
            device_id: id,
            kind: PrincipalKind::Device,
        }
    }

    pub(crate) fn local_trusted() -> Self {
        Self {
            device_id: LOCAL_TRUSTED_PRINCIPAL_ID.to_owned(),
            kind: PrincipalKind::LocalTrusted,
        }
    }

    pub(crate) fn device_id(&self) -> &str {
        &self.device_id
    }

    pub(crate) fn is_local_trusted(&self) -> bool {
        self.kind == PrincipalKind::LocalTrusted
    }
}

pub(crate) const LOCAL_TRUSTED_PRINCIPAL_ID: &str = RESERVED_LOCAL_PRINCIPAL_ID;

/// `local-trusted` only ever authenticates the machine it runs on, so the
/// listener must be bound to a loopback address. A literal `SocketAddr` is
/// required: hostname resolution could produce a non-loopback address and is
/// deliberately not attempted. IPv4-mapped IPv6 (`::ffff:127.0.0.1`) is not
/// `is_loopback()` and is rejected, matching the kernel's dual-stack behaviour.
pub(crate) fn validate_local_trusted_bind(bind: &str) -> Result<SocketAddr, String> {
    let trimmed = bind.trim();
    let address = trimmed.parse::<SocketAddr>().map_err(|_| {
        format!(
            "local-trusted mode requires a loopback bind address (127.0.0.0/8 or ::1) with an explicit port, got {bind:?}"
        )
    })?;
    if !address.ip().is_loopback() {
        return Err(format!(
            "local-trusted mode requires a loopback bind address (127.0.0.0/8 or ::1), got {bind:?}"
        ));
    }
    Ok(address)
}

/// Browsers send `Origin` on cross-origin-capable requests; same-origin
/// requests may omit it. When present, its authority must equal the request's
/// `Host` header, otherwise the request was aimed at another origin.
pub(crate) fn origin_matches_host(headers: &HeaderMap) -> bool {
    let Some(origin) = headers.get(header::ORIGIN) else {
        return true;
    };
    let Ok(origin) = origin.to_str() else {
        return false;
    };
    let Some((scheme, authority)) = origin.split_once("://") else {
        return false;
    };
    if scheme.is_empty() {
        return false;
    }
    let authority = authority.split('/').next().unwrap_or_default();
    if authority.is_empty() {
        return false;
    }
    headers
        .get(header::HOST)
        .and_then(|host| host.to_str().ok())
        .is_some_and(|host| authority.eq_ignore_ascii_case(host.trim()))
}

/// DNS-rebinding defence for local-trusted mode. The `Host` header is the only
/// name the browser and server can compare, so only loopback literals and
/// `localhost`/`*.localhost` are accepted. A public name that resolves to
/// 127.0.0.1 (for example `127.0.0.1.nip.io`) must be refused.
///
/// This is not a proxy filter: a reverse proxy that rewrites `Host` to the
/// upstream address (the nginx default) passes it. Local-trusted mode must not
/// be placed behind a proxy at all, which is stated in its documentation.
pub(crate) fn local_trusted_host_allowed(headers: &HeaderMap) -> bool {
    // Duplicate Host headers are an ambiguity/smuggling signal, not a name.
    if headers.get_all(header::HOST).iter().count() != 1 {
        return false;
    }
    let Some(value) = headers.get(header::HOST) else {
        return false;
    };
    let Ok(host) = value.to_str() else {
        return false;
    };
    let host = host.trim();
    if host.is_empty() {
        return false;
    }
    if host.starts_with('[') {
        // Bracketed IPv6: everything after `]` must be an optional numeric port.
        let Some(end) = host.find(']') else {
            return false;
        };
        let inner = &host[1..end];
        if inner.is_empty() {
            return false;
        }
        let remainder = &host[end + 1..];
        if !remainder.is_empty() {
            let Some(port) = remainder.strip_prefix(':') else {
                return false;
            };
            if port.is_empty() || !port.bytes().all(|byte| byte.is_ascii_digit()) {
                return false;
            }
        }
        inner
            .parse::<Ipv6Addr>()
            .map(|address| address.is_loopback())
            .unwrap_or(false)
    } else {
        // Unbracketed names may carry a numeric port, but only as the last
        // colon-delimited component. Anything else (`:port`, `@`, extra colons)
        // is rejected.
        let (authority, port) = match host.rfind(':') {
            Some(index) => {
                let (authority, port) = host.split_at(index);
                (authority, Some(&port[1..]))
            }
            None => (host, None),
        };
        if let Some(port) = port
            && (port.is_empty() || !port.bytes().all(|byte| byte.is_ascii_digit()))
        {
            return false;
        }
        if authority.is_empty()
            || !authority
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-'))
        {
            return false;
        }
        if authority.eq_ignore_ascii_case("localhost")
            || authority.to_ascii_lowercase().ends_with(".localhost")
        {
            return true;
        }
        match authority.parse::<IpAddr>() {
            Ok(IpAddr::V4(address)) => address.octets()[0] == 127,
            // Unbracketed IPv6 is not a valid Host authority here.
            Ok(IpAddr::V6(_)) => false,
            Err(_) => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn authentication_mode_parse_matrix() {
        assert_eq!(
            AuthenticationMode::parse(None).unwrap(),
            AuthenticationMode::Authenticated
        );
        assert_eq!(
            AuthenticationMode::parse(Some("")).unwrap(),
            AuthenticationMode::Authenticated
        );
        assert_eq!(
            AuthenticationMode::parse(Some("authenticated")).unwrap(),
            AuthenticationMode::Authenticated
        );
        assert_eq!(
            AuthenticationMode::parse(Some("local-trusted")).unwrap(),
            AuthenticationMode::LocalTrusted
        );
        assert_eq!(
            AuthenticationMode::parse(Some("  local-trusted  ")).unwrap(),
            AuthenticationMode::LocalTrusted
        );
        assert_eq!(
            AuthenticationMode::parse(Some("   ")).unwrap(),
            AuthenticationMode::Authenticated
        );
        assert!(AuthenticationMode::parse(Some("LOCAL-TRUSTED")).is_err());
        assert!(AuthenticationMode::parse(Some("trusted")).is_err());
    }

    #[test]
    fn authentication_mode_names_are_stable() {
        assert_eq!(AuthenticationMode::Authenticated.as_str(), "authenticated");
        assert_eq!(AuthenticationMode::LocalTrusted.as_str(), "local-trusted");
    }

    #[test]
    fn local_trusted_bind_accepts_loopback_only() {
        for accepted in [
            "127.0.0.1:8787",
            "127.0.0.2:8787",
            "[::1]:8787",
            "[::1]:0",
            " 127.0.0.1:8787 ",
        ] {
            assert!(
                validate_local_trusted_bind(accepted).is_ok(),
                "{accepted} should be accepted"
            );
        }
        for rejected in [
            "0.0.0.0:8787",
            "[::]:8787",
            "192.168.1.5:8787",
            "[::ffff:127.0.0.1]:8787",
            "localhost:8787",
            "127.0.0.1",
            "",
        ] {
            let message = validate_local_trusted_bind(rejected)
                .expect_err("non-loopback or non-literal bind must be rejected");
            assert!(
                message.contains("loopback"),
                "rejection should name loopback: {message}"
            );
            assert!(
                message.contains(rejected),
                "rejection should echo the offending bind: {message}"
            );
        }
    }

    #[test]
    fn local_trusted_host_accepts_only_loopback_names() {
        for accepted in [
            "127.0.0.1:8787",
            "127.0.0.2:8787",
            "localhost:8787",
            "LOCALHOST:8787",
            "app.localhost:8787",
            "[::1]:8787",
            "[::1]",
        ] {
            let mut headers = HeaderMap::new();
            headers.insert(header::HOST, accepted.parse().unwrap());
            assert!(
                local_trusted_host_allowed(&headers),
                "{accepted} should be accepted"
            );
        }
        for rejected in [
            "orbit.example.com",
            "127.0.0.1.nip.io:8787",
            "192.168.1.5:8787",
            "[::]:8787",
            "0.0.0.0:8787",
            "",
            "[::1]evil.com",
            "127.0.0.1:8787@evil.com",
            "127.0.0.1:port",
            "127.0.0.1:",
            "evil@127.0.0.1",
        ] {
            let mut headers = HeaderMap::new();
            headers.insert(header::HOST, rejected.parse().unwrap());
            assert!(
                !local_trusted_host_allowed(&headers),
                "{rejected} should be rejected"
            );
        }
        assert!(!local_trusted_host_allowed(&HeaderMap::new()));

        let mut duplicate = HeaderMap::new();
        duplicate.append(header::HOST, "127.0.0.1:8787".parse().unwrap());
        duplicate.append(header::HOST, "127.0.0.1:8787".parse().unwrap());
        assert!(!local_trusted_host_allowed(&duplicate));
    }

    #[test]
    fn origin_must_match_host() {
        let mut same = HeaderMap::new();
        same.insert(header::ORIGIN, "http://127.0.0.1:8787".parse().unwrap());
        same.insert(header::HOST, "127.0.0.1:8787".parse().unwrap());
        assert!(origin_matches_host(&same));

        let mut case_insensitive = HeaderMap::new();
        case_insensitive.insert(header::ORIGIN, "https://Orbit.Test".parse().unwrap());
        case_insensitive.insert(header::HOST, "orbit.test".parse().unwrap());
        assert!(origin_matches_host(&case_insensitive));

        let mut foreign = HeaderMap::new();
        foreign.insert(header::ORIGIN, "https://evil.example".parse().unwrap());
        foreign.insert(header::HOST, "orbit.test".parse().unwrap());
        assert!(!origin_matches_host(&foreign));

        let mut missing = HeaderMap::new();
        missing.insert(header::HOST, "orbit.test".parse().unwrap());
        assert!(origin_matches_host(&missing));

        let mut malformed = HeaderMap::new();
        malformed.insert(header::ORIGIN, "null".parse().unwrap());
        malformed.insert(header::HOST, "orbit.test".parse().unwrap());
        assert!(!origin_matches_host(&malformed));

        let mut empty_origin = HeaderMap::new();
        empty_origin.insert(header::ORIGIN, "https://".parse().unwrap());
        empty_origin.insert(header::HOST, "orbit.test".parse().unwrap());
        assert!(!origin_matches_host(&empty_origin));
    }
}
