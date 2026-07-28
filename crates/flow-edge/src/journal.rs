//! The appliance journal: what happened, and when.
//!
//! Configuration is a file because it is small, current and sometimes has
//! to be read off a card by hand. History is the opposite — it accumulates,
//! it is queried by time, and it has to be pruned — so it lives in SQLite
//! instead, in `appliance.db` next to the rest of the appliance data.
//!
//! # What is recorded
//!
//! Four categories, one table. Access and lifecycle events have emitters
//! today; installation and node events are part of the vocabulary and are
//! written as their features land.
//!
//! Authentication events carry the client address. That is administration
//! data — an installer's phone, a technician's laptop — and is deliberately
//! kept, because a journal that reports an attack without saying where it
//! came from is of little use. It has nothing to do with the people in the
//! monitored queue, of whom nothing is collected, ever. Because an address
//! is personal data, retention is bounded and the network handout says so.
//!
//! # Durability
//!
//! Every committed SQLite transaction costs a physical write, and the
//! deployment target boots from an SD card. The journal therefore splits
//! its traffic:
//!
//! - **access events are committed immediately**, because those are exactly
//!   the ones an attacker would erase by pulling the power;
//! - **everything else is buffered** and written in one transaction a few
//!   seconds later, or when the daemon shuts down.
//!
//! The database runs in WAL mode with `synchronous=FULL`, so a commit
//! really has reached the card when it returns — a weaker setting would
//! make "immediate" a promise the journal could not keep.
//!
//! # Retention
//!
//! Two bounds, because either alone fails: a time window ([`RETENTION_US`],
//! 90 days) covers the need to investigate, and a row cap
//! ([`MAX_EVENTS`]) guarantees that a runaway loop cannot fill the card in
//! an afternoon.

use std::net::IpAddr;
use std::path::Path;

use flow_core::DensityClass;
use rusqlite::{Connection, params};
use serde::Serialize;

use crate::error::JournalError;
use crate::history::MinuteSummary;

/// Events older than this are pruned, in µs (90 days).
pub const RETENTION_US: u64 = 90 * 24 * 60 * 60 * 1_000_000;

/// Hard ceiling on retained events, whatever their age.
pub const MAX_EVENTS: usize = 20_000;

/// File name of the journal inside the data directory.
pub const JOURNAL_FILE: &str = "appliance.db";

/// Buffered events are written once the buffer reaches this size, without
/// waiting for the periodic flush.
const PENDING_LIMIT: usize = 64;

/// Schema revision this build expects.
const SCHEMA_VERSION: i32 = 2;

/// Minute rows older than this are pruned, in µs (two years).
///
/// Long enough to compare a term against the same term a year earlier,
/// which is the question a site manager actually asks. At roughly 70 bytes
/// a row it costs on the order of 75 MB — affordable beside the capture
/// sessions sharing the card.
pub const ESTIMATE_RETENTION_US: u64 = 730 * 24 * 60 * 60 * 1_000_000;

/// Hard ceiling on retained minute rows, whatever their age.
pub const MAX_ESTIMATE_ROWS: usize = 1_500_000;

/// Broad family an event belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum EventCategory {
    /// Who reached the appliance and how it went.
    Access,
    /// The appliance itself: starting, stopping, being reconfigured.
    Lifecycle,
    /// Progress of the guided installation and of calibration.
    Installation,
    /// Sensing nodes coming and going.
    Nodes,
}

/// What happened.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum EventKind {
    /// A client presented the right secret.
    LoginSucceeded,
    /// A client presented a wrong secret.
    LoginFailed,
    /// A client was turned away because it is still being slowed down.
    LoginThrottled,
    /// A session was ended by its holder.
    LoggedOut,
    /// The administrator credential was replaced from the recovery file.
    CredentialReset,
    /// The daemon started serving.
    Started,
    /// The daemon shut down cleanly.
    Stopped,
    /// The appliance configuration was written.
    ConfigurationChanged,
    /// A step of the guided installation was satisfied.
    StageCompleted,
    /// A calibration capture began.
    CalibrationStarted,
    /// A calibration capture ended.
    CalibrationStopped,
    /// A density model became the active one.
    ModelActivated,
    /// A sensing node joined the sensor network.
    NodeAppeared,
    /// A sensing node stopped being seen.
    NodeLost,
}

