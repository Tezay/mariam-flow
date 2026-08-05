//! Proving who is asking, and keeping the cost of asking bounded.

use std::net::SocketAddr;

use axum::Json;
use axum::extract::{ConnectInfo, Request, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use serde::Deserialize;

use super::{ErrorResponse, error_response};
use crate::edge_state::{EdgeState, LoginRefusal};
use crate::journal::{Event, EventKind};
use crate::session::ABSOLUTE_LIFETIME_US;

/// Name of the cookie carrying the session token.
pub(super) const SESSION_COOKIE: &str = "mf_session";

/// Adds the protections a browser applies on the server's word alone.
///
/// The content security policy itself is not here: only the build knows the
/// hash of the script SvelteKit inlines, so the policy travels in the
/// document (see `dashboard/svelte.config.js`). What a document cannot
/// carry is sent here instead.
///
/// - `nosniff` stops a browser from second-guessing a declared content
///   type, which is how a JSON response gets executed as script.
/// - `DENY` refuses framing outright: nothing should ever embed an
///   administration interface, and this is the header form of
///   `frame-ancestors`, which a meta policy cannot express.
/// - `no-referrer` keeps the appliance's address out of any request the
///   browser makes elsewhere. The QR code already keeps the secret in the
///   URL fragment, which is never sent anywhere; this covers the path.
pub(super) async fn security_headers(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
    headers.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    response
}

/// Rejects any request that does not carry a live session.
pub(super) async fn require_session(
    State(state): State<EdgeState>,
    request: Request,
    next: Next,
) -> Response {
    let authorized =
        session_token(request.headers()).is_some_and(|token| state.touch_session(&token));
    if authorized {
        next.run(request).await
    } else {
        error_response(StatusCode::UNAUTHORIZED, "authentication required")
    }
}

#[derive(Deserialize)]
pub(super) struct LoginRequest {
    /// The device secret, as printed on the label or carried by the QR
    /// code. Sent in the body, never in the URL: query strings reach
    /// server logs and browser history.
    secret: String,
}

pub(super) async fn login(
    State(state): State<EdgeState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Json(request): Json<LoginRequest>,
) -> Response {
    match state.authenticate(peer.ip(), request.secret).await {
        Ok(token) => (
            StatusCode::NO_CONTENT,
            [(header::SET_COOKIE, session_cookie(&token))],
        )
            .into_response(),
        // Both refusals answer the same way, so the response never
        // distinguishes "wrong secret" from "wrong secret, and you are now
        // being slowed down" in a way that helps an attacker calibrate.
        Err(LoginRefusal::TooManyAttempts { wait_us } | LoginRefusal::WrongSecret { wait_us }) => {
            let seconds = wait_us.div_ceil(1_000_000);
            let status = if wait_us > 0 {
                StatusCode::TOO_MANY_REQUESTS
            } else {
                StatusCode::UNAUTHORIZED
            };
            (
                status,
                [(header::RETRY_AFTER, seconds.to_string())],
                Json(ErrorResponse {
                    error: "authentication failed".into(),
                }),
            )
                .into_response()
        }
    }
}

pub(super) async fn logout(
    State(state): State<EdgeState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> Response {
    if let Some(token) = session_token(&headers) {
        state.close_session(&token);
        state.record(Event::new(EventKind::LoggedOut).from_client(peer.ip()));
    }
    // Answering the same way whether or not a session existed keeps logout
    // idempotent and free of information.
    (
        StatusCode::NO_CONTENT,
        [(header::SET_COOKIE, cleared_cookie())],
    )
        .into_response()
}

/// Reads the session token out of the `Cookie` header.
pub(super) fn session_token(headers: &HeaderMap) -> Option<String> {
    let raw = headers.get(header::COOKIE)?.to_str().ok()?;
    raw.split(';')
        .filter_map(|pair| pair.trim().split_once('='))
        .find(|(name, _)| *name == SESSION_COOKIE)
        .map(|(_, value)| value.trim().to_owned())
}

/// The cookie a successful login sets.
///
/// `HttpOnly` keeps the token out of reach of scripts, so a cross-site
/// scripting flaw in the dashboard cannot read it. `SameSite=Strict` stops
/// another site from riding the session with a forged request.
///
/// `Secure` is deliberately absent: on the sensor access point the
/// dashboard is served over plain HTTP — the captive portal requires it —
/// and a `Secure` cookie would simply never be sent there. That traffic is
/// already encrypted by the access point's own WPA2. The HTTPS listener for
/// the site network will set it on its own cookies.
pub(super) fn session_cookie(token: &str) -> String {
    let max_age = ABSOLUTE_LIFETIME_US / 1_000_000;
    format!("{SESSION_COOKIE}={token}; HttpOnly; SameSite=Strict; Path=/; Max-Age={max_age}")
}

pub(super) fn cleared_cookie() -> String {
    format!("{SESSION_COOKIE}=; HttpOnly; SameSite=Strict; Path=/; Max-Age=0")
}
