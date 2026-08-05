//! The shared state every request reads from and writes through.
//!
//! The single place a change is validated before it is stored, which is what
//! keeps a rejected write from ever reaching the disk.

use std::net::IpAddr;
use std::sync::{Arc, Mutex, MutexGuard};

use serde::Serialize;

use crate::config::ApplianceConfig;
use crate::credential::AdminCredential;
use crate::discovery::Discovery;
use crate::history::MinuteSummary;
use crate::journal::{Event, EventKind, EventPage, Journal, RecordedEvent};
use crate::lifecycle::{Phase, Readiness, Runtime};
use crate::now_us;
use crate::pipeline::StreamHealth;
use crate::schedule::ServiceState;
use crate::session::SessionStore;
use crate::throttle::Throttle;
use crate::views::{
    EstimateView, LiveSnapshot, NodeView, SensorApView, StatusResponse, UplinkView,
};
use flow_core::Label;
use flow_ingest::SenderObservation;
use flow_ingest::SessionWriter;

/// Why a configuration write was refused.
pub enum WriteRejection {
    /// The result would not be a usable configuration. Carries the reason,
    /// which names the field at fault and never a path on the appliance.
    Invalid(String),
    /// A sound change could not be recorded — a read-only card, a full disk.
    Storage,
}

/// The journal refusing to be written, and since when.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct JournalFailure {
    /// When writes started failing.
    pub since_us: u64,
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

/// Why a login was refused.
pub(crate) enum LoginRefusal {
    /// The client is still serving a throttling delay.
    TooManyAttempts { wait_us: u64 },
    /// The secret did not match; a delay may now apply.
    WrongSecret { wait_us: u64 },
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
    pub(crate) fn watch_stopping(&self) -> tokio::sync::watch::Receiver<bool> {
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
    pub(crate) fn watch_estimates(
        &self,
    ) -> tokio::sync::watch::Receiver<Option<flow_infer::WaitEstimate>> {
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
    pub(crate) fn discovery(&self) -> Discovery {
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

    pub(crate) fn minutes(&self, since_us: u64, limit: usize) -> Vec<MinuteSummary> {
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

    pub(crate) fn recent_events(&self, page: &EventPage) -> Vec<RecordedEvent> {
        self.journal().events(page).unwrap_or_default()
    }

    pub(crate) fn newest_event_id(&self) -> Option<i64> {
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
    pub(crate) async fn authenticate(
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
    pub(crate) fn touch_session(&self, token: &str) -> bool {
        let now = now_us();
        self.lock().sessions.touch(token, now)
    }

    /// Ends a session.
    pub(crate) fn close_session(&self, token: &str) {
        self.lock().sessions.close(token);
    }

    pub(crate) fn status(&self) -> StatusResponse {
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

    /// The configuration file this appliance was started with.
    ///
    /// Only the tests need it: they assert on what reached the disk.
    #[cfg(test)]
    pub(crate) fn config_path(&self) -> std::path::PathBuf {
        self.lock().config_path.clone()
    }

    /// Where sessions, models and the journal live.
    pub(crate) fn data_dir(&self) -> std::path::PathBuf {
        self.lock().data_dir.clone()
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

impl EdgeState {
    pub(crate) fn live_snapshot(&self) -> LiveSnapshot {
        LiveSnapshot {
            estimate: self.latest_estimate().map(EstimateView::from),
            stream: self.stream_health(),
            service: self.service_state(),
            now_us: now_us(),
            journal_id: self.newest_event_id(),
        }
    }
}