impl EventKind {
    /// Family this kind belongs to.
    #[must_use]
    pub fn category(self) -> EventCategory {
        match self {
            Self::LoginSucceeded
            | Self::LoginFailed
            | Self::LoginThrottled
            | Self::LoggedOut
            | Self::CredentialReset => EventCategory::Access,
            Self::Started | Self::Stopped | Self::ConfigurationChanged => EventCategory::Lifecycle,
            Self::StageCompleted
            | Self::CalibrationStarted
            | Self::CalibrationStopped
            | Self::ModelActivated => EventCategory::Installation,
            Self::NodeAppeared | Self::NodeLost => EventCategory::Nodes,
        }
    }

    /// Whether this kind must reach the card before the call returns.
    #[must_use]
    pub fn is_immediate(self) -> bool {
        self.category() == EventCategory::Access
    }

    /// Stable name used in the database and on the wire.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::LoginSucceeded => "login-succeeded",
            Self::LoginFailed => "login-failed",
            Self::LoginThrottled => "login-throttled",
            Self::LoggedOut => "logged-out",
            Self::CredentialReset => "credential-reset",
            Self::Started => "started",
            Self::Stopped => "stopped",
            Self::ConfigurationChanged => "configuration-changed",
            Self::StageCompleted => "stage-completed",
            Self::CalibrationStarted => "calibration-started",
            Self::CalibrationStopped => "calibration-stopped",
            Self::ModelActivated => "model-activated",
            Self::NodeAppeared => "node-appeared",
            Self::NodeLost => "node-lost",
        }
    }
}

/// One thing to record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Event {
    /// What happened.
    pub kind: EventKind,
    /// Address of the client involved, for access events.
    pub client: Option<IpAddr>,
    /// Short human-readable context, if any.
    pub detail: Option<String>,
}

impl Event {
    /// An event with no client and no detail.
    #[must_use]
    pub fn new(kind: EventKind) -> Self {
        Self {
            kind,
            client: None,
            detail: None,
        }
    }

    /// Attaches the client address involved.
    #[must_use]
    pub fn from_client(mut self, client: IpAddr) -> Self {
        self.client = Some(client);
        self
    }

    /// Attaches short human-readable context.
    #[must_use]
    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }
}

/// An event as read back out of the journal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RecordedEvent {
    /// When it happened, µs since the Unix epoch, by the edge clock.
    pub ts_us: u64,
    /// Family it belongs to.
    pub category: EventCategory,
    /// What happened.
    pub kind: EventKind,
    /// Client address, when one was involved.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client: Option<String>,
    /// Short human-readable context, when there was any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// The appliance journal.
pub struct Journal {
    connection: Connection,
    pending: Vec<(u64, Event)>,
}

impl Journal {
    /// Opens (creating if needed) the journal in `data_dir`.
    ///
    /// # Errors
    ///
    /// [`JournalError`] if the database cannot be opened or prepared.
    pub fn open(data_dir: &Path) -> Result<Self, JournalError> {
        let connection = Connection::open(data_dir.join(JOURNAL_FILE))?;
        Self::prepare(connection)
    }

    /// Opens a journal that lives only in memory, for tests.
    ///
    /// # Errors
    ///
    /// [`JournalError`] if the database cannot be prepared.
    pub fn open_in_memory() -> Result<Self, JournalError> {
        Self::prepare(Connection::open_in_memory()?)
    }

