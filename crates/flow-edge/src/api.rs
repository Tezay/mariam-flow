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
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::sse::{Event as SseEvent, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use tokio_stream::StreamExt;

use crate::config::{ApplianceConfig, Uplink};
use crate::credential::AdminCredential;
use crate::history::{MINUTE_US, MinuteSummary};
use crate::journal::{Event, EventKind, Journal, RecordedEvent};
use crate::now_us;
use crate::pipeline::StreamHealth;
use crate::schedule::{ServiceState, ServiceWindow};
use crate::session::{ABSOLUTE_LIFETIME_US, SessionStore};
use crate::state::{Phase, Readiness, Runtime, RuntimeMode};
use crate::throttle::Throttle;

/// Name of the cookie carrying the session token.
const SESSION_COOKIE: &str = "mf_session";

struct Inner {
    config: ApplianceConfig,
    /// Where the configuration is persisted, so a write can be durable.
    config_path: std::path::PathBuf,
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
    /// What the pipeline thread publishes.
    live: Arc<Live>,
}

/// The live surface, written by the pipeline thread and read by handlers.
///
/// A `watch` channel rather than a queue: a client wants the *current*
/// estimate, not every one ever produced, and a slow reader must never
/// apply back-pressure to sensing.
struct Live {
    estimates: tokio::sync::watch::Sender<Option<flow_infer::WaitEstimate>>,
    health: Mutex<StreamHealth>,
}

impl EdgeState {
    /// Wraps a loaded configuration and the appliance credential.
    ///
    /// `model_installed` reports whether an active density model artifact
    /// is present on disk.
    #[must_use]
    pub fn new(
        config: ApplianceConfig,
        config_path: std::path::PathBuf,
        credential: AdminCredential,
        model_installed: bool,
        journal: Journal,
    ) -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner {
                config,
                config_path,
                credential,
                model_installed,
                runtime: Runtime::new(),
                sessions: SessionStore::default(),
                throttle: Throttle::new(),
            })),
            login_gate: Arc::new(tokio::sync::Mutex::new(())),
            journal: Arc::new(Mutex::new(journal)),
            live: Arc::new(Live {
                estimates: tokio::sync::watch::Sender::new(None),
                health: Mutex::new(StreamHealth::default()),
            }),
        }
    }

    /// Publishes a freshly computed estimate.
    ///
    /// `send_replace` rather than `send`: the latter *fails* when no
    /// receiver is subscribed and then discards the value. Nobody is
    /// subscribed most of the time — receivers only exist while a browser
    /// holds the live stream open — so a plain `send` would leave the
    /// channel empty and the appliance would report no estimate to the
    /// first client that connects.
    pub fn publish_estimate(&self, estimate: flow_infer::WaitEstimate) {
        self.live.estimates.send_replace(Some(estimate));
    }

    /// The most recent estimate, if the pipeline has produced one.
    #[must_use]
    pub fn latest_estimate(&self) -> Option<flow_infer::WaitEstimate> {
        *self.live.estimates.borrow()
    }

    /// A receiver that wakes on every new estimate.
    fn watch_estimates(&self) -> tokio::sync::watch::Receiver<Option<flow_infer::WaitEstimate>> {
        self.live.estimates.subscribe()
    }

    /// Replaces the reported stream health.
    pub fn set_stream_health(&self, health: StreamHealth) {
        *self.lock_health() = health;
    }

    /// Marks the pipeline as running or stopped.
    pub fn set_stream_running(&self, running: bool) {
        self.lock_health().running = running;
    }

    /// How the stream is feeding the pipeline.
    #[must_use]
    pub fn stream_health(&self) -> StreamHealth {
        self.lock_health().clone()
    }

    /// Stores one folded minute, reporting rather than propagating failure.
    pub fn write_minute(&self, summary: &MinuteSummary) {
        if let Err(err) = self.journal().write_minute(summary) {
            eprintln!("journal minute: {err}");
        }
    }

    fn minutes(&self, since_us: u64, limit: usize) -> Vec<MinuteSummary> {
        self.journal().minutes(since_us, limit).unwrap_or_default()
    }

    fn lock_health(&self) -> MutexGuard<'_, StreamHealth> {
        match self.live.health.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
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

    /// Applies a change to the configuration and persists it.
    ///
    /// Validation happens inside `save`, before anything reaches the disk,
    /// and the in-memory copy is only replaced once the write succeeded —
    /// so a rejected change leaves the running appliance exactly as it was.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the new configuration is unusable or cannot be
    /// written.
    pub fn update_config(
        &self,
        change: impl FnOnce(&mut ApplianceConfig),
    ) -> Result<(), crate::error::StoreError> {
        let mut inner = self.lock();
        let mut candidate = inner.config.clone();
        change(&mut candidate);
        candidate.save(&inner.config_path)?;
        inner.config = candidate;
        Ok(())
    }

    /// Whether the site is serving, and when that next changes.
    ///
    /// An appliance with no schedule is always open: hours that have not
    /// been declared must not silence a working installation.
    #[must_use]
    pub fn service_state(&self) -> ServiceState {
        let window = self.lock().config.service.clone();
        let Some(window) = window else {
            return ServiceState {
                open: true,
                changes_at_us: None,
            };
        };
        window.state(now_us()).unwrap_or(ServiceState {
            open: true,
            changes_at_us: None,
        })
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
        // Read before taking the lock: `service_state` takes it too.
        let service = self.service_state();
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
            stream: self.stream_health(),
            service,
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
        .route("/api/live", get(live))
        .route("/api/estimates", get(estimates))
        .route(
            "/api/service-window",
            get(service_window).put(set_service_window),
        )
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            require_session,
        ));

    Router::new()
        .route("/health", get(health))
        .route("/api/session", post(login).delete(logout))
        .merge(protected)
        // Anything the API does not claim is the dashboard: its assets, or
        // one of its client-side routes.
        .fallback(crate::assets::serve)
        .layer(middleware::from_fn(security_headers))
        .with_state(state)
}

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
async fn security_headers(request: Request, next: Next) -> Response {
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

/// What the live stream sends on every tick.
///
/// Estimate and stream health travel together because the screen needs
/// both to say anything useful: a missing estimate means one thing when the
/// nodes are streaming and quite another when they have gone silent.
#[derive(Serialize)]
struct LiveSnapshot {
    /// The current estimate, absent until the first window fills.
    #[serde(skip_serializing_if = "Option::is_none")]
    estimate: Option<EstimateView>,
    /// How the frames are arriving.
    stream: StreamHealth,
    /// Whether the site is serving, and when that next changes.
    service: ServiceState,
    /// Appliance clock, so a browser can judge staleness without trusting
    /// its own — the same reasoning as the labeling page.
    now_us: u64,
}

/// An estimate as the dashboard sees it.
///
/// `reliable` is carried rather than used to hide the value: an operator
/// looking at an administration screen needs to see what the model produced
/// *and* that it is not trustworthy. The public estimate surface is where
/// masking belongs, and it already does it.
#[derive(Serialize)]
struct EstimateView {
    ts_us: u64,
    wait_minutes: f32,
    people: f32,
    level: f32,
    class: String,
    confidence: f32,
    reliable: bool,
}

impl From<flow_infer::WaitEstimate> for EstimateView {
    fn from(estimate: flow_infer::WaitEstimate) -> Self {
        Self {
            ts_us: estimate.ts_us,
            wait_minutes: estimate.wait_minutes,
            people: estimate.people,
            level: estimate.level,
            class: estimate.display_class.to_string(),
            confidence: estimate.confidence,
            reliable: estimate.reliable,
        }
    }
}

impl EdgeState {
    fn live_snapshot(&self) -> LiveSnapshot {
        LiveSnapshot {
            estimate: self.latest_estimate().map(EstimateView::from),
            stream: self.stream_health(),
            service: self.service_state(),
            now_us: now_us(),
        }
    }
}

/// Streams the live state as server-sent events.
///
/// One-way and over plain HTTP, which is all this needs: the browser only
/// listens, and the built-in reconnection of `EventSource` covers a dropped
/// connection without a line of code. Keep-alives stop an idle appliance —
/// one whose queue has not changed — from looking dead to a proxy.
async fn live(
    State(state): State<EdgeState>,
) -> Sse<impl tokio_stream::Stream<Item = Result<SseEvent, std::convert::Infallible>>> {
    let receiver = state.watch_estimates();
    let stream = tokio_stream::wrappers::WatchStream::new(receiver).map(move |_| {
        let event = SseEvent::default()
            .json_data(state.live_snapshot())
            .unwrap_or_else(|_| SseEvent::default().comment("snapshot unavailable"));
        Ok(event)
    });
    Sse::new(stream).keep_alive(KeepAlive::default())
}

/// How far back `/api/estimates` reaches unless asked otherwise.
const DEFAULT_HISTORY_MINUTES: u64 = 60;

/// Ceiling on the minutes one request may ask for, so a single call cannot
/// make the appliance serialize two years of history.
const MAX_HISTORY_MINUTES: u64 = 7 * 24 * 60;

#[derive(Deserialize)]
struct EstimatesQuery {
    minutes: Option<u64>,
}

async fn estimates(
    State(state): State<EdgeState>,
    Query(query): Query<EstimatesQuery>,
) -> Json<Vec<MinuteSummary>> {
    let minutes = query
        .minutes
        .unwrap_or(DEFAULT_HISTORY_MINUTES)
        .clamp(1, MAX_HISTORY_MINUTES);
    let since = now_us().saturating_sub(minutes * MINUTE_US);
    Json(state.minutes(since, usize::try_from(minutes).unwrap_or(usize::MAX)))
}

/// Returns the stored service schedule, or `null` when none is declared.
///
/// The schedule is read here rather than from `/api/status` because only the
/// screen that edits it needs it: every other caller wants the resulting
/// state — open or closed — which the status already carries.
async fn service_window(State(state): State<EdgeState>) -> Json<Option<ServiceWindow>> {
    Json(state.lock().config.service.clone())
}

/// Replaces the service schedule, or clears it.
///
/// The whole schedule is sent at once rather than patched field by field:
/// its parts constrain one another — intervals must not overlap, a closure
/// must not end before it begins — so validating a fragment against a stored
/// remainder would be checking half a thing.
///
/// A `null` body clears the schedule, which returns the appliance to
/// estimating around the clock.
async fn set_service_window(
    State(state): State<EdgeState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Json(window): Json<Option<ServiceWindow>>,
) -> Response {
    if let Some(window) = window.as_ref() {
        if let Err(err) = window.validate() {
            return error_response(StatusCode::BAD_REQUEST, &err.to_string());
        }
    }

    let described = window.as_ref().map_or_else(
        || "service schedule cleared".to_owned(),
        |window| format!("service schedule set ({})", window.timezone),
    );

    match state.update_config(|config| config.service = window) {
        Ok(()) => {
            state.record(
                Event::new(EventKind::ConfigurationChanged)
                    .from_client(peer.ip())
                    .with_detail(described),
            );
            (StatusCode::OK, Json(state.service_state())).into_response()
        }
        // The schedule was already found sound, so what remains is the
        // appliance failing to record it — a read-only card, a full disk. That
        // is not something the caller can correct by sending something else.
        Err(err) => {
            state.record(
                Event::new(EventKind::ConfigurationChanged)
                    .from_client(peer.ip())
                    .with_detail(format!("service schedule rejected: {err}")),
            );
            error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "the appliance could not store the schedule",
            )
        }
    }
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
    stream: StreamHealth,
    service: ServiceState,
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
        // Tests that write configuration supply a real path of their own;
        // the rest never reach the disk.
        EdgeState::new(
            config,
            std::path::PathBuf::from("/nonexistent/appliance.json"),
            credential,
            model_installed,
            journal,
        )
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
    async fn every_response_carries_the_browser_protections() {
        let state = state_for(installed(), true);
        // The probe, a refused request and an authenticated one: the
        // headers must not depend on the outcome.
        let cookie = session_of(&state).await;
        for (path, cookie) in [
            ("/health", None),
            ("/api/status", None),
            ("/api/status", Some(cookie.as_str())),
        ] {
            let (_, headers, _) = send(&state, "GET", path, cookie, None, 10).await;
            assert_eq!(
                headers.get("x-content-type-options").unwrap(),
                "nosniff",
                "{path}"
            );
            assert_eq!(headers.get("x-frame-options").unwrap(), "DENY", "{path}");
            assert_eq!(
                headers.get("referrer-policy").unwrap(),
                "no-referrer",
                "{path}"
            );
        }
    }

    #[tokio::test]
    async fn an_estimate_published_with_nobody_listening_is_still_kept() {
        // The pipeline runs whether or not a browser is watching. A channel
        // that dropped values when unobserved would leave the appliance
        // reporting nothing to the first client to connect.
        let state = state_for(installed(), true);
        assert!(state.latest_estimate().is_none());

        state.publish_estimate(flow_infer::WaitEstimate {
            ts_us: 1_800_000_000_000_000,
            wait_minutes: 4.5,
            people: 12.0,
            level: 1.8,
            display_class: flow_core::DensityClass::Medium,
            confidence: 0.71,
            reliable: true,
        });

        let kept = state.latest_estimate().expect("kept without a subscriber");
        assert!((kept.wait_minutes - 4.5).abs() < 1e-6);

        let cookie = session_of(&state).await;
        let (status, _, body) = send(&state, "GET", "/api/status", Some(&cookie), None, 10).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["stream"]["running"], false, "no pipeline in this test");
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

    /// An appliance whose configuration can actually be written, with the
    /// directory kept alive for the duration of the test.
    fn writable() -> (EdgeState, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("appliance.json");
        let config = installed();
        config.save(&path).unwrap();

        let secret = DeviceSecret::parse(SECRET).unwrap();
        let credential = AdminCredential::establish(&secret, now_us()).unwrap();
        let journal = Journal::open_in_memory().unwrap();
        let state = EdgeState::new(config, path, credential, true, journal);
        (state, dir)
    }

    fn always_open() -> serde_json::Value {
        let day = serde_json::json!([{ "from": "00:00", "to": "23:59" }]);
        serde_json::json!({
            "timezone": "Europe/Paris",
            "weekly": {
                "monday": day, "tuesday": day, "wednesday": day, "thursday": day,
                "friday": day, "saturday": day, "sunday": day,
            },
            "closures": [],
        })
    }

    #[tokio::test]
    async fn a_schedule_is_stored_and_answered_with_the_resulting_state() {
        let (state, _dir) = writable();
        let cookie = session_of(&state).await;
        let body = always_open().to_string();

        let (status, _, answer) = send(
            &state,
            "PUT",
            "/api/service-window",
            Some(&cookie),
            Some(body),
            10,
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(answer["open"], true);
        // Reloaded from disk, not from the copy held in memory: the point of
        // the write is that it survives a restart.
        let path = state.lock().config_path.clone();
        let stored = ApplianceConfig::load(&path).unwrap();
        assert_eq!(stored.service.unwrap().timezone, "Europe/Paris");
    }

    #[tokio::test]
    async fn the_stored_schedule_can_be_read_back_for_editing() {
        let (state, _dir) = writable();
        let cookie = session_of(&state).await;

        // Nothing declared yet: the screen that edits hours has to tell an
        // appliance with no schedule from one it failed to read.
        let (status, _, body) = send(
            &state,
            "GET",
            "/api/service-window",
            Some(&cookie),
            None,
            10,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert!(body.is_null());

        send(
            &state,
            "PUT",
            "/api/service-window",
            Some(&cookie),
            Some(always_open().to_string()),
            10,
        )
        .await;

        let (status, _, body) = send(
            &state,
            "GET",
            "/api/service-window",
            Some(&cookie),
            None,
            10,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["timezone"], "Europe/Paris");
        assert_eq!(body["weekly"]["monday"][0]["from"], "00:00");
    }

    #[tokio::test]
    async fn the_schedule_cannot_be_read_without_a_session() {
        let (state, _dir) = writable();
        let (status, _, _) = send(&state, "GET", "/api/service-window", None, None, 10).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn clearing_the_schedule_returns_the_appliance_to_estimating() {
        let (state, _dir) = writable();
        let cookie = session_of(&state).await;
        let body = always_open().to_string();
        send(
            &state,
            "PUT",
            "/api/service-window",
            Some(&cookie),
            Some(body),
            10,
        )
        .await;

        let (status, _, answer) = send(
            &state,
            "PUT",
            "/api/service-window",
            Some(&cookie),
            Some("null".to_owned()),
            10,
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(answer["open"], true);
        let path = state.lock().config_path.clone();
        assert!(ApplianceConfig::load(&path).unwrap().service.is_none());
    }

    #[tokio::test]
    async fn a_refusal_says_what_is_wrong_and_names_no_server_path() {
        let (state, _dir) = writable();
        let cookie = session_of(&state).await;
        let overlapping = serde_json::json!({
            "timezone": "Europe/Paris",
            "weekly": { "monday": [
                { "from": "08:00", "to": "12:00" },
                { "from": "11:00", "to": "14:00" },
            ] },
            "closures": [],
        })
        .to_string();

        let (status, _, body) = send(
            &state,
            "PUT",
            "/api/service-window",
            Some(&cookie),
            Some(overlapping),
            10,
        )
        .await;

        assert_eq!(status, StatusCode::BAD_REQUEST);
        let message = body["error"].as_str().unwrap();
        // Someone is typing opening hours into a form: the answer has to name
        // the day that is wrong. Reporting that the configuration as a whole
        // was refused — and naming the file it was refused for — tells them
        // nothing and hands out a server path.
        assert!(message.contains("monday"), "unhelpful message: {message}");
        assert!(!message.contains('/'), "leaks a path: {message}");
    }

    #[tokio::test]
    async fn a_refused_schedule_leaves_the_stored_one_untouched() {
        let (state, _dir) = writable();
        let cookie = session_of(&state).await;
        let body = always_open().to_string();
        send(
            &state,
            "PUT",
            "/api/service-window",
            Some(&cookie),
            Some(body),
            10,
        )
        .await;

        let unknown_zone =
            serde_json::json!({ "timezone": "Mars/Olympus", "weekly": {}, "closures": [] })
                .to_string();
        let (status, _, _) = send(
            &state,
            "PUT",
            "/api/service-window",
            Some(&cookie),
            Some(unknown_zone),
            10,
        )
        .await;

        assert_eq!(status, StatusCode::BAD_REQUEST);
        let path = state.lock().config_path.clone();
        let stored = ApplianceConfig::load(&path).unwrap();
        assert_eq!(stored.service.unwrap().timezone, "Europe/Paris");
    }

    #[tokio::test]
    async fn the_schedule_cannot_be_set_without_a_session() {
        let (state, _dir) = writable();
        let body = always_open().to_string();

        let (status, _, _) = send(&state, "PUT", "/api/service-window", None, Some(body), 10).await;

        assert_eq!(status, StatusCode::UNAUTHORIZED);
        let path = state.lock().config_path.clone();
        assert!(ApplianceConfig::load(&path).unwrap().service.is_none());
    }
}
