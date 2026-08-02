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
use axum::routing::{get, post, put};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use tokio_stream::StreamExt;

use crate::calibration::{
    RecordedSession, SessionRequest, recorded_sessions, session_id, session_meta, write_archive,
};
use crate::config::{ApplianceConfig, NetworkSurvey, PairedNode, Uplink};
use crate::credential::AdminCredential;
use crate::discovery::Discovery;
use crate::history::{MINUTE_US, MinuteSummary};
use crate::journal::{
    EVENT_CATEGORIES, Event, EventCategory, EventKind, EventPage, Journal, RecordedEvent,
};
use crate::model;
use crate::now_us;
use crate::pipeline::StreamHealth;
use crate::schedule::{ServiceState, ServiceWindow};
use crate::session::{ABSOLUTE_LIFETIME_US, SessionStore};
use crate::state::{Phase, Readiness, Runtime, RuntimeMode, Stage};
use crate::system::SystemReport;
use crate::throttle::Throttle;
use flow_core::{DensityClass, Label, NodeRole};
use flow_ingest::SenderObservation;
use flow_ingest::SessionWriter;

/// Name of the cookie carrying the session token.
const SESSION_COOKIE: &str = "mf_session";

/// Why a configuration write was refused.
pub enum WriteRejection {
    /// The result would not be a usable configuration. Carries the reason,
    /// which names the field at fault and never a path on the appliance.
    Invalid(String),
    /// A sound change could not be recorded — a read-only card, a full disk.
    Storage,
}

/// Answers a refused write.
///
/// A storage failure is not the caller's to fix by sending something else, so
/// it is reported as the appliance's fault rather than theirs.
fn refusal(rejection: &WriteRejection) -> Response {
    match rejection {
        WriteRejection::Invalid(reason) => error_response(StatusCode::BAD_REQUEST, reason),
        WriteRejection::Storage => error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "the appliance could not store the change",
        ),
    }
}

struct Inner {
    config: ApplianceConfig,
    /// Where sessions and the active model live.
    data_dir: std::path::PathBuf,
    /// Whether a calibration session has ever been sealed here.
    site_captured: bool,
    /// Bumped on every accepted write, so the intake can notice that the
    /// configuration it was built from is no longer the current one.
    config_generation: u64,
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
    /// Set once the daemon has been asked to stop, so responses that would
    /// otherwise never end can end.
    stopping: Arc<tokio::sync::watch::Sender<bool>>,
    /// Why the journal last refused a write, if it is refusing them.
    journal_failure: Arc<Mutex<Option<JournalFailure>>>,
}