    fn prepare(connection: Connection) -> Result<Self, JournalError> {
        // WAL keeps readers from blocking the writer; FULL makes a commit
        // mean the data really reached the card, which is what "immediate"
        // has to mean for an access event.
        connection.pragma_update(None, "journal_mode", "WAL")?;
        connection.pragma_update(None, "synchronous", "FULL")?;

        let version: i32 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
        if version < SCHEMA_VERSION {
            connection.execute_batch(
                "CREATE TABLE IF NOT EXISTS events (
                     id        INTEGER PRIMARY KEY AUTOINCREMENT,
                     ts_us     INTEGER NOT NULL,
                     category  TEXT    NOT NULL,
                     kind      TEXT    NOT NULL,
                     client    TEXT,
                     detail    TEXT
                 );
                 CREATE INDEX IF NOT EXISTS events_ts_us ON events(ts_us);

                 -- One row per minute of live estimation. `minute_us` is the
                 -- primary key, so re-writing a minute replaces it rather
                 -- than duplicating it: a restart mid-minute cannot leave two
                 -- rows describing the same sixty seconds.
                 CREATE TABLE IF NOT EXISTS estimates (
                     minute_us        INTEGER PRIMARY KEY,
                     samples          INTEGER NOT NULL,
                     reliable_samples INTEGER NOT NULL,
                     wait_minutes     REAL    NOT NULL,
                     level            REAL    NOT NULL,
                     class            INTEGER NOT NULL,
                     confidence       REAL    NOT NULL
                 );",
            )?;
            connection.pragma_update(None, "user_version", SCHEMA_VERSION)?;
        }

