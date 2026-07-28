//! The appliance HTTP surface.
//!
//! Everything the dashboard is built on hangs off one piece of shared
//! state: what this appliance is, how far its installation has got, what it
//! is doing, and who is allowed to ask.
//!
//! Access is **denied by default**. Exactly two routes are open — the
//! liveness probe, which reveals nothing, and the login endpoint itself.
//! Every other route requires a session, so a route added later is
//! protected unless someone deliberately places it outside the guard,
//! rather than exposed unless someone remembers to protect it.
//!
//! Three rules bound what may cross this boundary:
//!
//! - **No credentials, ever.** The status surface reports network *shape*
//!   (which SSID, which mode) and never a passphrase, even though the
//!   daemon holds them.
//! - **No raw CSI.** The privacy invariant of the whole system.
//! - **No unbounded verification.** Checking a secret costs an Argon2id
//!   hash — 19 MiB and real CPU time — so attempts are both throttled per
//!   client and serialized process-wide (ADR 0011).

use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, Mutex, MutexGuard};

use axum::extract::{ConnectInfo, Query, Request, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use crate::config::{ApplianceConfig, Uplink};
use crate::credential::AdminCredential;
use crate::journal::{Event, EventKind, Journal, RecordedEvent};
use crate::now_us;
use crate::session::{ABSOLUTE_LIFETIME_US, SessionStore};
use crate::state::{Phase, Readiness, Runtime, RuntimeMode};
use crate::throttle::Throttle;

/// Name of the cookie carrying the session token.
const SESSION_COOKIE: &str = "mf_session";

struct Inner {
    config: ApplianceConfig,
    credential: AdminCredential,
    model_installed: bool,
    runtime: Runtime,
    sessions: SessionStore,
    throttle: Throttle,
}

/// State shared by every handler.
#[derive(Clone)]
pub struct EdgeState {
    inner: Arc<Mutex<Inner>>,
    /// Serializes secret verification.
    ///
    /// One Argon2id hash claims 19 MiB by design. Without this gate a
    /// handful of parallel login attempts would claim that much each and
    /// exhaust a 512 MB appliance — the very cost that makes guessing
    /// expensive would become the way to take the unit down.
    login_gate: Arc<tokio::sync::Mutex<()>>,
    /// The appliance journal, behind its own lock so a database write
    /// never holds up a status request.
    journal: Arc<Mutex<Journal>>,
}

impl EdgeState {
    /// Wraps a loaded configuration and the appliance credential.
    ///
    /// `model_installed` reports whether an active density model artifact
    /// is present on disk.
    #[must_use]
    pub fn new(
        config: ApplianceConfig,
        credential: AdminCredential,
        model_installed: bool,
        journal: Journal,
    ) -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner {
                config,
                credential,
                model_installed,
                runtime: Runtime::new(),
                sessions: SessionStore::default(),
                throttle: Throttle::new(),
            })),
            login_gate: Arc::new(tokio::sync::Mutex::new(())),
            journal: Arc::new(Mutex::new(journal)),
        }
    }

    /// Records an event, reporting rather than propagating a failure.
    ///
    /// An appliance that refused to authenticate anyone because its card
    /// filled up would be worse than one that loses an audit line, so a
    /// journal failure is written to the console and the request carries
    /// on.
    pub fn record(&self, event: Event) {
        let now = now_us();
        if let Err(err) = self.journal().record(event, now) {
            eprintln!("journal: {err}");
        }
    }

    /// Writes everything the journal has buffered.
    pub fn flush_journal(&self) {
        if let Err(err) = self.journal().flush() {
            eprintln!("journal flush: {err}");
        }
    }

    /// Drops events past the retention window or the row cap.
    pub fn prune_journal(&self) {
        let now = now_us();
        if let Err(err) = self.journal().prune(now) {
            eprintln!("journal prune: {err}");
        }
    }

    fn recent_events(&self, limit: usize) -> Vec<RecordedEvent> {
        self.journal().recent(limit).unwrap_or_default()
    }

    fn journal(&self) -> MutexGuard<'_, Journal> {
        match self.journal.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    /// The installation facts, as they stand.
    #[must_use]
    pub fn readiness(&self) -> Readiness {
        let inner = self.lock();
        Readiness::evaluate(&inner.config, inner.model_installed)
    }

    /// What the appliance is doing at the product level.
    #[must_use]
    pub fn phase(&self) -> Phase {
        let inner = self.lock();
        Phase::of(
            Readiness::evaluate(&inner.config, inner.model_installed),
            inner.config.onboarding_completed,
        )
    }

    /// Runs `f` against the stream guard, so callers can start or stop an
    /// activity without the guard escaping the shared lock.
    pub fn with_runtime<T>(&self, f: impl FnOnce(&mut Runtime) -> T) -> T {
        f(&mut self.lock().runtime)
    }

    /// Checks a secret and, if it matches, opens a session.
    ///
    /// Returns the session token, or the wait imposed on this client. The
    /// hash itself runs on a blocking thread behind the login gate: it is
    /// CPU- and memory-bound work that must not stall the async runtime,
    /// and only one may run at a time.
    async fn authenticate(
        &self,
        client: IpAddr,
        presented: String,
    ) -> Result<String, LoginRefusal> {
        let now = now_us();
        if let Err(wait_us) = self.lock().throttle.check(client, now) {
            self.record(Event::new(EventKind::LoginThrottled).from_client(client));
            return Err(LoginRefusal::TooManyAttempts { wait_us });
        }

        let _permit = self.login_gate.lock().await;
        let credential = self.lock().credential.clone();
        let verified = tokio::task::spawn_blocking(move || credential.verify(&presented))
            .await
            .unwrap_or(false);

        let now = now_us();
        let opened = {
            let mut inner = self.lock();
            if verified {
                inner.throttle.record_success(client);
                Ok(inner.sessions.open(now))
            } else {
                Err(inner.throttle.record_failure(client, now))
            }
        };
        match opened {
            Ok(token) => {
                self.record(Event::new(EventKind::LoginSucceeded).from_client(client));
                Ok(token)
            }
            Err(wait_us) => {
                self.record(Event::new(EventKind::LoginFailed).from_client(client));
                Err(LoginRefusal::WrongSecret { wait_us })
            }
        }
    }

    /// Whether `token` names a live session, marking it as just used.
    fn touch_session(&self, token: &str) -> bool {
        let now = now_us();
        self.lock().sessions.touch(token, now)
    }

    /// Ends a session.
    fn close_session(&self, token: &str) {
        self.lock().sessions.close(token);
    }

    fn status(&self) -> StatusResponse {
        let inner = self.lock();
        let readiness = Readiness::evaluate(&inner.config, inner.model_installed);
        let config = &inner.config;
        StatusResponse {
            kit_id: config.identity.kit_id.clone(),
            site_name: config.identity.site_name.clone(),
            phase: Phase::of(readiness, config.onboarding_completed),
            readiness,
            runtime: inner.runtime.mode().clone(),
            model_installed: inner.model_installed,
            sensor_ap: SensorApView {
                ssid: config.network.sensor_ap.ssid.clone(),
                channel: config.network.sensor_ap.channel,
            },
            uplink: UplinkView::of(config.network.uplink.as_ref()),
            nodes: config
                .nodes
                .iter()
                .map(|node| NodeView {
                    node_id: node.node_id.clone(),
                    role: node.role,
                    mac: node.mac.clone(),
                    address: node.address.map(|address| address.to_string()),
                })
                .collect(),
        }
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        // A poisoned lock means a handler panicked mid-read; the state it
        // guards is plain data that stays coherent, so recovering the
        // guard keeps the appliance serving rather than failing every
        // subsequent request.
        match self.inner.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }
}