/// The live surface, written by the pipeline thread and read by handlers.
///
/// A `watch` channel rather than a queue: a client wants the *current*
/// estimate, not every one ever produced, and a slow reader must never
/// apply back-pressure to sensing.
struct Live {
    estimates: tokio::sync::watch::Sender<Option<flow_infer::WaitEstimate>>,
    health: Mutex<StreamHealth>,
    /// Senders the intake has seen that no node mapping claims.
    observations: Mutex<Vec<SenderObservation>>,
    /// The session being recorded, if one is.
    ///
    /// Opened by the handler that starts it, so a directory that cannot be
    /// created is reported in the response rather than failing silently on
    /// the intake thread; written to by that thread, which is where the
    /// frames are.
    recorder: Mutex<Option<SessionWriter>>,
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
        data_dir: std::path::PathBuf,
        credential: AdminCredential,
        model_installed: bool,
        journal: Journal,
    ) -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner {
                config,
                site_captured: crate::calibration::has_sealed_session(&data_dir.join("sessions")),
                data_dir,
                config_generation: 0,
                config_path,
                credential,
                model_installed,
                runtime: Runtime::new(),
                sessions: SessionStore::default(),
                throttle: Throttle::new(),
            })),
            login_gate: Arc::new(tokio::sync::Mutex::new(())),
            journal: Arc::new(Mutex::new(journal)),
            stopping: Arc::new(tokio::sync::watch::Sender::new(false)),
            journal_failure: Arc::new(Mutex::new(None)),
            live: Arc::new(Live {
                estimates: tokio::sync::watch::Sender::new(None),
                health: Mutex::new(StreamHealth::default()),
                observations: Mutex::new(Vec::new()),
                recorder: Mutex::new(None),
            }),
        }
    }

    /// Asks every long-lived response to end.
    ///
    /// The live stream never completes on its own, so a graceful shutdown that
    /// waits for connections to finish would wait for as long as one dashboard
    /// is open — until a supervisor kills the process, and with it the journal
    /// entries still buffered.
    pub fn begin_shutdown(&self) {
        self.stopping.send_replace(true);
    }

    /// Watches for the daemon being asked to stop.
    fn watch_stopping(&self) -> tokio::sync::watch::Receiver<bool> {
        self.stopping.subscribe()
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

    /// Marks the intake as reading or stopped.
    pub fn set_stream_running(&self, running: bool) {
        self.lock_health().running = running;
    }

    /// Marks whether an estimator is attached to the stream.
    pub fn set_estimating(&self, estimating: bool) {
        self.lock_health().estimating = estimating;
    }

    /// How the stream is feeding the pipeline.
    #[must_use]
    pub fn stream_health(&self) -> StreamHealth {
        self.lock_health().clone()
    }

    /// Replaces what the intake has seen from unmapped senders.
    pub fn set_observations(&self, observations: Vec<SenderObservation>) {
        let mut held = match self.live.observations.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        *held = observations;
    }

    fn lock_recorder(&self) -> MutexGuard<'_, Option<SessionWriter>> {
        match self.live.recorder.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    /// Writes a frame to the session being recorded, if there is one.
    ///
    /// A rejected frame is counted by the writer and reported, never fatal:
    /// losing the rest of a capture over one malformed frame would cost far
    /// more than the frame.
    pub fn record_frame(&self, frame: &flow_core::CsiFrame) {
        let mut held = self.lock_recorder();
        if let Some(writer) = held.as_mut()
            && let Err(err) = writer.write_frame(frame)
        {
            eprintln!("session frame: {err}");
        }
    }

    /// Claims the stream for a capture, or says why it cannot be claimed.
    ///
    /// # Errors
    ///
    /// The transition the runtime refused.
    pub fn begin_calibration(
        &self,
        session_id: &str,
        started_us: u64,
    ) -> Result<(), crate::error::TransitionError> {
        self.lock()
            .runtime
            .start_calibration(session_id, started_us)
    }

    /// Records whether an active model artifact is present.
    pub fn set_model_installed(&self, installed: bool) {
        self.lock().model_installed = installed;
    }

    /// Marks the configuration as changed without changing it.
    ///
    /// The intake is rebuilt from the configuration *and* the model on disk;
    /// replacing the model alone still has to reach it.
    pub fn bump_configuration(&self) {
        self.lock().config_generation += 1;
    }

    /// Records that a usable session now exists at this site.
    pub fn mark_site_captured(&self) {
        self.lock().site_captured = true;
    }

    /// Releases the stream, whatever it was doing.
    pub fn end_calibration(&self) {
        self.lock().runtime.stop();
    }

    /// Hands the open session to the intake thread.
    pub fn attach_recorder(&self, writer: SessionWriter) {
        *self.lock_recorder() = Some(writer);
    }

    /// Takes the session back, leaving nothing recording.
    pub fn detach_recorder(&self) -> Option<SessionWriter> {
        self.lock_recorder().take()
    }

    /// Annotates the capture in progress.
    ///
    /// # Errors
    ///
    /// A message when no capture is running, or when the writer refused it.
    pub fn record_label(&self, label: &Label) -> Result<(), String> {
        let mut held = self.lock_recorder();
        let writer = held.as_mut().ok_or("no capture is running")?;
        writer.write_label(label).map_err(|err| err.to_string())
    }

    /// Whether a capture is being recorded.
    #[must_use]
    pub fn is_recording(&self) -> bool {
        self.lock_recorder().is_some()
    }

    /// What is streaming unpaired, with the pairing offered for it.
    fn discovery(&self) -> Discovery {
        let observations = match self.live.observations.lock() {
            Ok(guard) => guard.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        };
        let paired = self.lock().config.rx_node_ids();
        crate::discovery::discover(&observations, &paired, now_us())
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
        let outcome = self.journal().record(event, now);
        self.note_journal(&outcome, "recording");
    }

    /// Writes everything the journal has buffered.
    pub fn flush_journal(&self) {
        let outcome = self.journal().flush();
        self.note_journal(&outcome, "writing");
    }

    /// Drops events past the retention window or the row cap.
    pub fn prune_journal(&self) {
        let now = now_us();
        let outcome = self.journal().prune(now).map(|_| ());
        self.note_journal(&outcome, "pruning");
    }

    /// Remembers whether the journal can still be written.
    ///
    /// A card that has gone read-only — how these usually end — makes every
    /// write fail. Left to a console nobody reads, the appliance would look
    /// healthy while keeping no record of anything at all, which is the one
    /// failure a journal must not have quietly.
    fn note_journal<T>(
        &self,
        outcome: &Result<T, crate::error::JournalError>,
        doing: &'static str,
    ) {
        let mut failure = match self.journal_failure.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        match outcome {
            Ok(_) => *failure = None,
            Err(err) => {
                if failure.is_none() {
                    eprintln!("journal {doing}: {err}");
                }
                *failure = Some(JournalFailure {
                    since_us: failure.map_or_else(now_us, |previous| previous.since_us),
                });
            }
        }
    }

    /// Since when the journal has been failing to write, if it is.
    #[must_use]
    pub fn journal_failure(&self) -> Option<JournalFailure> {
        match self.journal_failure.lock() {
            Ok(guard) => *guard,
            Err(poisoned) => *poisoned.into_inner(),
        }
    }

    fn recent_events(&self, page: &EventPage) -> Vec<RecordedEvent> {
        self.journal().events(page).unwrap_or_default()
    }

    fn newest_event_id(&self) -> Option<i64> {
        self.recent_events(&EventPage {
            limit: 1,
            ..EventPage::default()
        })
        .first()
        .map(|event| event.id)
    }

    fn journal(&self) -> MutexGuard<'_, Journal> {
        match self.journal.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    /// Applies a change to the configuration and persists it.
    ///
    /// The candidate is validated before the save that would validate it
    /// anyway, because only this error names the field at fault rather than
    /// the file. The in-memory copy is replaced only once the write
    /// succeeded.
    ///
    /// # Errors
    ///
    /// [`WriteRejection::Invalid`] with the reason, or
    /// [`WriteRejection::Storage`] when a sound change cannot be recorded.
    pub fn write_config(
        &self,
        change: impl FnOnce(&mut ApplianceConfig),
    ) -> Result<(), WriteRejection> {
        let mut inner = self.lock();
        let mut candidate = inner.config.clone();
        change(&mut candidate);
        candidate
            .validate()
            .map_err(|err| WriteRejection::Invalid(err.to_string()))?;
        candidate
            .save(&inner.config_path)
            .map_err(|_| WriteRejection::Storage)?;
        inner.config = candidate;
        inner.config_generation += 1;
        Ok(())
    }

    /// How many times the configuration has been replaced.
    #[must_use]
    pub fn config_generation(&self) -> u64 {
        self.lock().config_generation
    }

    /// A copy of the configuration as it now stands.
    #[must_use]
    pub fn config_snapshot(&self) -> ApplianceConfig {
        self.lock().config.clone()
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
        Readiness::evaluate(&inner.config, inner.model_installed, inner.site_captured)
    }

    /// What the appliance is doing at the product level.
    #[must_use]
    pub fn phase(&self) -> Phase {
        let inner = self.lock();
        Phase::of(
            Readiness::evaluate(&inner.config, inner.model_installed, inner.site_captured),
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
        if let Err(refusal) = self.lock().throttle.check(client, now) {
            // One entry per block, not per request: a refusal is decided
            // before the hash is computed, so it costs the client nothing and
            // a row each would let anyone on the network evict the journal.
            if refusal.first {
                self.record(Event::new(EventKind::LoginThrottled).from_client(client));
            }
            return Err(LoginRefusal::TooManyAttempts {
                wait_us: refusal.wait_us,
            });
        }
        let suppressed = self.lock().throttle.take_suppressed(client);
        if suppressed > 0 {
            self.record(
                Event::new(EventKind::LoginThrottled)
                    .from_client(client)
                    .with_detail(format!("{suppressed} further attempts refused")),
            );
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
        let readiness =
            Readiness::evaluate(&inner.config, inner.model_installed, inner.site_captured);
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
            survey: config.network.survey,
            classes: config.classes.clone(),
            active_model: inner.config.active_model.clone(),
            journal_failure: self.journal_failure(),
            nodes: config
                .nodes
                .iter()
                .map(|node| NodeView {
                    node_id: node.node_id.clone(),
                    role: node.role,
                    mac: node.mac.clone(),
                    address: node.address.map(|address| address.to_string()),
                    position: node.position.clone(),
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
        .route("/api/events.csv", get(events_csv))
        .route("/api/live", get(live))
        .route("/api/estimates", get(estimates))
        .route("/api/discovery", get(discovery))
        .route("/api/system", get(system))
        .route("/api/site", put(set_site))
        .route("/api/nodes", put(set_nodes))
        .route("/api/nodes/{node_id}", axum::routing::patch(describe_node))
        .route("/api/nodes/{node_id}/hardware", post(adopt_hardware))
        .route("/api/uplink", put(set_uplink))
        .route("/api/network-survey", put(set_network_survey))
        .route(
            "/api/model",
            post(import_model).layer(axum::extract::DefaultBodyLimit::max(MAX_BUNDLE_BYTES)),
        )
        .route("/api/models", get(models))
        .route(
            "/api/models/{model_id}",
            axum::routing::delete(forget_model)
                .post(use_model)
                .patch(rename_model),
        )
        .route("/api/classes", put(set_classes))
        .route(
            "/api/calibration",
            post(start_calibration).delete(stop_calibration),
        )
        .route("/api/calibration/label", post(add_label))
        .route("/api/sessions", get(sessions))
        .route(
            "/api/sessions/{session_id}",
            axum::routing::delete(delete_session).patch(rename_session),
        )
        .route("/api/sessions/{session_id}/archive", get(session_archive))
        .route("/api/installation", put(set_installation))
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
    /// Newest row of the journal, or nothing if it is empty.
    ///
    /// Carried here rather than polled for: this stream already ticks every
    /// second for the live view, so a reader of the journal learns that
    /// something happened within a second and at the cost of one integer.
    #[serde(skip_serializing_if = "Option::is_none")]
    journal_id: Option<i64>,
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
            journal_id: self.newest_event_id(),
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
    // Driven by a tick as well as by new estimates. On estimates alone, the
    // stream falls silent in exactly the situations a watcher needs to hear
    // about: every sensor gone quiet produces no estimate, so the screen
    // would freeze on its last good state instead of reporting the silence —
    // and a capture, which suspends estimation entirely, would show a clock
    // that never advances.
    let estimates =
        tokio_stream::wrappers::WatchStream::new(state.watch_estimates()).map(|_| false);
    let ticks = tokio_stream::wrappers::IntervalStream::new(tokio::time::interval(
        std::time::Duration::from_secs(1),
    ))
    .map(|_| false);

    // Merged in as a third source rather than raced against the whole stream:
    // this response has to *end* when the daemon is asked to stop, or a
    // graceful shutdown waits for as long as one dashboard is left open.
    let stopping = tokio_stream::wrappers::WatchStream::new(state.watch_stopping());

    let stream = estimates
        .merge(ticks)
        .merge(stopping)
        .map_while(move |stopping| {
            if stopping {
                return None;
            }
            let event = SseEvent::default()
                .json_data(state.live_snapshot())
                .unwrap_or_else(|_| SseEvent::default().comment("snapshot unavailable"));
            Some(Ok(event))
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

/// Reports what the machine says about itself.
///
/// Read on each request rather than cached: uptime, load and temperature are
/// the point, and a stale temperature is worse than none.
async fn system() -> Json<SystemReport> {
    Json(SystemReport::read())
}

/// Reports what is streaming that no node mapping claims.
///
/// Always available, never a mode: replacing a node on a running
/// installation must not mean stopping the estimation to find it again.
async fn discovery(State(state): State<EdgeState>) -> Json<Discovery> {
    Json(state.discovery())
}

#[derive(Deserialize)]
struct SiteBody {
    site_name: String,
}

/// Names the site this appliance is installed at.
async fn set_site(
    State(state): State<EdgeState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Json(body): Json<SiteBody>,
) -> Response {
    let name = body.site_name.trim().to_owned();
    match state.write_config(|config| config.identity.site_name = Some(name.clone())) {
        Ok(()) => {
            state.record(
                Event::new(EventKind::ConfigurationChanged)
                    .from_client(peer.ip())
                    .with_detail(format!("site named {name:?}")),
            );
            (StatusCode::OK, Json(state.status())).into_response()
        }
        Err(rejection) => refusal(&rejection),
    }
}

/// Replaces the paired nodes with the set the installer confirmed.
///
/// The whole list at once: identifiers, addresses and MACs must be unique
/// across it and only one node may transmit, so checking an addition against
/// a stored remainder would be checking half a thing.
async fn set_nodes(
    State(state): State<EdgeState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Json(nodes): Json<Vec<PairedNode>>,
) -> Response {
    let described = format!("{} node(s) paired", nodes.len());
    match state.write_config(|config| config.nodes = nodes) {
        Ok(()) => {
            state.record(
                Event::new(EventKind::ConfigurationChanged)
                    .from_client(peer.ip())
                    .with_detail(described),
            );
            (StatusCode::OK, Json(state.status())).into_response()
        }
        Err(rejection) => refusal(&rejection),
    }
}

/// Where a node physically sits.
#[derive(Debug, Deserialize)]
struct Placement {
    /// The operator's own words, or nothing to say it is unknown again.
    position: String,
}

/// Describes where one node sits.
async fn describe_node(
    State(state): State<EdgeState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    axum::extract::Path(node_id): axum::extract::Path<String>,
    Json(placement): Json<Placement>,
) -> Response {
    let position = placement.position.trim();
    // A blank field is a statement — "I do not know where this one is" — and
    // not the same thing as a refused name.
    let position = (!position.is_empty()).then(|| position.to_owned());

    let mut known = false;
    let outcome = state.write_config(|config| {
        for node in &mut config.nodes {
            if node.node_id == node_id {
                node.position.clone_from(&position);
                known = true;
            }
        }
    });
    if !known {
        return error_response(StatusCode::NOT_FOUND, "no such node");
    }
    match outcome {
        Ok(()) => {
            state.record(
                Event::new(EventKind::ConfigurationChanged)
                    .from_client(peer.ip())
                    .with_detail(format!("node {node_id} placed")),
            );
            (StatusCode::OK, Json(state.status())).into_response()
        }
        Err(rejection) => refusal(&rejection),
    }
}

/// The hardware now answering for a node.
#[derive(Debug, Deserialize)]
struct Hardware {
    /// Source address of the replacement, for a receiver.
    #[serde(default)]
    address: Option<IpAddr>,
    /// Hardware address of the replacement, for a transmitter.
    #[serde(default)]
    mac: Option<String>,
}

/// Points an existing node at the hardware that replaced it.
///
/// The identifier is kept rather than the node re-paired from scratch: it is
/// what capture sessions are written against, and what a trained model was
/// validated for. A receiver renamed on replacement would leave the site with
/// a model that no longer fits it.
async fn adopt_hardware(
    State(state): State<EdgeState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    axum::extract::Path(node_id): axum::extract::Path<String>,
    Json(hardware): Json<Hardware>,
) -> Response {
    let role = {
        let inner = state.lock();
        inner
            .config
            .nodes
            .iter()
            .find(|node| node.node_id == node_id)
            .map(|node| node.role)
    };
    let Some(role) = role else {
        return error_response(StatusCode::NOT_FOUND, "no such node");
    };

    // Each role is known by exactly one thing, so offering the other is a
    // mistake worth naming rather than a field to ignore.
    let described = match (role, &hardware.address, &hardware.mac) {
        (NodeRole::Rx, Some(address), None) => format!("{node_id} now at {address}"),
        (NodeRole::Tx, None, Some(mac)) => format!("{node_id} now {mac}"),
        (NodeRole::Rx, _, _) => {
            return error_response(StatusCode::BAD_REQUEST, "a receiver is adopted by address");
        }
        (NodeRole::Tx, _, _) => {
            return error_response(
                StatusCode::BAD_REQUEST,
                "a transmitter is adopted by MAC address",
            );
        }
    };

    match state.write_config(|config| {
        for node in &mut config.nodes {
            if node.node_id == node_id {
                match role {
                    NodeRole::Rx => node.address = hardware.address,
                    NodeRole::Tx => node.mac.clone_from(&hardware.mac),
                }
            }
        }
    }) {
        Ok(()) => {
            state.record(
                Event::new(EventKind::ConfigurationChanged)
                    .from_client(peer.ip())
                    .with_detail(described),
            );
            (StatusCode::OK, Json(state.status())).into_response()
        }
        Err(rejection) => refusal(&rejection),
    }
}

/// Records how the appliance reaches the site network, or that it will not.
///
/// `null` returns the question to unanswered, which is a different state from
/// a deliberate `offline` and is what the installation progress reads.
async fn set_uplink(
    State(state): State<EdgeState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Json(uplink): Json<Option<Uplink>>,
) -> Response {
    // Named by shape, never by secret: the journal is readable by anyone who
    // can read the appliance, and a passphrase must not travel into it.
    let described = match uplink.as_ref() {
        None => "uplink question reopened".to_owned(),
        Some(Uplink::Offline) => "uplink set offline".to_owned(),
        Some(Uplink::Wifi { ssid, .. }) => format!("uplink set to Wi-Fi {ssid:?}"),
        Some(Uplink::Ethernet { .. }) => "uplink set to wired".to_owned(),
    };
    match state.write_config(|config| config.network.uplink = uplink) {
        Ok(()) => {
            state.record(
                Event::new(EventKind::ConfigurationChanged)
                    .from_client(peer.ip())
                    .with_detail(described),
            );
            (StatusCode::OK, Json(state.status())).into_response()
        }
        Err(rejection) => refusal(&rejection),
    }
}

/// Largest bundle the appliance will read into memory.
///
/// A classical model is kilobytes. The cap is not about real models but about
/// what an authenticated caller could otherwise ask a 512 MB machine to hold.
const MAX_BUNDLE_BYTES: usize = 32 * 1024 * 1024;

/// Takes a trained model into service.
///
/// Validated, then activated in one move: an import that left the appliance
/// with a checked model it was not using would need a second visit to finish,
/// and the person who did the import has already left.
async fn import_model(
    State(state): State<EdgeState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    body: axum::body::Bytes,
) -> Response {
    let (data_dir, rx_nodes) = {
        let inner = state.lock();
        (inner.data_dir.clone(), inner.config.rx_node_ids())
    };
    if rx_nodes.is_empty() {
        return error_response(StatusCode::CONFLICT, "no receiver is paired");
    }

    let staging = data_dir.join("model.staging");
    let staged = match model::stage(&body, &staging) {
        Ok(staged) => staged,
        Err(err) => return error_response(StatusCode::BAD_REQUEST, &err.to_string()),
    };
    if let Err(err) = model::check(&staged, rx_nodes) {
        let _ = std::fs::remove_dir_all(&staging);
        return error_response(StatusCode::BAD_REQUEST, &err.to_string());
    }
    let stored = match model::store(&staged, &data_dir, now_us()) {
        Ok(id) => id,
        Err(err) => {
            let _ = std::fs::remove_dir_all(&staging);
            return error_response(StatusCode::INTERNAL_SERVER_ERROR, &err.to_string());
        }
    };
    let _ = std::fs::remove_dir_all(&staging);

    let named = staged
        .manifest
        .as_ref()
        .map_or_else(|| stored.clone(), |manifest| manifest.name.clone());
    put_in_service(&state, peer.ip(), &data_dir, &stored, &named)
}

/// Puts a stored model into service and lets the intake know.
///
/// Writing the configuration bumps its generation, which is what makes the
/// intake rebuild and pick the model up — the appliance starts estimating
/// without being restarted.
fn put_in_service(
    state: &EdgeState,
    peer: IpAddr,
    data_dir: &std::path::Path,
    id: &str,
    named: &str,
) -> Response {
    let tuning = match model::activate(data_dir, id) {
        Ok(tuning) => tuning,
        Err(err) => return error_response(StatusCode::CONFLICT, &err.to_string()),
    };
    let handle = id.to_owned();
    if let Err(rejection) = state.write_config(|config| {
        config.site = Some(tuning);
        config.active_model = Some(handle);
    }) {
        return refusal(&rejection);
    }
    state.set_model_installed(true);
    state.record(
        Event::new(EventKind::ModelActivated)
            .from_client(peer)
            .with_detail(named.to_owned()),
    );
    (StatusCode::OK, Json(state.status())).into_response()
}

/// Lists every model the appliance holds, newest first.
async fn models(State(state): State<EdgeState>) -> Json<Vec<model::StoredModel>> {
    let (data_dir, active) = {
        let inner = state.lock();
        (inner.data_dir.clone(), inner.config.active_model.clone())
    };
    Json(model::library(&data_dir, active.as_deref()))
}

/// Puts one of the stored models back into service.
async fn use_model(
    State(state): State<EdgeState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    axum::extract::Path(model_id): axum::extract::Path<String>,
) -> Response {
    let data_dir = state.lock().data_dir.clone();
    put_in_service(&state, peer.ip(), &data_dir, &model_id, &model_id)
}

/// Removes a stored model.
async fn forget_model(
    State(state): State<EdgeState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    axum::extract::Path(model_id): axum::extract::Path<String>,
) -> Response {
    let (data_dir, active) = {
        let inner = state.lock();
        (inner.data_dir.clone(), inner.config.active_model.clone())
    };
    // Refused rather than allowed to leave the appliance estimating from a
    // model nobody can name any more.
    if active.as_deref() == Some(model_id.as_str()) {
        return error_response(StatusCode::CONFLICT, "that model is in service");
    }
    match model::remove(&data_dir, &model_id) {
        Ok(()) => {
            state.record(
                Event::new(EventKind::ConfigurationChanged)
                    .from_client(peer.ip())
                    .with_detail(format!("model removed: {model_id}")),
            );
            StatusCode::NO_CONTENT.into_response()
        }
        Err(err) => error_response(StatusCode::NOT_FOUND, &err.to_string()),
    }
}

/// A new name for something the appliance holds.
#[derive(Debug, Deserialize)]
struct Rename {
    /// What it should be called from now on.
    name: String,
}

impl Rename {
    /// The trimmed name, or nothing if it says nothing.
    ///
    /// A blank name is refused rather than stored: a list of recordings where
    /// one row is empty is a list nobody can act on.
    fn trimmed(&self) -> Option<&str> {
        let name = self.name.trim();
        (!name.is_empty()).then_some(name)
    }
}

/// Renames a stored model.
async fn rename_model(
    State(state): State<EdgeState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    axum::extract::Path(model_id): axum::extract::Path<String>,
    Json(rename): Json<Rename>,
) -> Response {
    let Some(name) = rename.trimmed() else {
        return error_response(StatusCode::BAD_REQUEST, "a name is required");
    };
    let data_dir = state.lock().data_dir.clone();
    match model::rename(&data_dir, &model_id, name) {
        Ok(()) => {
            state.record(
                Event::new(EventKind::ConfigurationChanged)
                    .from_client(peer.ip())
                    .with_detail(format!("model renamed: {model_id}")),
            );
            StatusCode::NO_CONTENT.into_response()
        }
        Err(err) => error_response(StatusCode::NOT_FOUND, &err.to_string()),
    }
}

/// Lists the captures recorded at this site, newest first.
async fn sessions(State(state): State<EdgeState>) -> Json<Vec<RecordedSession>> {
    let root = state.lock().data_dir.join("sessions");
    Json(recorded_sessions(&root))
}

/// Removes a recorded capture.
///
/// Offered because a truncated or mistaken capture is dead weight on a card
/// shared with everything else the appliance stores.
async fn delete_session(
    State(state): State<EdgeState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    axum::extract::Path(session_id): axum::extract::Path<String>,
) -> Response {
    let root = state.lock().data_dir.join("sessions");
    let Some(dir) = session_dir(&root, &session_id) else {
        return error_response(StatusCode::BAD_REQUEST, "not a session identifier");
    };
    if !dir.is_dir() {
        return error_response(StatusCode::NOT_FOUND, "no such session");
    }
    match std::fs::remove_dir_all(&dir) {
        Ok(()) => {
            state.record(
                Event::new(EventKind::CalibrationStopped)
                    .from_client(peer.ip())
                    .with_detail(format!("session deleted: {session_id}")),
            );
            StatusCode::NO_CONTENT.into_response()
        }
        Err(err) => error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            &format!("could not delete the session: {err}"),
        ),
    }
}

/// Renames a recorded capture.
async fn rename_session(
    State(state): State<EdgeState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    axum::extract::Path(session_id): axum::extract::Path<String>,
    Json(rename): Json<Rename>,
) -> Response {
    let Some(name) = rename.trimmed() else {
        return error_response(StatusCode::BAD_REQUEST, "a name is required");
    };
    let root = state.lock().data_dir.join("sessions");
    let Some(dir) = session_dir(&root, &session_id) else {
        return error_response(StatusCode::BAD_REQUEST, "not a session identifier");
    };
    if !dir.is_dir() {
        return error_response(StatusCode::NOT_FOUND, "no such session");
    }
    match crate::calibration::rename_session(&dir, name) {
        Ok(()) => {
            state.record(
                Event::new(EventKind::ConfigurationChanged)
                    .from_client(peer.ip())
                    .with_detail(format!("session renamed: {session_id}")),
            );
            StatusCode::NO_CONTENT.into_response()
        }
        Err(err) => error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            &format!("could not rename the session: {err}"),
        ),
    }
}

/// The directory a session identifier names, or nothing if it names anything
/// else.
///
/// Refused rather than sanitised: the identifier is a directory name, and a
/// value that could climb out of the sessions root is not a mistyped session,
/// it is not a session at all.
fn session_dir(root: &std::path::Path, session_id: &str) -> Option<std::path::PathBuf> {
    if session_id.is_empty() || session_id.contains(['/', '\\']) || session_id.contains("..") {
        return None;
    }
    Some(root.join(session_id))
}

/// Sends one capture as a gzipped tar, for training elsewhere.
///
/// Written to a temporary file and streamed from it, rather than assembled in
/// memory: a capture runs to tens of megabytes, on a machine with 512 MB.
async fn session_archive(
    State(state): State<EdgeState>,
    axum::extract::Path(session_id): axum::extract::Path<String>,
) -> Response {
    let root = state.lock().data_dir.join("sessions");
    let Some(session_dir) = session_dir(&root, &session_id) else {
        return error_response(StatusCode::BAD_REQUEST, "not a session identifier");
    };
    if !session_dir.is_dir() {
        return error_response(StatusCode::NOT_FOUND, "no such session");
    }

    let named = crate::calibration::archive_name(
        &session_id,
        &recorded_sessions(&root)
            .into_iter()
            .find(|session| session.session_id == session_id)
            .map(|session| session.environment)
            .unwrap_or_default(),
    );
    let archive_path = root.join(format!("{session_id}.tar.gz"));
    let built = {
        let (dir, id, path) = (
            session_dir.clone(),
            session_id.clone(),
            archive_path.clone(),
        );
        tokio::task::spawn_blocking(move || write_archive(&dir, &id, &path)).await
    };
    if !matches!(built, Ok(Ok(()))) {
        let _ = std::fs::remove_file(&archive_path);
        return error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "could not build the archive",
        );
    }

    let Ok(file) = tokio::fs::File::open(&archive_path).await else {
        let _ = std::fs::remove_file(&archive_path);
        return error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "could not read the archive",
        );
    };
    // Removed now: the open handle keeps it readable until the body is sent,
    // so no half-built archive survives a client that walks away.
    let _ = std::fs::remove_file(&archive_path);

    let body = axum::body::Body::from_stream(tokio_util::io::ReaderStream::new(file));
    (
        [
            (header::CONTENT_TYPE, "application/gzip".to_owned()),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{named}\""),
            ),
        ],
        body,
    )
        .into_response()
}

/// Records what each density class means at this site.
///
/// A property of the queue rather than of one capture, so it is answered once
/// and copied into every session recorded afterwards.
async fn set_classes(
    State(state): State<EdgeState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Json(classes): Json<Option<flow_core::ClassMapping>>,
) -> Response {
    match state.write_config(|config| config.classes = classes) {
        Ok(()) => {
            state.record(
                Event::new(EventKind::ConfigurationChanged)
                    .from_client(peer.ip())
                    .with_detail("density classes described".to_owned()),
            );
            (StatusCode::OK, Json(state.status())).into_response()
        }
        Err(rejection) => refusal(&rejection),
    }
}

/// Starts recording a labeled capture session.
///
/// The session directory is created here rather than on the intake thread, so
/// a full card or a name already taken is answered to the caller instead of
/// failing out of sight.
///
/// Estimation is not torn down: the intake skips that stage while a capture is
/// running and resumes on its own when the capture ends, which is why nothing
/// has to remember whether it was estimating beforehand.
async fn start_calibration(
    State(state): State<EdgeState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Json(request): Json<SessionRequest>,
) -> Response {
    let (config, data_dir) = {
        let inner = state.lock();
        (inner.config.clone(), inner.data_dir.clone())
    };
    if config.rx_node_ids().is_empty() {
        return error_response(StatusCode::CONFLICT, "no receiver is paired");
    }
    // Refused rather than started empty: a capture recorded while nothing is
    // being read produces a session with labels and no frames, and the person
    // labelling would spend the hour finding out afterwards.
    if !state.stream_health().running {
        return error_response(
            StatusCode::CONFLICT,
            "the appliance is not reading any stream",
        );
    }

    let id = session_id(now_us(), &config.identity.kit_id);
    if let Err(err) = state.begin_calibration(&id, now_us()) {
        return error_response(StatusCode::CONFLICT, &err.to_string());
    }

    let meta = session_meta(&config, &request, id.clone());
    match SessionWriter::create(&data_dir.join("sessions"), &meta) {
        Ok(writer) => {
            state.attach_recorder(writer);
            state.record(
                Event::new(EventKind::CalibrationStarted)
                    .from_client(peer.ip())
                    .with_detail(id.clone()),
            );
            (StatusCode::OK, Json(state.status())).into_response()
        }
        Err(err) => {
            // The runtime was claimed a moment ago; releasing it here keeps a
            // failed start from leaving the appliance unable to try again.
            state.end_calibration();
            error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                &format!("could not open the session: {err}"),
            )
        }
    }
}

/// Ends the capture and seals its directory.
async fn stop_calibration(
    State(state): State<EdgeState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
) -> Response {
    let Some(writer) = state.detach_recorder() else {
        return error_response(StatusCode::CONFLICT, "no capture is running");
    };
    state.end_calibration();

    match writer.finalize() {
        Ok(summary) => {
            // The site counts as captured from here, not from the next boot:
            // sealing is what makes the session usable, and the installation
            // waits on exactly that.
            state.mark_site_captured();
            state.record(
                Event::new(EventKind::CalibrationStopped)
                    .from_client(peer.ip())
                    .with_detail(format!(
                        "{} frames, {} labels",
                        summary.frames, summary.labels
                    )),
            );
            (StatusCode::OK, Json(state.status())).into_response()
        }
        Err(err) => error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            &format!("could not seal the session: {err}"),
        ),
    }
}