        Ok(Self {
            connection,
            pending: Vec::new(),
        })
    }

    /// Records an event.
    ///
    /// Access events are committed before this returns; everything else is
    /// buffered until [`Journal::flush`] or until the buffer fills.
    ///
    /// # Errors
    ///
    /// [`JournalError`] if the write fails. Buffered events are only ever
    /// written by a later flush, so this cannot fail for them.
    pub fn record(&mut self, event: Event, now_us: u64) -> Result<(), JournalError> {
        let immediate = event.kind.is_immediate();
        self.pending.push((now_us, event));
        if immediate || self.pending.len() >= PENDING_LIMIT {
            self.flush()?;
        }
        Ok(())
    }

    /// Writes every buffered event in one transaction.
    ///
    /// # Errors
    ///
    /// [`JournalError`] if the transaction fails; the buffer is kept so a
    /// later flush can retry.
    pub fn flush(&mut self) -> Result<(), JournalError> {
        if self.pending.is_empty() {
            return Ok(());
        }
        let transaction = self.connection.transaction()?;
        {
            let mut statement = transaction.prepare(
                "INSERT INTO events (ts_us, category, kind, client, detail)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
            )?;
            for (ts_us, event) in &self.pending {
                statement.execute(params![
                    to_sql_us(*ts_us),
                    serde_plain_category(event.kind.category()),
                    event.kind.as_str(),
                    event.client.map(|client| client.to_string()),
                    event.detail.as_deref(),
                ])?;
            }
        }
        transaction.commit()?;
        self.pending.clear();
        Ok(())
    }

    /// Drops events past the retention window or the row cap.
    ///
    /// Returns how many were removed.
    ///
    /// # Errors
    ///
    /// [`JournalError`] if the deletion fails.
    pub fn prune(&mut self, now_us: u64) -> Result<usize, JournalError> {
        let cutoff = to_sql_us(now_us.saturating_sub(RETENTION_US));
        let mut removed = self
            .connection
            .execute("DELETE FROM events WHERE ts_us < ?1", params![cutoff])?;

        // Then the ceiling: keep the newest MAX_EVENTS rows, whatever their
        // age. Ordering by id rather than by timestamp keeps this correct
        // even if the clock were to step backwards.
        let cap = i64::try_from(MAX_EVENTS).unwrap_or(i64::MAX);
        removed += self.connection.execute(
            "DELETE FROM events WHERE id NOT IN
                 (SELECT id FROM events ORDER BY id DESC LIMIT ?1)",
            params![cap],
        )?;

        // The minute series is bounded the same way, on its own horizon: it
        // is answered by year-on-year questions, so it is kept far longer
        // than the event log.
        let estimate_cutoff = to_sql_us(now_us.saturating_sub(ESTIMATE_RETENTION_US));
        removed += self.connection.execute(
            "DELETE FROM estimates WHERE minute_us < ?1",
            params![estimate_cutoff],
        )?;
        let estimate_cap = i64::try_from(MAX_ESTIMATE_ROWS).unwrap_or(i64::MAX);
        removed += self.connection.execute(
            "DELETE FROM estimates WHERE minute_us NOT IN
                 (SELECT minute_us FROM estimates ORDER BY minute_us DESC LIMIT ?1)",
            params![estimate_cap],
        )?;
        Ok(removed)
    }

    /// The most recent events, newest first.
    ///
    /// # Errors
    ///
    /// [`JournalError`] if the query fails.
    pub fn recent(&self, limit: usize) -> Result<Vec<RecordedEvent>, JournalError> {
        let mut statement = self.connection.prepare(
            "SELECT ts_us, category, kind, client, detail
             FROM events ORDER BY id DESC LIMIT ?1",
        )?;
        let limit = i64::try_from(limit).unwrap_or(i64::MAX);
        let rows = statement.query_map(params![limit], |row| {
            let category: String = row.get(1)?;
            let kind: String = row.get(2)?;
            let ts_us: i64 = row.get(0)?;
            Ok(RecordedEvent {
                ts_us: from_sql_us(ts_us),
                category: parse_category(&category),
                kind: parse_kind(&kind),
                client: row.get(3)?,
                detail: row.get(4)?,
            })
        })?;
        let mut events = Vec::new();
        for row in rows {
            events.push(row?);
        }
        Ok(events)
    }

    /// Stores one folded minute of estimates.
    ///
    /// Written as it closes rather than buffered: one transaction a minute
    /// is negligible against the card's endurance, and buffering would risk
    /// losing the minute a power cut interrupts for no gain.
    ///
    /// # Errors
    ///
    /// [`JournalError`] if the write fails.
    pub fn write_minute(&mut self, summary: &MinuteSummary) -> Result<(), JournalError> {
        self.connection.execute(
            "INSERT OR REPLACE INTO estimates
                 (minute_us, samples, reliable_samples, wait_minutes, level, class, confidence)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                to_sql_us(summary.minute_us),
                summary.samples,
                summary.reliable_samples,
                f64::from(summary.wait_minutes),
                f64::from(summary.level),
                i64::from(summary.class.as_u8()),
                f64::from(summary.confidence),
            ],
        )?;
        Ok(())
    }

    /// The most recent minutes at or after `since_us`, oldest first.
    ///
    /// The limit keeps the *newest* rows, then hands them back in
    /// chronological order: a caller asking for "the last hour" wants the
    /// last hour, and wants to plot it left to right.
    ///
    /// # Errors
    ///
    /// [`JournalError`] if the query fails.
    pub fn minutes(&self, since_us: u64, limit: usize) -> Result<Vec<MinuteSummary>, JournalError> {
        let mut statement = self.connection.prepare(
            "SELECT minute_us, samples, reliable_samples, wait_minutes, level, class, confidence
             FROM (
                 SELECT * FROM estimates WHERE minute_us >= ?1 ORDER BY minute_us DESC LIMIT ?2
             )
             ORDER BY minute_us ASC",
        )?;
        let rows = statement.query_map(
            params![
                to_sql_us(since_us),
                i64::try_from(limit).unwrap_or(i64::MAX)
            ],
            |row| {
                let minute_us: i64 = row.get(0)?;
                let class: i64 = row.get(5)?;
                let wait: f64 = row.get(3)?;
                let level: f64 = row.get(4)?;
                let confidence: f64 = row.get(6)?;
                Ok(MinuteSummary {
                    minute_us: from_sql_us(minute_us),
                    samples: row.get(1)?,
                    reliable_samples: row.get(2)?,
                    wait_minutes: wait as f32,
                    level: level as f32,
                    class: DensityClass::try_from(u8::try_from(class).unwrap_or(0))
                        .unwrap_or(DensityClass::Empty),
                    confidence: confidence as f32,
                })
            },
        )?;
        let mut minutes = Vec::new();
        for row in rows {
            minutes.push(row?);
        }
        Ok(minutes)
    }

    /// Minute rows stored.
    ///
    /// # Errors
    ///
    /// [`JournalError`] if the query fails.
    pub fn minute_count(&self) -> Result<usize, JournalError> {
        let count: i64 =
            self.connection
                .query_row("SELECT COUNT(*) FROM estimates", [], |row| row.get(0))?;
        Ok(usize::try_from(count).unwrap_or(0))
    }

    /// Events written but not yet counted by [`Journal::recent`].
    #[must_use]
    pub fn pending(&self) -> usize {
        self.pending.len()
    }

    /// Total events stored.
    ///
    /// # Errors
    ///
    /// [`JournalError`] if the query fails.
    pub fn count(&self) -> Result<usize, JournalError> {
        let count: i64 = self
            .connection
            .query_row("SELECT COUNT(*) FROM events", [], |row| row.get(0))?;
        Ok(usize::try_from(count).unwrap_or(0))
    }
}