/// Why a login was refused.
enum LoginRefusal {
    /// The client is still serving a throttling delay.
    TooManyAttempts { wait_us: u64 },
    /// The secret did not match; a delay may now apply.
    WrongSecret { wait_us: u64 },
}

/// Builds the appliance router.
///
/// Protected routes are grouped behind the session guard, so adding a route
/// to that group is enough to protect it.
pub fn router(state: EdgeState) -> Router {
    let protected = Router::new()
        .route("/api/status", get(status))
        .route("/api/events", get(events))
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            require_session,
        ));

    Router::new()
        .route("/health", get(health))
        .route("/api/session", post(login).delete(logout))
        .merge(protected)
        .with_state(state)
}

/// Rejects any request that does not carry a live session.
async fn require_session(State(state): State<EdgeState>, request: Request, next: Next) -> Response {
    let authorized =
        session_token(request.headers()).is_some_and(|token| state.touch_session(&token));
    if authorized {
        next.run(request).await
    } else {
        error_response(StatusCode::UNAUTHORIZED, "authentication required")
    }
}

#[derive(Serialize)]
struct Health {
    status: &'static str,
}

async fn health() -> Json<Health> {
    Json(Health { status: "ok" })
}

async fn status(State(state): State<EdgeState>) -> Json<StatusResponse> {
    Json(state.status())
}