#[derive(Deserialize)]
struct LabelBody {
    class: DensityClass,
}

/// Annotates the capture with what is being observed right now.
///
/// Stamped by the appliance clock, never by the caller's: the phone doing the
/// labeling and the appliance recording the frames are two machines, and a
/// label has to land on the same timeline as the frames it describes.
async fn add_label(State(state): State<EdgeState>, Json(body): Json<LabelBody>) -> Response {
    let label = Label {
        ts_us: now_us(),
        class: body.class,
        count: None,
    };
    match state.record_label(&label) {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(message) => error_response(StatusCode::CONFLICT, &message),
    }
}

/// Records what the site's network was found to ask for.
///
/// Separate from the uplink because it is a statement about the site rather
/// than about the appliance: it stays true when the appliance is left offline
/// precisely because the site asks for something it cannot yet offer, and the
/// request sent to the network administrator is built from it.
async fn set_network_survey(
    State(state): State<EdgeState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Json(survey): Json<Option<NetworkSurvey>>,
) -> Response {
    let described = survey.map_or_else(
        || "network survey cleared".to_owned(),
        |survey| format!("network survey recorded ({:?})", survey.authentication),
    );
    match state.write_config(|config| config.network.survey = survey) {
        Ok(()) => {
            state.record(
                Event::new(EventKind::ConfigurationChanged)
                    .from_client(peer.ip())
                    .with_detail(described),
            );
            (StatusCode::OK, Json(state.status())).into_response()
        }
        Err(rejection) => refusal(&rejection),
    }
}

