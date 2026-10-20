"""Shared authenticated HTTP transport for optional Orbit machine workers.

Never follows redirects carrying credentials, never sends credentials to URL
userinfo, and limits decoded response size. Workers own endpoint semantics.
"""
import json
from urllib.parse import urlsplit
from urllib.request import HTTPRedirectHandler, Request, build_opener


class RejectRedirect(HTTPRedirectHandler):
    def redirect_request(self, request, fp, code, msg, headers, newurl):
        raise ValueError("Orbit worker refuses authenticated HTTP redirects")


_OPENER = build_opener(RejectRedirect)


def validate_server(base):
    """Accept HTTPS or explicit loopback HTTP; reject ambiguous base URLs."""
    parsed = urlsplit(base)
    if parsed.scheme != "https" and not (
        parsed.scheme == "http" and parsed.hostname in ("localhost", "127.0.0.1")
    ):
        raise ValueError("HTTPS required except explicit loopback HTTP")
    if (not parsed.hostname or parsed.username is not None or
            parsed.password is not None or parsed.query or parsed.fragment or
            parsed.path not in ("", "/")):
        raise ValueError("invalid authority server base URL")
    return base.rstrip("/")


def json_request(base, token, endpoint, *, method="GET", payload=None,
                 device=None, max_response=65536, timeout=15):
    """Issue a bounded authenticated JSON request without redirect forwarding."""
    base = validate_server(base)
    if not endpoint.startswith("/") or endpoint.startswith("//") or             "#" in endpoint or "\r" in endpoint or "\n" in endpoint:
        raise ValueError("invalid worker endpoint")
    if not token or not isinstance(token, str):
        raise ValueError("missing authority credential")
    if method not in ("GET", "POST"):
        raise ValueError("unsupported worker HTTP method")
    body = None if payload is None else json.dumps(payload, separators=(",", ":")).encode()
    headers = {"Authorization": "Bearer " + token, "Content-Type": "application/json"}
    if device is not None:
        if not isinstance(device, str) or not device.strip() or "\n" in device or "\r" in device:
            raise ValueError("invalid device identity")
        headers["x-orbit-device-id"] = device
    request = Request(base + endpoint, data=body, headers=headers, method=method)
    with _OPENER.open(request, timeout=timeout) as response:
        raw = response.read(max_response + 1)
    if len(raw) > max_response:
        raise ValueError("oversized authority response")
    return json.loads(raw) if raw else None