/// How many journal entries `/api/events` returns unless asked otherwise.
const DEFAULT_EVENT_LIMIT: usize = 100;

/// Ceiling on that, so one request cannot ask the appliance to serialize
/// its whole journal.
const MAX_EVENT_LIMIT: usize = 1_000;

#[derive(Deserialize)]
struct EventsQuery {
    limit: Option<usize>,
}

async fn events(
    State(state): State<EdgeState>,
    Query(query): Query<EventsQuery>,
) -> Json<Vec<RecordedEvent>> {
    let limit = query
        .limit
        .unwrap_or(DEFAULT_EVENT_LIMIT)
        .clamp(1, MAX_EVENT_LIMIT);
    Json(state.recent_events(limit))
}

#[derive(Deserialize)]
struct LoginRequest {
    /// The device secret, as printed on the label or carried by the QR
    /// code. Sent in the body, never in the URL: query strings reach
    /// server logs and browser history.
    secret: String,
}

async fn login(
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

async fn logout(
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
fn session_token(headers: &HeaderMap) -> Option<String> {
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
fn session_cookie(token: &str) -> String {
    let max_age = ABSOLUTE_LIFETIME_US / 1_000_000;
    format!("{SESSION_COOKIE}={token}; HttpOnly; SameSite=Strict; Path=/; Max-Age={max_age}")
}

fn cleared_cookie() -> String {
    format!("{SESSION_COOKIE}=; HttpOnly; SameSite=Strict; Path=/; Max-Age=0")
}

#[derive(Serialize)]
struct ErrorResponse {
    error: String,
}

fn error_response(status: StatusCode, message: &str) -> Response {
    (
        status,
        Json(ErrorResponse {
            error: message.to_owned(),
        }),
    )
        .into_response()
}

#[derive(Serialize)]
struct StatusResponse {
    kit_id: String,
    site_name: Option<String>,
    phase: Phase,
    readiness: Readiness,
    runtime: RuntimeMode,
    model_installed: bool,
    sensor_ap: SensorApView,
    uplink: UplinkView,
    nodes: Vec<NodeView>,
}

#[derive(Serialize)]
struct SensorApView {
    ssid: String,
    channel: u8,
}

/// The uplink as the dashboard sees it: its shape, never its secrets.
#[derive(Serialize)]
struct UplinkView {
    mode: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    ssid: Option<String>,
}

impl UplinkView {
    fn of(uplink: Option<&Uplink>) -> Self {
        match uplink {
            None => Self {
                mode: "undecided",
                ssid: None,
            },
            Some(Uplink::Offline) => Self {
                mode: "offline",
                ssid: None,
            },
            Some(Uplink::Wifi { ssid, .. }) => Self {
                mode: "wifi",
                ssid: Some(ssid.clone()),
            },
            Some(Uplink::Ethernet { .. }) => Self {
                mode: "ethernet",
                ssid: None,
            },
        }
    }
}

#[derive(Serialize)]
struct NodeView {
    node_id: String,
    role: flow_core::NodeRole,
    mac: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    address: Option<String>,
}

#[cfg(test)]
mod tests {
    use std::net::Ipv4Addr;

    use flow_core::NodeRole;
    use http_body_util::BodyExt;
    use tower::util::ServiceExt;

    use super::*;
    use crate::config::{Addressing, PairedNode, SiteTuning, WifiSecurity};
    use crate::secret::DeviceSecret;
    use crate::state::Stage;

    const AP_PASSPHRASE: &str = "correct-horse-battery";
    const UPLINK_PASSPHRASE: &str = "campus-secret-value";
    const SECRET: &str = "K7M4-9PQR-2WXY-6BTN-3HFD";
    const WRONG_SECRET: &str = "K7M4-9PQR-2WXY-6BTN-3HFE";

    fn factory() -> ApplianceConfig {
        ApplianceConfig::factory("KIT-0001", "mariam-flow-0001", AP_PASSPHRASE)
    }

    fn installed() -> ApplianceConfig {
        let mut config = factory();
        config.identity.site_name = Some("RU EFREI".into());
        config.nodes = vec![
            PairedNode {
                node_id: "tx-1".into(),
                role: NodeRole::Tx,
                mac: "1a:00:00:00:00:00".into(),
                address: None,
            },
            PairedNode {
                node_id: "rx-1".into(),
                role: NodeRole::Rx,
                mac: "aa:bb:cc:00:00:01".into(),
                address: Some("192.168.4.51".parse().unwrap()),
            },
        ];
        config.network.uplink = Some(Uplink::Wifi {
            ssid: "campus".into(),
            security: WifiSecurity::WpaPersonal {
                passphrase: UPLINK_PASSPHRASE.into(),
            },
            addressing: Addressing::Dhcp,
        });
        config.site = Some(SiteTuning {
            people_per_class: [0.0, 4.0, 12.0, 25.0],
            service_rate_per_min: 6.0,
            smoothing_tau_s: 30.0,
            hysteresis_margin: 0.15,
            min_confidence: 0.5,
            window_us: 5_000_000,
            hop_us: 1_000_000,
        });
        config.onboarding_completed = true;
        config
    }

    fn state_for(config: ApplianceConfig, model_installed: bool) -> EdgeState {
        let secret = DeviceSecret::parse(SECRET).unwrap();
        let credential = AdminCredential::establish(&secret, now_us()).unwrap();
        let journal = Journal::open_in_memory().unwrap();
        EdgeState::new(config, credential, model_installed, journal)
    }

    /// Sends a request, optionally with a cookie and a JSON body, from a
    /// given client address.
    async fn send(
        state: &EdgeState,
        method: &str,
        path: &str,
        cookie: Option<&str>,
        body: Option<String>,
        client: u8,
    ) -> (StatusCode, HeaderMap, serde_json::Value) {
        let mut builder = axum::http::Request::builder().method(method).uri(path);
        if let Some(cookie) = cookie {
            builder = builder.header(header::COOKIE, cookie);
        }
        let body = match body {
            Some(json) => {
                builder = builder.header(header::CONTENT_TYPE, "application/json");
                axum::body::Body::from(json)
            }
            None => axum::body::Body::empty(),
        };
        let mut request = builder.body(body).unwrap();
        // The real server supplies this through `into_make_service_with_
        // connect_info`; the throttle keys on it.
        request
            .extensions_mut()
            .insert(ConnectInfo(SocketAddr::from((
                Ipv4Addr::new(192, 168, 4, client),
                51_000,
            ))));

        let response = router(state.clone()).oneshot(request).await.unwrap();
        let status = response.status();
        let headers = response.headers().clone();
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let value = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
        (status, headers, value)
    }

    async fn login_with(state: &EdgeState, secret: &str, client: u8) -> (StatusCode, HeaderMap) {
        let body = serde_json::json!({ "secret": secret }).to_string();
        let (status, headers, _) =
            send(state, "POST", "/api/session", None, Some(body), client).await;
        (status, headers)
    }

    /// Logs in and returns the cookie to present on later requests.
    async fn session_of(state: &EdgeState) -> String {
        let (status, headers) = login_with(state, SECRET, 10).await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        let set = headers.get(header::SET_COOKIE).unwrap().to_str().unwrap();
        set.split(';').next().unwrap().to_owned()
    }

    #[tokio::test]
    async fn health_needs_no_session() {
        let state = state_for(factory(), false);
        let (status, _, body) = send(&state, "GET", "/health", None, None, 10).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["status"], "ok");
    }

    #[tokio::test]
    async fn the_status_surface_is_closed_to_anonymous_callers() {
        let state = state_for(installed(), true);
        let (status, _, body) = send(&state, "GET", "/api/status", None, None, 10).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_eq!(body["error"], "authentication required");
        assert!(body.get("kit_id").is_none(), "nothing leaks before login");
    }

    #[tokio::test]
    async fn a_forged_or_stale_cookie_is_refused() {
        let state = state_for(installed(), true);
        for cookie in [
            "mf_session=deadbeef",
            "mf_session=",
            "other=value",
            "mf_session",
        ] {
            let (status, _, _) = send(&state, "GET", "/api/status", Some(cookie), None, 10).await;
            assert_eq!(status, StatusCode::UNAUTHORIZED, "cookie {cookie:?}");
        }
    }

    #[tokio::test]
    async fn the_right_secret_opens_a_session_that_unlocks_the_surface() {
        let state = state_for(installed(), true);
        let cookie = session_of(&state).await;

        let (status, _, body) = send(&state, "GET", "/api/status", Some(&cookie), None, 10).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["kit_id"], "KIT-0001");
        assert_eq!(body["phase"]["phase"], "operational");
    }

    #[tokio::test]
    async fn the_session_cookie_is_defended_against_scripts_and_other_sites() {
        let state = state_for(installed(), true);
        let (_, headers) = login_with(&state, SECRET, 10).await;
        let cookie = headers.get(header::SET_COOKIE).unwrap().to_str().unwrap();

        assert!(cookie.contains("HttpOnly"), "unreadable by scripts");
        assert!(cookie.contains("SameSite=Strict"), "no cross-site riding");
        assert!(cookie.contains("Path=/"));
        assert!(
            !cookie.contains(SECRET),
            "the cookie must carry a token, never the secret"
        );
    }

    #[tokio::test]
    async fn the_secret_is_accepted_however_it_was_typed() {
        let state = state_for(installed(), true);
        let (status, _) = login_with(&state, "k7m49pqr2wxy6btn3hfd", 10).await;
        assert_eq!(status, StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn a_wrong_secret_is_refused_without_a_session() {
        let state = state_for(installed(), true);
        let (status, headers) = login_with(&state, WRONG_SECRET, 10).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert!(
            headers.get(header::SET_COOKIE).is_none(),
            "no session handed out"
        );
    }

    #[tokio::test]
    async fn repeated_failures_start_costing_time() {
        let state = state_for(installed(), true);

        // The first few failures are free — mistyping happens.
        for _ in 0..3 {
            let (status, _) = login_with(&state, WRONG_SECRET, 20).await;
            assert_eq!(status, StatusCode::UNAUTHORIZED);
        }
        // Then the client is told to wait.
        let (status, headers) = login_with(&state, WRONG_SECRET, 20).await;
        assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(headers.get(header::RETRY_AFTER).unwrap(), "1");

        // And while blocked, even the correct secret has to wait its turn —
        // otherwise the block would be trivially bypassed.
        let (status, headers) = login_with(&state, SECRET, 20).await;
        assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
        assert!(headers.get(header::SET_COOKIE).is_none());
    }

    #[tokio::test]
    async fn one_client_being_throttled_never_locks_out_another() {
        let state = state_for(installed(), true);
        for _ in 0..8 {
            login_with(&state, WRONG_SECRET, 66).await;
        }
        let (blocked, _) = login_with(&state, SECRET, 66).await;
        assert_eq!(blocked, StatusCode::TOO_MANY_REQUESTS);

        let (installer, _) = login_with(&state, SECRET, 10).await;
        assert_eq!(
            installer,
            StatusCode::NO_CONTENT,
            "the installer must still get in"
        );
    }

    #[tokio::test]
    async fn logging_out_revokes_the_session_and_clears_the_cookie() {
        let state = state_for(installed(), true);
        let cookie = session_of(&state).await;

        let (status, headers, _) =
            send(&state, "DELETE", "/api/session", Some(&cookie), None, 10).await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        let cleared = headers.get(header::SET_COOKIE).unwrap().to_str().unwrap();
        assert!(cleared.contains("Max-Age=0"));

        let (status, _, _) = send(&state, "GET", "/api/status", Some(&cookie), None, 10).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "the token is dead");

        // Logging out again is harmless.
        let (status, _, _) = send(&state, "DELETE", "/api/session", Some(&cookie), None, 10).await;
        assert_eq!(status, StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn sessions_are_independent_of_one_another() {
        let state = state_for(installed(), true);
        let first = session_of(&state).await;
        let second = session_of(&state).await;
        assert_ne!(first, second);

        send(&state, "DELETE", "/api/session", Some(&first), None, 10).await;

        let (status, _, _) = send(&state, "GET", "/api/status", Some(&second), None, 10).await;
        assert_eq!(status, StatusCode::OK, "one logout must not end the others");
    }

    #[tokio::test]
    async fn the_session_cookie_is_found_among_others() {
        let state = state_for(installed(), true);
        let cookie = session_of(&state).await;
        let mixed = format!("theme=dark; {cookie}; lang=fr");

        let (status, _, _) = send(&state, "GET", "/api/status", Some(&mixed), None, 10).await;
        assert_eq!(status, StatusCode::OK);
    }

    #[tokio::test]
    async fn the_status_surface_never_leaks_credentials() {
        let state = state_for(installed(), true);
        let cookie = session_of(&state).await;
        let (_, _, body) = send(&state, "GET", "/api/status", Some(&cookie), None, 10).await;
        let text = body.to_string();

        assert!(!text.contains(AP_PASSPHRASE), "sensor AP passphrase leaked");
        assert!(
            !text.contains(UPLINK_PASSPHRASE),
            "uplink passphrase leaked"
        );
        assert!(!text.contains("passphrase"), "no passphrase field at all");
        assert!(!text.contains("argon2"), "no credential material");
    }

    #[tokio::test]
    async fn a_factory_appliance_reports_the_first_onboarding_step() {
        let state = state_for(factory(), false);
        let cookie = session_of(&state).await;
        let (_, _, body) = send(&state, "GET", "/api/status", Some(&cookie), None, 10).await;

        assert_eq!(body["phase"]["phase"], "onboarding");
        assert_eq!(body["phase"]["stage"], "site");
        assert_eq!(body["uplink"]["mode"], "undecided");
        assert_eq!(body["runtime"]["mode"], "idle");
        assert_eq!(body["sensor_ap"]["channel"], 6);
        assert!(body["nodes"].as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn an_installed_appliance_reports_operational_state() {
        let state = state_for(installed(), true);
        let cookie = session_of(&state).await;
        let (_, _, body) = send(&state, "GET", "/api/status", Some(&cookie), None, 10).await;

        assert_eq!(body["site_name"], "RU EFREI");
        assert_eq!(body["uplink"]["mode"], "wifi");
        assert_eq!(body["uplink"]["ssid"], "campus");
        assert_eq!(body["readiness"]["model_ready"], true);
        assert_eq!(body["nodes"][1]["address"], "192.168.4.51");
    }

    #[tokio::test]
    async fn the_reported_runtime_mode_follows_the_stream_guard() {
        let state = state_for(installed(), true);
        let cookie = session_of(&state).await;
        state
            .with_runtime(|runtime| runtime.start_calibration("s-001"))
            .unwrap();

        let (_, _, body) = send(&state, "GET", "/api/status", Some(&cookie), None, 10).await;
        assert_eq!(body["runtime"]["mode"], "calibrating");
        assert_eq!(body["runtime"]["session_id"], "s-001");

        state.with_runtime(Runtime::stop);
        let (_, _, body) = send(&state, "GET", "/api/status", Some(&cookie), None, 10).await;
        assert_eq!(body["runtime"]["mode"], "idle");
    }

    #[tokio::test]
    async fn the_journal_is_closed_to_anonymous_callers() {
        let state = state_for(installed(), true);
        let (status, _, _) = send(&state, "GET", "/api/events", None, None, 10).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn a_successful_login_is_recorded_with_its_client() {
        let state = state_for(installed(), true);
        let cookie = session_of(&state).await;

        let (status, _, body) = send(&state, "GET", "/api/events", Some(&cookie), None, 10).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body[0]["kind"], "login-succeeded");
        assert_eq!(body[0]["category"], "access");
        assert_eq!(body[0]["client"], "192.168.4.10");
    }

    #[tokio::test]
    async fn failures_and_throttling_are_recorded_too() {
        let state = state_for(installed(), true);
        // Three failures are free, the fourth sets the delay, and only the
        // fifth is actually turned away.
        for _ in 0..5 {
            login_with(&state, WRONG_SECRET, 66).await;
        }
        let cookie = session_of(&state).await;

        let (_, _, body) = send(&state, "GET", "/api/events", Some(&cookie), None, 10).await;
        let kinds: Vec<&str> = body
            .as_array()
            .unwrap()
            .iter()
            .map(|event| event["kind"].as_str().unwrap())
            .collect();

        assert!(kinds.contains(&"login-failed"));
        assert!(
            kinds.contains(&"login-throttled"),
            "a blocked attempt is worth recording: {kinds:?}"
        );
        // The attacker's address is on record, not just the installer's.
        let attackers = body
            .as_array()
            .unwrap()
            .iter()
            .filter(|event| event["client"] == "192.168.4.66")
            .count();
        assert!(attackers >= 5, "every attempt from 192.168.4.66 is kept");
    }

    #[tokio::test]
    async fn logging_out_is_recorded() {
        let state = state_for(installed(), true);
        let cookie = session_of(&state).await;
        send(&state, "DELETE", "/api/session", Some(&cookie), None, 10).await;

        let fresh = session_of(&state).await;
        let (_, _, body) = send(&state, "GET", "/api/events", Some(&fresh), None, 10).await;
        let kinds: Vec<&str> = body
            .as_array()
            .unwrap()
            .iter()
            .map(|event| event["kind"].as_str().unwrap())
            .collect();
        assert!(kinds.contains(&"logged-out"), "{kinds:?}");
    }

    #[tokio::test]
    async fn the_journal_query_honours_and_clamps_its_limit() {
        let state = state_for(installed(), true);
        for _ in 0..3 {
            login_with(&state, WRONG_SECRET, 10).await;
        }
        let cookie = session_of(&state).await;

        let (_, _, body) = send(
            &state,
            "GET",
            "/api/events?limit=2",
            Some(&cookie),
            None,
            10,
        )
        .await;
        assert_eq!(body.as_array().unwrap().len(), 2);

        // An absurd limit is clamped rather than refused.
        let (status, _, body) = send(
            &state,
            "GET",
            "/api/events?limit=999999",
            Some(&cookie),
            None,
            10,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert!(body.as_array().unwrap().len() <= MAX_EVENT_LIMIT);
    }

    #[tokio::test]
    async fn the_journal_never_records_the_secret_itself() {
        let state = state_for(installed(), true);
        login_with(&state, WRONG_SECRET, 10).await;
        let cookie = session_of(&state).await;

        let (_, _, body) = send(&state, "GET", "/api/events", Some(&cookie), None, 10).await;
        let text = body.to_string();
        assert!(
            !text.contains(SECRET),
            "a secret must never reach the journal"
        );
        assert!(!text.contains(WRONG_SECRET), "not even a wrong one");
    }

    #[tokio::test]
    async fn readiness_is_reported_even_when_the_installation_is_closed() {
        let mut config = installed();
        config.nodes.clear();
        let state = state_for(config, true);
        let cookie = session_of(&state).await;

        assert_eq!(state.phase(), Phase::Operational);
        assert_eq!(state.readiness().stage(), Stage::Nodes);

        let (_, _, body) = send(&state, "GET", "/api/status", Some(&cookie), None, 10).await;
        assert_eq!(body["phase"]["phase"], "operational");
        assert_eq!(body["readiness"]["nodes_paired"], false);
    }
}