/// SQLite stores signed 64-bit integers; microsecond timestamps fit with
/// room to spare (i64::MAX is year 294247), so the conversion is total in
/// practice and saturates rather than panicking if it ever were not.
fn to_sql_us(us: u64) -> i64 {
    i64::try_from(us).unwrap_or(i64::MAX)
}

fn from_sql_us(value: i64) -> u64 {
    u64::try_from(value).unwrap_or(0)
}

fn serde_plain_category(category: EventCategory) -> &'static str {
    match category {
        EventCategory::Access => "access",
        EventCategory::Lifecycle => "lifecycle",
        EventCategory::Installation => "installation",
        EventCategory::Nodes => "nodes",
    }
}

fn parse_category(text: &str) -> EventCategory {
    match text {
        "lifecycle" => EventCategory::Lifecycle,
        "installation" => EventCategory::Installation,
        "nodes" => EventCategory::Nodes,
        _ => EventCategory::Access,
    }
}

fn parse_kind(text: &str) -> EventKind {
    match text {
        "login-failed" => EventKind::LoginFailed,
        "login-throttled" => EventKind::LoginThrottled,
        "logged-out" => EventKind::LoggedOut,
        "credential-reset" => EventKind::CredentialReset,
        "started" => EventKind::Started,
        "stopped" => EventKind::Stopped,
        "configuration-changed" => EventKind::ConfigurationChanged,
        "stage-completed" => EventKind::StageCompleted,
        "calibration-started" => EventKind::CalibrationStarted,
        "calibration-stopped" => EventKind::CalibrationStopped,
        "model-activated" => EventKind::ModelActivated,
        "node-appeared" => EventKind::NodeAppeared,
        "node-lost" => EventKind::NodeLost,
        _ => EventKind::LoginSucceeded,
    }
}

#[cfg(test)]
mod tests {
    use std::net::Ipv4Addr;

    use super::*;

    const NOW: u64 = 1_800_000_000_000_000;
    const DAY: u64 = 24 * 60 * 60 * 1_000_000;

    fn client() -> IpAddr {
        IpAddr::V4(Ipv4Addr::new(192, 168, 4, 10))
    }

    fn summary(minute_us: u64, class: DensityClass, samples: u32) -> MinuteSummary {
        MinuteSummary {
            minute_us,
            samples,
            reliable_samples: samples,
            wait_minutes: 6.5,
            level: 2.0,
            class,
            confidence: 0.8,
        }
    }

    #[test]
    fn a_minute_round_trips_through_storage() {
        let mut journal = Journal::open_in_memory().unwrap();
        let written = summary(NOW, DensityClass::Medium, 58);
        journal.write_minute(&written).unwrap();

        let read = journal.minutes(0, 10).unwrap();
        assert_eq!(read.len(), 1);
        assert_eq!(read[0].minute_us, written.minute_us);
        assert_eq!(read[0].class, DensityClass::Medium);
        assert_eq!(read[0].samples, 58);
        assert!((read[0].wait_minutes - 6.5).abs() < 1e-5);
        assert!(read[0].is_reliable());
    }