#[derive(Deserialize)]
struct InstallationBody {
    completed: bool,
}

/// Closes the installation, or reopens it.
///
/// Closing is refused while any step is outstanding: the flag only stops the
/// wizard reappearing, so setting it early would leave an operational screen
/// the appliance cannot honour. Reopening is always allowed.
async fn set_installation(
    State(state): State<EdgeState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Json(body): Json<InstallationBody>,
) -> Response {
    if body.completed {
        let readiness = state.readiness();
        let stage = readiness.stage();
        if stage != Stage::Complete {
            return error_response(
                StatusCode::CONFLICT,
                &format!("{stage} is not finished yet"),
            );
        }
    }

    let described = if body.completed {
        "installation closed"
    } else {
        "installation reopened"
    };
    match state.write_config(|config| config.onboarding_completed = body.completed) {
        Ok(()) => {
            state.record(
                Event::new(EventKind::ConfigurationChanged)
                    .from_client(peer.ip())
                    .with_detail(described.to_owned()),
            );
            (StatusCode::OK, Json(state.status())).into_response()
        }
        Err(rejection) => refusal(&rejection),
    }
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
    let described = window.as_ref().map_or_else(
        || "service schedule cleared".to_owned(),
        |window| format!("service schedule set ({})", window.timezone),
    );

    match state.write_config(|config| config.service = window) {
        Ok(()) => {
            state.record(
                Event::new(EventKind::ConfigurationChanged)
                    .from_client(peer.ip())
                    .with_detail(described),
            );
            (StatusCode::OK, Json(state.service_state())).into_response()
        }
        Err(rejection) => refusal(&rejection),
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
    /// Read further back: only rows older than this one.
    before: Option<i64>,
    /// Read forward: only rows newer than this one.
    ///
    /// This is what tells a reader how much has arrived while they were
    /// reading, without the list moving under them.
    after: Option<i64>,
    /// Restrict to one family.
    category: Option<String>,
}

async fn events(
    State(state): State<EdgeState>,
    Query(query): Query<EventsQuery>,
) -> Json<Vec<RecordedEvent>> {
    // An unknown family reads as no filter rather than as an error: it can
    // only come from a hand-written URL, and an empty journal would look like
    // an appliance that has never done anything.
    let category = query
        .category
        .as_deref()
        .and_then(|name| EVENT_CATEGORIES.into_iter().find(|c| c.as_str() == name));

    Json(
        state.recent_events(&EventPage {
            limit: query
                .limit
                .unwrap_or(DEFAULT_EVENT_LIMIT)
                .clamp(1, MAX_EVENT_LIMIT),
            before: query.before,
            after: query.after,
            category,
        }),
    )
}

/// How many rows are held at once while an export is written.
///
/// The whole journal is 20 000 rows; reading it in slices keeps the row buffer
/// small whatever the retention grows to, and the text it produces is under
/// two megabytes.
const CSV_SLICE: usize = 2_000;

#[derive(Deserialize)]
struct CsvQuery {
    category: Option<String>,
}

/// Sends the journal as CSV, honouring the family filter.
///
/// The client address is included: an access incident forwarded to whoever
/// handles it is not usable without saying where it came from. It is personal
/// data, which is why the journal bounds its retention (ADR 0012) — an export
/// takes it off the appliance, and that is the operator's decision to make.
async fn events_csv(State(state): State<EdgeState>, Query(query): Query<CsvQuery>) -> Response {
    let category = query
        .category
        .as_deref()
        .and_then(|name| EVENT_CATEGORIES.into_iter().find(|c| c.as_str() == name));

    let mut out = String::from("time,category,kind,client,detail\n");
    let mut before = None;
    loop {
        let slice = state.recent_events(&EventPage {
            limit: CSV_SLICE,
            before,
            after: None,
            category,
        });
        let Some(last) = slice.last() else { break };
        before = Some(last.id);
        for event in &slice {
            out.push_str(&csv_row(event));
        }
        if slice.len() < CSV_SLICE {
            break;
        }
    }

    (
        [
            (header::CONTENT_TYPE, "text/csv; charset=utf-8".to_owned()),
            (
                header::CONTENT_DISPOSITION,
                format!(
                    "attachment; filename=\"journal-{}.csv\"",
                    category.map_or("all", EventCategory::as_str)
                ),
            ),
        ],
        out,
    )
        .into_response()
}

fn csv_row(event: &RecordedEvent) -> String {
    format!(
        "{},{},{},{},{}\n",
        csv_field(&utc_instant(event.ts_us)),
        event.category.as_str(),
        event.kind.as_str(),
        csv_field(event.client.as_deref().unwrap_or_default()),
        csv_field(event.detail.as_deref().unwrap_or_default()),
    )
}

/// Quotes a field so that a comma, a quote or a newline in an operator's own
/// words cannot end the field early — RFC 4180, doubling the quote.
/// The instant an event carries, in UTC and in a form a spreadsheet sorts.
///
/// UTC rather than the site's zone: an export is read elsewhere, and a naive
/// local time with no offset is the classic way two records of the same
/// incident stop lining up.
fn utc_instant(ts_us: u64) -> String {
    let micros = i64::try_from(ts_us).unwrap_or(0);
    jiff::Timestamp::from_microsecond(micros).map_or_else(
        |_| ts_us.to_string(),
        |ts| ts.strftime("%Y-%m-%dT%H:%M:%SZ").to_string(),
    )
}

fn csv_field(value: &str) -> String {
    if value.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_owned()
    }
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
    #[serde(skip_serializing_if = "Option::is_none")]
    survey: Option<NetworkSurvey>,
    #[serde(skip_serializing_if = "Option::is_none")]
    classes: Option<flow_core::ClassMapping>,
    /// Which stored model is estimating, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    active_model: Option<String>,
    /// Since when the journal has been unable to write, if it cannot.
    #[serde(skip_serializing_if = "Option::is_none")]
    journal_failure: Option<JournalFailure>,
    nodes: Vec<NodeView>,
}