    #[test]
    fn rewriting_a_minute_replaces_it_rather_than_duplicating() {
        // A restart mid-minute must not leave two rows for one minute.
        let mut journal = Journal::open_in_memory().unwrap();
        journal
            .write_minute(&summary(NOW, DensityClass::Low, 10))
            .unwrap();
        journal
            .write_minute(&summary(NOW, DensityClass::Saturated, 60))
            .unwrap();

        let read = journal.minutes(0, 10).unwrap();
        assert_eq!(read.len(), 1);
        assert_eq!(
            read[0].class,
            DensityClass::Saturated,
            "the later write wins"
        );
        assert_eq!(journal.minute_count().unwrap(), 1);
    }

    #[test]
    fn minutes_come_back_in_chronological_order() {
        let mut journal = Journal::open_in_memory().unwrap();
        for step in 0..5u64 {
            journal
                .write_minute(&summary(NOW + step * 60_000_000, DensityClass::Low, 60))
                .unwrap();
        }
        let read = journal.minutes(0, 10).unwrap();
        let order: Vec<u64> = read.iter().map(|m| m.minute_us).collect();
        let mut sorted = order.clone();
        sorted.sort_unstable();
        assert_eq!(order, sorted, "a caller plots these left to right");
    }

    #[test]
    fn a_limit_keeps_the_newest_minutes_still_in_order() {
        let mut journal = Journal::open_in_memory().unwrap();
        for step in 0..10u64 {
            journal
                .write_minute(&summary(NOW + step * 60_000_000, DensityClass::Low, 60))
                .unwrap();
        }
        let read = journal.minutes(0, 3).unwrap();
        assert_eq!(read.len(), 3);
        assert_eq!(read[0].minute_us, NOW + 7 * 60_000_000, "the newest three");
        assert_eq!(read[2].minute_us, NOW + 9 * 60_000_000);
    }

    #[test]
    fn the_since_bound_excludes_older_minutes() {
        let mut journal = Journal::open_in_memory().unwrap();
        for step in 0..5u64 {
            journal
                .write_minute(&summary(NOW + step * 60_000_000, DensityClass::Low, 60))
                .unwrap();
        }
        let read = journal.minutes(NOW + 3 * 60_000_000, 10).unwrap();
        assert_eq!(read.len(), 2);
        assert_eq!(read[0].minute_us, NOW + 3 * 60_000_000);
    }

    #[test]
    fn minutes_past_their_horizon_are_pruned() {
        let mut journal = Journal::open_in_memory().unwrap();
        journal
            .write_minute(&summary(NOW, DensityClass::Low, 60))
            .unwrap();
        journal
            .write_minute(&summary(NOW + ESTIMATE_RETENTION_US, DensityClass::Low, 60))
            .unwrap();

        journal.prune(NOW + ESTIMATE_RETENTION_US + DAY).unwrap();
        assert_eq!(journal.minute_count().unwrap(), 1);
        assert_eq!(
            journal.minutes(0, 10).unwrap()[0].minute_us,
            NOW + ESTIMATE_RETENTION_US
        );
    }

    #[test]
    fn pruning_events_leaves_the_minute_series_alone() {
        // The two series have different horizons; one purge must not take
        // the other with it.
        let mut journal = Journal::open_in_memory().unwrap();
        journal.record(Event::new(EventKind::Started), NOW).unwrap();
        journal.flush().unwrap();
        journal
            .write_minute(&summary(NOW, DensityClass::Low, 60))
            .unwrap();

        journal.prune(NOW + RETENTION_US + DAY).unwrap();
        assert_eq!(journal.count().unwrap(), 0, "the old event went");
        assert_eq!(journal.minute_count().unwrap(), 1, "the minute stayed");
    }

    #[test]
    fn the_minute_series_survives_a_reopen() {
        let dir = tempfile::tempdir().unwrap();
        {
            let mut journal = Journal::open(dir.path()).unwrap();
            journal
                .write_minute(&summary(NOW, DensityClass::Medium, 60))
                .unwrap();
        }
        let journal = Journal::open(dir.path()).unwrap();
        assert_eq!(
            journal.minutes(0, 10).unwrap()[0].class,
            DensityClass::Medium
        );
    }

    #[test]
    fn every_kind_maps_to_a_distinct_name_and_back() {
        let kinds = [
            EventKind::LoginSucceeded,
            EventKind::LoginFailed,
            EventKind::LoginThrottled,
            EventKind::LoggedOut,
            EventKind::CredentialReset,
            EventKind::Started,
            EventKind::Stopped,
            EventKind::ConfigurationChanged,
            EventKind::StageCompleted,
            EventKind::CalibrationStarted,
            EventKind::CalibrationStopped,
            EventKind::ModelActivated,
            EventKind::NodeAppeared,
            EventKind::NodeLost,
        ];
        let names: std::collections::HashSet<&str> =
            kinds.iter().map(|kind| kind.as_str()).collect();
        assert_eq!(names.len(), kinds.len(), "names must be unique");
        for kind in kinds {
            assert_eq!(parse_kind(kind.as_str()), kind, "{kind:?} must round-trip");
            assert_eq!(
                parse_category(serde_plain_category(kind.category())),
                kind.category()
            );
        }
    }

    #[test]
    fn access_events_are_the_immediate_ones() {
        assert!(EventKind::LoginFailed.is_immediate());
        assert!(EventKind::CredentialReset.is_immediate());
        assert!(!EventKind::Started.is_immediate());
        assert!(!EventKind::NodeLost.is_immediate());
    }

    #[test]
    fn an_access_event_is_written_before_the_call_returns() {
        let mut journal = Journal::open_in_memory().unwrap();
        journal
            .record(
                Event::new(EventKind::LoginFailed).from_client(client()),
                NOW,
            )
            .unwrap();

        assert_eq!(journal.pending(), 0, "nothing left buffered");
        assert_eq!(journal.count().unwrap(), 1);
        let recorded = &journal.recent(10).unwrap()[0];
        assert_eq!(recorded.kind, EventKind::LoginFailed);
        assert_eq!(recorded.category, EventCategory::Access);
        assert_eq!(recorded.client.as_deref(), Some("192.168.4.10"));
        assert_eq!(recorded.ts_us, NOW);
    }

    #[test]
    fn other_events_wait_for_a_flush() {
        let mut journal = Journal::open_in_memory().unwrap();
        journal.record(Event::new(EventKind::Started), NOW).unwrap();

        assert_eq!(journal.pending(), 1);
        assert_eq!(journal.count().unwrap(), 0, "not on disk yet");

        journal.flush().unwrap();
        assert_eq!(journal.pending(), 0);
        assert_eq!(journal.count().unwrap(), 1);
    }

    #[test]
    fn an_access_event_carries_buffered_ones_with_it() {
        let mut journal = Journal::open_in_memory().unwrap();
        journal.record(Event::new(EventKind::Started), NOW).unwrap();
        journal
            .record(Event::new(EventKind::NodeAppeared), NOW + 1)
            .unwrap();
        assert_eq!(journal.count().unwrap(), 0);

        journal
            .record(
                Event::new(EventKind::LoginSucceeded).from_client(client()),
                NOW + 2,
            )
            .unwrap();
        assert_eq!(
            journal.count().unwrap(),
            3,
            "the immediate write commits what was waiting"
        );
    }

    #[test]
    fn a_full_buffer_writes_itself_without_waiting() {
        let mut journal = Journal::open_in_memory().unwrap();
        for offset in 0..PENDING_LIMIT as u64 {
            journal
                .record(Event::new(EventKind::NodeAppeared), NOW + offset)
                .unwrap();
        }
        assert_eq!(journal.pending(), 0);
        assert_eq!(journal.count().unwrap(), PENDING_LIMIT);
    }