#[derive(Serialize)]
struct SensorApView {
    ssid: String,
    channel: u8,
}

/// The journal refusing to be written, and since when.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct JournalFailure {
    /// When writes started failing.
    pub since_us: u64,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    mac: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    address: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    position: Option<String>,
}

#[cfg(test)]
mod tests {
    use std::net::Ipv4Addr;

    use flow_core::NodeRole;
    use http_body_util::BodyExt;
    use serde_json::json;
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
                mac: Some("1a:00:00:00:00:00".into()),
                address: None,
                position: None,
            },
            PairedNode {
                node_id: "rx-1".into(),
                role: NodeRole::Rx,
                mac: Some("aa:bb:cc:00:00:01".into()),
                address: Some("192.168.4.51".parse().unwrap()),
                position: None,
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
            std::path::PathBuf::from("/nonexistent"),
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
            .with_runtime(|runtime| runtime.start_calibration("s-001", 0))
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
        writable_with(installed(), true)
    }

    /// A state whose intake is reading, which starting a capture requires.
    fn recording_ready() -> (EdgeState, tempfile::TempDir) {
        let (state, dir) = writable();
        state.set_stream_running(true);
        (state, dir)
    }

    fn writable_with(
        config: ApplianceConfig,
        model_installed: bool,
    ) -> (EdgeState, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("appliance.json");
        config.save(&path).unwrap();

        let secret = DeviceSecret::parse(SECRET).unwrap();
        let credential = AdminCredential::establish(&secret, now_us()).unwrap();
        let journal = Journal::open_in_memory().unwrap();
        let state = EdgeState::new(
            config,
            path,
            dir.path().to_path_buf(),
            credential,
            model_installed,
            journal,
        );
        (state, dir)
    }

    async fn put(
        state: &EdgeState,
        path: &str,
        cookie: &str,
        body: serde_json::Value,
    ) -> (StatusCode, serde_json::Value) {
        let (status, _, value) =
            send(state, "PUT", path, Some(cookie), Some(body.to_string()), 10).await;
        (status, value)
    }

    fn stored(state: &EdgeState) -> ApplianceConfig {
        let path = state.lock().config_path.clone();
        ApplianceConfig::load(&path).unwrap()
    }

    fn journal_details(state: &EdgeState) -> String {
        // The journal batches writes for the SD card's sake, so a reader has
        // to ask for them the way the housekeeping task does.
        state.flush_journal();
        state
            .recent_events(&EventPage {
                limit: 50,
                ..EventPage::default()
            })
            .into_iter()
            .filter_map(|event| event.detail)
            .collect::<Vec<String>>()
            .join(" | ")
    }

    #[tokio::test]
    async fn naming_the_site_is_stored_and_journalled() {
        let (state, _dir) = writable_with(factory(), false);
        let cookie = session_of(&state).await;

        let (status, body) = put(
            &state,
            "/api/site",
            &cookie,
            json!({ "site_name": "  RU EFREI  " }),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["site_name"], "RU EFREI");
        assert_eq!(
            stored(&state).identity.site_name.as_deref(),
            Some("RU EFREI")
        );
        assert!(journal_details(&state).contains("RU EFREI"));
    }

    #[tokio::test]
    async fn a_blank_site_name_is_refused_and_nothing_is_stored() {
        let (state, _dir) = writable_with(factory(), false);
        let cookie = session_of(&state).await;

        let (status, body) = put(&state, "/api/site", &cookie, json!({ "site_name": "   " })).await;

        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(body["error"].as_str().unwrap().contains("site_name"));
        assert!(stored(&state).identity.site_name.is_none());
    }

    #[tokio::test]
    async fn a_confirmed_pairing_replaces_the_node_list() {
        let (state, _dir) = writable_with(factory(), false);
        let cookie = session_of(&state).await;
        let nodes = json!([
            { "node_id": "tx-1", "role": "tx", "mac": "1a:00:00:00:00:00" },
            { "node_id": "rx-1", "role": "rx", "mac": "aa:bb:cc:00:00:01", "address": "192.168.4.51" },
        ]);

        let (status, body) = put(&state, "/api/nodes", &cookie, nodes).await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["readiness"]["nodes_paired"], true);
        assert_eq!(stored(&state).nodes.len(), 2);
    }

    #[tokio::test]
    async fn a_pairing_the_appliance_cannot_use_is_refused_by_its_reason() {
        let (state, _dir) = writable_with(factory(), false);
        let cookie = session_of(&state).await;
        let two_transmitters = json!([
            { "node_id": "tx-1", "role": "tx", "mac": "1a:00:00:00:00:00" },
            { "node_id": "tx-2", "role": "tx", "mac": "1a:00:00:00:00:01" },
        ]);

        let (status, body) = put(&state, "/api/nodes", &cookie, two_transmitters).await;

        assert_eq!(status, StatusCode::BAD_REQUEST);
        let message = body["error"].as_str().unwrap();
        assert!(message.contains("transmitter"), "unhelpful: {message}");
        assert!(!message.contains('/'), "leaks a path: {message}");
        assert!(stored(&state).nodes.is_empty());
    }

    #[tokio::test]
    async fn the_uplink_is_journalled_by_shape_never_by_passphrase() {
        // The journal is readable by anyone who can read the appliance, and
        // the status surface already refuses to report secrets. A write must
        // not be the way one escapes.
        let (state, _dir) = writable_with(factory(), false);
        let cookie = session_of(&state).await;
        let uplink = json!({
            "mode": "wifi",
            "ssid": "campus",
            "security": { "type": "wpa-personal", "passphrase": UPLINK_PASSPHRASE },
            "addressing": { "method": "dhcp" },
        });

        let (status, body) = put(&state, "/api/uplink", &cookie, uplink).await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["readiness"]["uplink_decided"], true);
        assert_eq!(body["uplink"]["mode"], "wifi");
        let details = journal_details(&state);
        assert!(details.contains("campus"));
        assert!(
            !details.contains(UPLINK_PASSPHRASE),
            "the journal carries the passphrase"
        );
    }

    async fn post(
        state: &EdgeState,
        path: &str,
        cookie: &str,
        body: serde_json::Value,
    ) -> (StatusCode, serde_json::Value) {
        let (status, _, value) = send(
            state,
            "POST",
            path,
            Some(cookie),
            Some(body.to_string()),
            10,
        )
        .await;
        (status, value)
    }

    #[tokio::test]
    async fn a_capture_claims_the_stream_and_seals_its_directory() {
        let (state, dir) = recording_ready();
        let cookie = session_of(&state).await;

        let (status, body) = post(&state, "/api/calibration", &cookie, json!({})).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["runtime"]["mode"], "calibrating");
        assert!(state.is_recording());

        // While recording, the directory carries the suffix that tells a
        // truncated capture from a clean one.
        let sessions = dir.path().join("sessions");
        let recording: Vec<_> = std::fs::read_dir(&sessions)
            .unwrap()
            .filter_map(|entry| Some(entry.ok()?.file_name().to_string_lossy().into_owned()))
            .collect();
        assert_eq!(recording.len(), 1);
        assert!(recording[0].ends_with(".recording"), "{recording:?}");

        let (status, _, body) = send(
            &state,
            "DELETE",
            "/api/calibration",
            Some(&cookie),
            None,
            10,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["runtime"]["mode"], "idle");
        assert!(!state.is_recording());

        let sealed: Vec<_> = std::fs::read_dir(&sessions)
            .unwrap()
            .filter_map(|entry| Some(entry.ok()?.file_name().to_string_lossy().into_owned()))
            .collect();
        assert!(!sealed[0].ends_with(".recording"), "{sealed:?}");
    }

    #[tokio::test]
    async fn what_the_classes_mean_travels_with_the_session() {
        // Answered once for the site, but each recorded session carries a
        // copy: the stored format has to stay readable on its own, long after
        // the appliance that produced it.
        let (state, dir) = recording_ready();
        let cookie = session_of(&state).await;
        put(
            &state,
            "/api/classes",
            &cookie,
            json!({
                "empty": "personne",
                "low": "quelques personnes",
                "medium": "file constituée",
                "saturated": "file au-delà de la porte",
            }),
        )
        .await;

        post(&state, "/api/calibration", &cookie, json!({})).await;
        send(
            &state,
            "DELETE",
            "/api/calibration",
            Some(&cookie),
            None,
            10,
        )
        .await;

        let session = std::fs::read_dir(dir.path().join("sessions"))
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        let meta: serde_json::Value =
            serde_json::from_slice(&std::fs::read(session.join("meta.json")).unwrap()).unwrap();
        assert_eq!(
            meta["class_mapping"]["saturated"],
            "file au-delà de la porte"
        );
    }

    #[tokio::test]
    async fn a_second_capture_is_refused_while_one_is_running() {
        // The stream has one consumer at a time; the refusal is what keeps a
        // running capture from being cut short by a stray request.
        let (state, _dir) = recording_ready();
        let cookie = session_of(&state).await;
        post(&state, "/api/calibration", &cookie, json!({})).await;

        let (status, _) = post(&state, "/api/calibration", &cookie, json!({})).await;

        assert_eq!(status, StatusCode::CONFLICT);
        assert!(state.is_recording(), "the running capture survived");
    }

    #[tokio::test]
    async fn a_capture_is_refused_while_nothing_is_being_read() {
        // A capture started with no stream records labels against no frames,
        // and whoever is labelling finds out an hour later.
        let (state, _dir) = writable();
        let cookie = session_of(&state).await;
        state.set_stream_running(false);

        let (status, body) = post(&state, "/api/calibration", &cookie, json!({})).await;

        assert_eq!(status, StatusCode::CONFLICT);
        assert!(body["error"].as_str().unwrap().contains("stream"));
        assert!(!state.is_recording());
    }

    #[tokio::test]
    async fn a_capture_needs_a_receiver_to_record_anything() {
        let (state, _dir) = writable_with(factory(), false);
        let cookie = session_of(&state).await;

        let (status, body) = post(&state, "/api/calibration", &cookie, json!({})).await;

        assert_eq!(status, StatusCode::CONFLICT);
        assert!(body["error"].as_str().unwrap().contains("receiver"));
    }

    #[tokio::test]
    async fn labels_are_stamped_by_the_appliance_and_refused_without_a_capture() {
        let (state, _dir) = recording_ready();
        let cookie = session_of(&state).await;

        // Classes travel as the integers the canonical label format freezes,
        // not as names. The labelling phone and the appliance are also two
        // machines, so the timestamp is the appliance's.
        let (status, _) = post(
            &state,
            "/api/calibration/label",
            &cookie,
            json!({ "class": 1 }),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT);

        post(&state, "/api/calibration", &cookie, json!({})).await;
        let (status, _) = post(
            &state,
            "/api/calibration/label",
            &cookie,
            json!({ "class": 1 }),
        )
        .await;
        assert_eq!(status, StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn a_session_identifier_that_leaves_the_sessions_root_is_refused() {
        // The identifier names a directory. A value that could climb out of
        // the root is not a mistyped session, it is not a session at all —
        // refused rather than sanitised into something that looks fine.
        let (state, _dir) = recording_ready();
        let cookie = session_of(&state).await;

        for hostile in ["..", "../../etc", "a/b", "..%2Fetc"] {
            let path = format!("/api/sessions/{hostile}/archive");
            let (status, _, _) = send(&state, "GET", &path, Some(&cookie), None, 10).await;
            assert_ne!(status, StatusCode::OK, "{hostile} was served");

            let path = format!("/api/sessions/{hostile}");
            let (status, _, _) = send(&state, "DELETE", &path, Some(&cookie), None, 10).await;
            assert_ne!(status, StatusCode::NO_CONTENT, "{hostile} was deleted");
        }
    }

    #[tokio::test]
    async fn an_unknown_session_is_not_found_rather_than_an_error() {
        let (state, _dir) = recording_ready();
        let cookie = session_of(&state).await;

        let (status, _, _) = send(
            &state,
            "GET",
            "/api/sessions/kit-0042-20260731T140000Z/archive",
            Some(&cookie),
            None,
            10,
        )
        .await;

        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn a_recorded_session_is_listed_then_downloadable_then_removable() {
        let (state, dir) = recording_ready();
        let cookie = session_of(&state).await;
        post(
            &state,
            "/api/calibration",
            &cookie,
            json!({ "environment": "midi" }),
        )
        .await;
        send(
            &state,
            "DELETE",
            "/api/calibration",
            Some(&cookie),
            None,
            10,
        )
        .await;

        let (_, _, listed) = send(&state, "GET", "/api/sessions", Some(&cookie), None, 10).await;
        let id = listed[0]["session_id"].as_str().unwrap().to_owned();
        assert_eq!(listed[0]["environment"], "midi");
        assert_eq!(listed[0]["sealed"], true);

        let (status, headers, _) = send(
            &state,
            "GET",
            &format!("/api/sessions/{id}/archive"),
            Some(&cookie),
            None,
            10,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            headers.get(header::CONTENT_TYPE).unwrap(),
            "application/gzip"
        );
        // The archive is streamed from a file that is unlinked immediately, so
        // nothing half-built survives in the sessions directory.
        let leftovers: Vec<_> = std::fs::read_dir(dir.path().join("sessions"))
            .unwrap()
            .flatten()
            .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "gz"))
            .collect();
        assert!(leftovers.is_empty(), "an archive was left behind");

        let (status, _, _) = send(
            &state,
            "DELETE",
            &format!("/api/sessions/{id}"),
            Some(&cookie),
            None,
            10,
        )
        .await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        let (_, _, listed) = send(&state, "GET", "/api/sessions", Some(&cookie), None, 10).await;
        assert_eq!(listed.as_array().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn a_renamed_session_keeps_its_identifier_and_is_downloaded_under_its_new_name() {
        let (state, _dir) = recording_ready();
        let cookie = session_of(&state).await;
        post(
            &state,
            "/api/calibration",
            &cookie,
            json!({ "environment": "midi" }),
        )
        .await;
        send(
            &state,
            "DELETE",
            "/api/calibration",
            Some(&cookie),
            None,
            10,
        )
        .await;
        let (_, _, listed) = send(&state, "GET", "/api/sessions", Some(&cookie), None, 10).await;
        let id = listed[0]["session_id"].as_str().unwrap().to_owned();

        let (status, _, _) = send(
            &state,
            "PATCH",
            &format!("/api/sessions/{id}"),
            Some(&cookie),
            Some(json!({ "name": "Service du midi, pluie" }).to_string()),
            10,
        )
        .await;
        assert_eq!(status, StatusCode::NO_CONTENT);

        let (_, _, listed) = send(&state, "GET", "/api/sessions", Some(&cookie), None, 10).await;
        assert_eq!(listed[0]["environment"], "Service du midi, pluie");
        assert_eq!(
            listed[0]["session_id"], id,
            "the identifier is the handle and does not move"
        );

        let (status, headers, _) = send(
            &state,
            "GET",
            &format!("/api/sessions/{id}/archive"),
            Some(&cookie),
            None,
            10,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let disposition = headers
            .get(header::CONTENT_DISPOSITION)
            .unwrap()
            .to_str()
            .unwrap();
        // Recognisable in a downloads folder, and still unique per capture.
        assert!(
            disposition.contains("service-du-midi-pluie-"),
            "{disposition} does not carry the name"
        );
        assert!(disposition.ends_with(".tar.gz\""));
    }

    #[tokio::test]
    async fn a_name_made_only_of_punctuation_cannot_reach_the_download_header() {
        // The operator's own words land in a header; a quote or a newline
        // there would be a header injection.
        let (state, _dir) = recording_ready();
        let cookie = session_of(&state).await;
        post(&state, "/api/calibration", &cookie, json!({})).await;
        send(
            &state,
            "DELETE",
            "/api/calibration",
            Some(&cookie),
            None,
            10,
        )
        .await;
        let (_, _, listed) = send(&state, "GET", "/api/sessions", Some(&cookie), None, 10).await;
        let id = listed[0]["session_id"].as_str().unwrap().to_owned();
        send(
            &state,
            "PATCH",
            &format!("/api/sessions/{id}"),
            Some(&cookie),
            Some(json!({ "name": "\"; rm -rf /\r\nX-Evil: 1" }).to_string()),
            10,
        )
        .await;

        let (status, headers, _) = send(
            &state,
            "GET",
            &format!("/api/sessions/{id}/archive"),
            Some(&cookie),
            None,
            10,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let disposition = headers
            .get(header::CONTENT_DISPOSITION)
            .unwrap()
            .to_str()
            .unwrap();
        assert_eq!(
            disposition,
            format!(
                "attachment; filename=\"rm-rf-x-evil-1-{}.tar.gz\"",
                id.rsplit('-').next().unwrap()
            )
        );
    }

    #[tokio::test]
    async fn a_blank_name_is_refused_rather_than_stored() {
        let (state, _dir) = recording_ready();
        let cookie = session_of(&state).await;
        post(&state, "/api/calibration", &cookie, json!({})).await;
        send(
            &state,
            "DELETE",
            "/api/calibration",
            Some(&cookie),
            None,
            10,
        )
        .await;
        let (_, _, listed) = send(&state, "GET", "/api/sessions", Some(&cookie), None, 10).await;
        let id = listed[0]["session_id"].as_str().unwrap().to_owned();

        let (status, _, _) = send(
            &state,
            "PATCH",
            &format!("/api/sessions/{id}"),
            Some(&cookie),
            Some(json!({ "name": "   " }).to_string()),
            10,
        )
        .await;

        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn the_session_surface_is_closed_to_anonymous_callers() {
        let (state, _dir) = writable();
        for (method, path) in [
            ("GET", "/api/sessions"),
            ("GET", "/api/sessions/s-001/archive"),
            ("DELETE", "/api/sessions/s-001"),
            ("PATCH", "/api/sessions/s-001"),
        ] {
            let (status, _, _) = send(&state, method, path, None, None, 10).await;
            assert_eq!(status, StatusCode::UNAUTHORIZED, "{method} {path} is open");
        }
    }

    #[tokio::test]
    async fn a_node_is_told_where_it_sits_and_can_be_told_it_is_unknown_again() {
        let (state, _dir) = recording_ready();
        let cookie = session_of(&state).await;

        let (status, _, body) = send(
            &state,
            "PATCH",
            "/api/nodes/rx-1",
            Some(&cookie),
            Some(json!({ "position": "  above the entrance  " }).to_string()),
            10,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let placed = body["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|node| node["node_id"] == "rx-1")
            .unwrap()
            .clone();
        assert_eq!(placed["position"], "above the entrance");

        let (_, _, body) = send(
            &state,
            "PATCH",
            "/api/nodes/rx-1",
            Some(&cookie),
            Some(json!({ "position": "   " }).to_string()),
            10,
        )
        .await;
        // Blank is an answer here — "I do not know" — not a refusal.
        let cleared = body["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|node| node["node_id"] == "rx-1")
            .unwrap()
            .clone();
        assert!(cleared.get("position").is_none());
    }

    #[tokio::test]
    async fn replacing_a_receiver_keeps_the_identifier_a_model_was_trained_for() {
        let (state, _dir) = recording_ready();
        let cookie = session_of(&state).await;
        send(
            &state,
            "PATCH",
            "/api/nodes/rx-1",
            Some(&cookie),
            Some(json!({ "position": "above the entrance" }).to_string()),
            10,
        )
        .await;

        let (status, _, body) = send(
            &state,
            "POST",
            "/api/nodes/rx-1/hardware",
            Some(&cookie),
            Some(json!({ "address": "192.168.4.57" }).to_string()),
            10,
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        let node = body["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|node| node["node_id"] == "rx-1")
            .unwrap()
            .clone();
        assert_eq!(node["address"], "192.168.4.57");
        // What the swap exists to protect: the identifier is what sessions
        // are written against and what a model was validated for.
        assert_eq!(node["node_id"], "rx-1");
        assert_eq!(node["position"], "above the entrance");
    }

    #[tokio::test]
    async fn each_role_is_adopted_by_the_one_thing_that_identifies_it() {
        let (state, _dir) = recording_ready();
        let cookie = session_of(&state).await;

        for (node_id, body) in [
            ("rx-1", json!({ "mac": "aa:bb:cc:00:00:09" })),
            ("tx-1", json!({ "address": "192.168.4.57" })),
        ] {
            let (status, _, _) = send(
                &state,
                "POST",
                &format!("/api/nodes/{node_id}/hardware"),
                Some(&cookie),
                Some(body.to_string()),
                10,
            )
            .await;
            assert_eq!(
                status,
                StatusCode::BAD_REQUEST,
                "{node_id} accepted the wrong kind"
            );
        }
    }

    #[tokio::test]
    async fn adopting_an_address_another_node_holds_is_refused() {
        let (state, _dir) = recording_ready();
        let cookie = session_of(&state).await;
        send(
            &state,
            "PUT",
            "/api/nodes",
            Some(&cookie),
            Some(
                json!([
                    { "node_id": "rx-1", "role": "rx", "address": "192.168.4.51" },
                    { "node_id": "rx-2", "role": "rx", "address": "192.168.4.52" },
                ])
                .to_string(),
            ),
            10,
        )
        .await;

        let (status, _, body) = send(
            &state,
            "POST",
            "/api/nodes/rx-2/hardware",
            Some(&cookie),
            Some(json!({ "address": "192.168.4.51" }).to_string()),
            10,
        )
        .await;

        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(
            body["error"].as_str().unwrap().contains("192.168.4.51"),
            "{body} does not name the clash"
        );
    }

    #[tokio::test]
    async fn a_node_the_appliance_does_not_have_is_not_found() {
        let (state, _dir) = recording_ready();
        let cookie = session_of(&state).await;
        for (method, path, body) in [
            ("PATCH", "/api/nodes/rx-9", json!({ "position": "nowhere" })),
            (
                "POST",
                "/api/nodes/rx-9/hardware",
                json!({ "address": "192.168.4.57" }),
            ),
        ] {
            let (status, _, _) = send(
                &state,
                method,
                path,
                Some(&cookie),
                Some(body.to_string()),
                10,
            )
            .await;
            assert_eq!(status, StatusCode::NOT_FOUND, "{method} {path}");
        }
    }

    #[tokio::test]
    async fn the_node_surface_is_closed_to_anonymous_callers() {
        let (state, _dir) = writable();
        for (method, path) in [
            ("PUT", "/api/nodes"),
            ("PATCH", "/api/nodes/rx-1"),
            ("POST", "/api/nodes/rx-1/hardware"),
        ] {
            let (status, _, _) = send(&state, method, path, None, Some("{}".to_owned()), 10).await;
            assert_eq!(status, StatusCode::UNAUTHORIZED, "{method} {path} is open");
        }
    }

    #[tokio::test]
    async fn the_live_stream_ends_when_the_daemon_is_asked_to_stop() {
        // Without this the stream never completes, so a graceful shutdown
        // waits for as long as one dashboard is open — until a supervisor
        // kills the process, taking the still-buffered journal with it.
        let (state, _dir) = writable();
        let cookie = session_of(&state).await;

        let mut request = axum::http::Request::builder()
            .method("GET")
            .uri("/api/live")
            .header(header::COOKIE, cookie)
            .body(axum::body::Body::empty())
            .unwrap();
        request
            .extensions_mut()
            .insert(ConnectInfo(SocketAddr::from((
                Ipv4Addr::new(192, 168, 4, 10),
                51_000,
            ))));
        let response = router(state.clone()).oneshot(request).await.unwrap();

        state.begin_shutdown();

        let drained = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            response.into_body().collect(),
        )
        .await;
        assert!(drained.is_ok(), "the live stream never ended");
    }

    #[tokio::test]
    async fn the_journal_exports_as_csv_and_quotes_what_would_break_a_row() {
        let (state, _dir) = writable();
        let cookie = session_of(&state).await;
        // A detail containing a comma, a quote and a newline: all three end a
        // CSV field early if they are not quoted.
        state.record(
            Event::new(EventKind::ConfigurationChanged)
                .with_detail("site named \"RU, Efrei\"\nline two"),
        );
        state.flush_journal();

        let (status, headers, _) =
            send(&state, "GET", "/api/events.csv", Some(&cookie), None, 10).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            headers.get(header::CONTENT_TYPE).unwrap(),
            "text/csv; charset=utf-8"
        );
    }

    #[test]
    fn a_csv_field_survives_a_comma_a_quote_and_a_newline() {
        assert_eq!(csv_field("plain"), "plain");
        assert_eq!(csv_field("a,b"), "\"a,b\"");
        assert_eq!(csv_field("say \"hi\""), "\"say \"\"hi\"\"\"");
        assert_eq!(csv_field("two\nlines"), "\"two\nlines\"");
    }

    #[test]
    fn an_exported_instant_is_utc_and_sorts_as_text() {
        // Read elsewhere, so a naive local time with no offset is how two
        // records of one incident stop lining up.
        assert_eq!(utc_instant(1_785_700_800_000_000), "2026-08-02T20:00:00Z");
    }

    #[tokio::test]
    async fn the_model_surface_is_closed_to_anonymous_callers() {
        let (state, _dir) = writable();
        for (method, path) in [
            ("GET", "/api/models"),
            ("POST", "/api/models/m-001"),
            ("PATCH", "/api/models/m-001"),
            ("DELETE", "/api/models/m-001"),
        ] {
            let (status, _, _) = send(&state, method, path, None, Some("{}".to_owned()), 10).await;
            assert_eq!(status, StatusCode::UNAUTHORIZED, "{method} {path} is open");
        }
    }

    #[tokio::test]
    async fn calibration_is_closed_to_anonymous_callers() {
        let (state, _dir) = writable();
        for (method, path) in [
            ("POST", "/api/calibration"),
            ("DELETE", "/api/calibration"),
            ("POST", "/api/calibration/label"),
        ] {
            let (status, _, _) = send(&state, method, path, None, Some("{}".to_owned()), 10).await;
            assert_eq!(status, StatusCode::UNAUTHORIZED, "{method} {path} is open");
        }
    }

    #[tokio::test]
    async fn the_survey_outlives_the_decision_it_led_to() {
        // A site that demands 802.1X leaves the appliance offline. The reason
        // has to survive that, or the request sent to its network
        // administrator cannot be rebuilt later.
        let (state, _dir) = writable_with(factory(), false);
        let cookie = session_of(&state).await;

        let (status, body) = put(
            &state,
            "/api/network-survey",
            &cookie,
            json!({ "authentication": "account", "registration_required": true }),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["survey"]["authentication"], "account");

        let (status, body) =
            put(&state, "/api/uplink", &cookie, json!({ "mode": "offline" })).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["uplink"]["mode"], "offline");
        assert_eq!(body["survey"]["authentication"], "account");

        let stored = stored(&state).network.survey.unwrap();
        assert!(stored.registration_required);
        assert!(!stored.authentication.joinable());
    }

    #[tokio::test]
    async fn a_survey_of_an_ordinary_network_reports_it_as_joinable() {
        let (state, _dir) = writable_with(factory(), false);
        let cookie = session_of(&state).await;

        put(
            &state,
            "/api/network-survey",
            &cookie,
            json!({ "authentication": "shared-password" }),
        )
        .await;

        let survey = stored(&state).network.survey.unwrap();
        assert!(survey.authentication.joinable());
        assert!(!survey.registration_required);
        assert!(!survey.fixed_address);
    }

    #[tokio::test]
    async fn the_survey_cannot_be_written_without_a_session() {
        let (state, _dir) = writable();
        let (status, _, _) = send(
            &state,
            "PUT",
            "/api/network-survey",
            None,
            Some(json!({ "authentication": "nothing" }).to_string()),
            10,
        )
        .await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn reopening_the_uplink_question_is_not_the_same_as_going_offline() {
        let (state, _dir) = writable();
        let cookie = session_of(&state).await;

        let (status, body) = put(&state, "/api/uplink", &cookie, serde_json::Value::Null).await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["readiness"]["uplink_decided"], false);
        assert!(stored(&state).network.uplink.is_none());
    }

    #[tokio::test]
    async fn an_unfinished_installation_cannot_be_closed() {
        let (state, _dir) = writable_with(factory(), false);
        let cookie = session_of(&state).await;

        let (status, body) = put(
            &state,
            "/api/installation",
            &cookie,
            json!({ "completed": true }),
        )
        .await;

        assert_eq!(status, StatusCode::CONFLICT);
        assert!(
            body["error"]
                .as_str()
                .unwrap()
                .contains("site identification")
        );
        assert!(!stored(&state).onboarding_completed);
    }

    #[tokio::test]
    async fn a_finished_installation_closes_and_can_be_reopened() {
        let mut config = installed();
        config.onboarding_completed = false;
        let (state, _dir) = writable_with(config, true);
        state.set_stream_running(true);
        let cookie = session_of(&state).await;

        // Recording a session is what finishes the installation; the model is
        // imported later, from the settings.
        post(&state, "/api/calibration", &cookie, json!({})).await;
        send(
            &state,
            "DELETE",
            "/api/calibration",
            Some(&cookie),
            None,
            10,
        )
        .await;

        let (status, body) = put(
            &state,
            "/api/installation",
            &cookie,
            json!({ "completed": true }),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["phase"]["phase"], "operational");
        assert!(stored(&state).onboarding_completed);

        let (status, body) = put(
            &state,
            "/api/installation",
            &cookie,
            json!({ "completed": false }),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["phase"]["phase"], "onboarding");
        assert!(!stored(&state).onboarding_completed);
    }

    #[tokio::test]
    async fn the_onboarding_writes_are_closed_to_anonymous_callers() {
        let (state, _dir) = writable();
        for (path, body) in [
            ("/api/site", json!({ "site_name": "x" })),
            ("/api/nodes", json!([])),
            ("/api/uplink", serde_json::Value::Null),
            ("/api/installation", json!({ "completed": false })),
        ] {
            let (status, _, _) = send(&state, "PUT", path, None, Some(body.to_string()), 10).await;
            assert_eq!(status, StatusCode::UNAUTHORIZED, "{path} is open");
        }
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
    async fn discovery_is_closed_to_anonymous_callers() {
        let state = state_for(installed(), true);
        let (status, _, _) = send(&state, "GET", "/api/discovery", None, None, 10).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn discovery_offers_what_the_intake_has_seen() {
        let state = state_for(installed(), true);
        let cookie = session_of(&state).await;
        state.set_observations(vec![flow_ingest::SenderObservation {
            source: std::net::IpAddr::from([192, 168, 4, 53]),
            tx_macs: vec!["1a:00:00:00:00:00".parse().unwrap()],
            datagrams: 900,
            first_seen_us: now_us() - 9_000_000,
            last_seen_us: now_us(),
            sampled: 32,
        }]);

        let (status, _, body) =
            send(&state, "GET", "/api/discovery", Some(&cookie), None, 10).await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["candidates"][0]["address"], "192.168.4.53");
        // rx-1 is already configured, so the box just plugged in is offered
        // the next free identifier rather than a colliding one.
        assert_eq!(body["proposal"]["receivers"][0]["node_id"], "rx-2");
        assert_eq!(body["proposal"]["tx_mac"], "1a:00:00:00:00:00");
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