    #[test]
    fn flushing_an_empty_buffer_is_free() {
        let mut journal = Journal::open_in_memory().unwrap();
        journal.flush().unwrap();
        assert_eq!(journal.count().unwrap(), 0);
    }

    #[test]
    fn events_come_back_newest_first_with_their_detail() {
        let mut journal = Journal::open_in_memory().unwrap();
        for offset in 0..5 {
            journal
                .record(
                    Event::new(EventKind::Started).with_detail(format!("boot {offset}")),
                    NOW + offset,
                )
                .unwrap();
        }
        journal.flush().unwrap();

        let recent = journal.recent(3).unwrap();
        assert_eq!(recent.len(), 3, "the limit is honoured");
        assert_eq!(recent[0].detail.as_deref(), Some("boot 4"));
        assert_eq!(recent[2].detail.as_deref(), Some("boot 2"));
    }

    #[test]
    fn an_event_without_a_client_stores_none() {
        let mut journal = Journal::open_in_memory().unwrap();
        journal.record(Event::new(EventKind::Started), NOW).unwrap();
        journal.flush().unwrap();
        assert_eq!(journal.recent(1).unwrap()[0].client, None);
    }

    #[test]
    fn events_past_the_retention_window_are_pruned() {
        let mut journal = Journal::open_in_memory().unwrap();
        journal
            .record(Event::new(EventKind::Started).with_detail("old"), NOW)
            .unwrap();
        journal
            .record(
                Event::new(EventKind::Started).with_detail("recent"),
                NOW + RETENTION_US,
            )
            .unwrap();
        journal.flush().unwrap();

        let removed = journal.prune(NOW + RETENTION_US + DAY).unwrap();
        assert_eq!(removed, 1);
        let remaining = journal.recent(10).unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].detail.as_deref(), Some("recent"));
    }

    #[test]
    fn the_row_cap_holds_even_when_nothing_is_old() {
        let mut journal = Journal::open_in_memory().unwrap();
        for offset in 0..(MAX_EVENTS as u64 + 50) {
            journal
                .record(Event::new(EventKind::NodeAppeared), NOW + offset)
                .unwrap();
        }
        journal.flush().unwrap();
        assert!(journal.count().unwrap() > MAX_EVENTS);

        journal.prune(NOW).unwrap();
        assert_eq!(journal.count().unwrap(), MAX_EVENTS);
        // The newest survive.
        assert_eq!(
            journal.recent(1).unwrap()[0].ts_us,
            NOW + MAX_EVENTS as u64 + 49
        );
    }

    #[test]
    fn pruning_an_empty_journal_removes_nothing() {
        let mut journal = Journal::open_in_memory().unwrap();
        assert_eq!(journal.prune(NOW).unwrap(), 0);
    }

    #[test]
    fn a_journal_survives_being_closed_and_reopened() {
        let dir = tempfile::tempdir().unwrap();
        {
            let mut journal = Journal::open(dir.path()).unwrap();
            journal
                .record(
                    Event::new(EventKind::LoginSucceeded).from_client(client()),
                    NOW,
                )
                .unwrap();
        }
        let journal = Journal::open(dir.path()).unwrap();
        assert_eq!(journal.count().unwrap(), 1);
        assert_eq!(
            journal.recent(1).unwrap()[0].kind,
            EventKind::LoginSucceeded
        );
    }

    #[test]
    fn buffered_events_are_lost_if_the_daemon_never_flushes() {
        // Documents the accepted trade-off: only access events are promised
        // to survive an abrupt power cut.
        let dir = tempfile::tempdir().unwrap();
        {
            let mut journal = Journal::open(dir.path()).unwrap();
            journal.record(Event::new(EventKind::Started), NOW).unwrap();
            assert_eq!(journal.pending(), 1);
        }
        assert_eq!(Journal::open(dir.path()).unwrap().count().unwrap(), 0);
    }
}
