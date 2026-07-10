//! Immutable on-disk storage of capture sessions.
//!
//! A session is one directory in the canonical format (ADR 0001):
//!
//! ```text
//! <root>/<session_id>/
//! ├── meta.json        # SessionMeta
//! ├── csi.ndjson       # one CsiFrame per line, append-ordered by ts_us
//! └── labels.ndjson    # one Label per line, append-ordered by ts_us
//! ```
//!
//! While a capture is running, the directory is named
//! `<session_id>.recording`; [`SessionWriter::finalize`] flushes, syncs,
//! and atomically renames it to `<session_id>`. A crash therefore leaves a
//! clearly marked `.recording` directory — a truncated capture can never
//! be mistaken for a clean one.
//!
//! The writer guards the dataset's invariants: structurally invalid
//! frames, frames from nodes not declared in the session metadata, and
//! out-of-order timestamps are all rejected. Recorded sessions are
//! immutable — creating a session whose directory already exists is an
//! error, never an overwrite.

use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use flow_core::{CsiFrame, FrameError, Label, SessionMeta, TimestampUs};
use thiserror::Error;

/// File name of the session metadata document.
pub const META_FILE: &str = "meta.json";
/// File name of the CSI frame log.
pub const CSI_FILE: &str = "csi.ndjson";
/// File name of the ground-truth label log.
pub const LABELS_FILE: &str = "labels.ndjson";
/// Suffix marking a session directory whose capture is still running
/// (or crashed before finalization).
pub const RECORDING_SUFFIX: &str = ".recording";

/// Failure while creating or writing a session.
#[derive(Debug, Error)]
pub enum SessionError {
    /// The session id is empty or contains characters unsafe for a
    /// directory name.
    #[error("invalid session id: {id:?}")]
    InvalidSessionId {
        /// The offending id.
        id: String,
    },
    /// A directory for this session already exists (recorded sessions are
    /// immutable, and concurrent captures of one id are forbidden).
    #[error("session directory already exists: {}", path.display())]
    AlreadyExists {
        /// The existing path.
        path: PathBuf,
    },
    /// The frame violates the structural invariants of [`CsiFrame`].
    #[error(transparent)]
    Frame(#[from] FrameError),
    /// The frame's node is not declared in the session metadata.
    #[error("frame from node {node_id:?} not declared in session metadata")]
    UnknownNode {
        /// The undeclared node id.
        node_id: String,
    },
    /// A timestamp went backwards within a file (files are append-ordered).
    #[error("out-of-order timestamp: {got} after {last}")]
    OutOfOrder {
        /// Highest timestamp written so far.
        last: TimestampUs,
        /// The offending timestamp.
        got: TimestampUs,
    },
    /// Serialization failure.
    #[error("serialization failed: {0}")]
    Json(#[from] serde_json::Error),
    /// Underlying I/O failure.
    #[error("I/O failure: {0}")]
    Io(#[from] std::io::Error),
}

/// Counts returned by a successful [`SessionWriter::finalize`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionSummary {
    /// Final session directory.
    pub path: PathBuf,
    /// Number of CSI frames written.
    pub frames: u64,
    /// Number of labels written.
    pub labels: u64,
}

/// Writer for one capture session in the canonical on-disk format.
///
/// See the module documentation for the directory layout, the
/// `.recording` convention, and the invariants enforced on writes.
#[derive(Debug)]
pub struct SessionWriter {
    recording_dir: PathBuf,
    final_dir: PathBuf,
    csi: BufWriter<File>,
    labels: BufWriter<File>,
    known_nodes: HashSet<String>,
    last_frame_ts: Option<TimestampUs>,
    last_label_ts: Option<TimestampUs>,
    frames: u64,
    label_count: u64,
}

impl SessionWriter {
    /// Creates the session directory under `root` and writes `meta.json`.
    ///
    /// `root` is created if missing. The session directory itself must not
    /// exist yet, in either recording or finalized form.
    ///
    /// # Errors
    ///
    /// [`SessionError::InvalidSessionId`] for an unsafe id,
    /// [`SessionError::AlreadyExists`] if the session exists, or an I/O
    /// error.
    pub fn create(root: &Path, meta: &SessionMeta) -> Result<Self, SessionError> {
        validate_session_id(&meta.session_id)?;
        let final_dir = root.join(&meta.session_id);
        if final_dir.exists() {
            return Err(SessionError::AlreadyExists { path: final_dir });
        }
        let recording_dir = root.join(format!("{}{RECORDING_SUFFIX}", meta.session_id));
        fs::create_dir_all(root)?;
        if let Err(err) = fs::create_dir(&recording_dir) {
            return if err.kind() == std::io::ErrorKind::AlreadyExists {
                Err(SessionError::AlreadyExists {
                    path: recording_dir,
                })
            } else {
                Err(err.into())
            };
        }

        let meta_json = serde_json::to_vec_pretty(meta)?;
        fs::write(recording_dir.join(META_FILE), meta_json)?;

        let csi = BufWriter::new(File::create(recording_dir.join(CSI_FILE))?);
        let labels = BufWriter::new(File::create(recording_dir.join(LABELS_FILE))?);
        let known_nodes = meta.nodes.iter().map(|node| node.node_id.clone()).collect();

        Ok(Self {
            recording_dir,
            final_dir,
            csi,
            labels,
            known_nodes,
            last_frame_ts: None,
            last_label_ts: None,
            frames: 0,
            label_count: 0,
        })
    }

    /// Appends one CSI frame to `csi.ndjson`.
    ///
    /// # Errors
    ///
    /// Rejects structurally invalid frames, frames from nodes not declared
    /// in the session metadata, and timestamps lower than the previous
    /// frame's.
    pub fn write_frame(&mut self, frame: &CsiFrame) -> Result<(), SessionError> {
        frame.validate()?;
        if !self.known_nodes.contains(&frame.node_id) {
            return Err(SessionError::UnknownNode {
                node_id: frame.node_id.clone(),
            });
        }
        check_order(&mut self.last_frame_ts, frame.ts_us)?;
        serde_json::to_writer(&mut self.csi, frame)?;
        self.csi.write_all(b"\n")?;
        self.frames += 1;
        Ok(())
    }

    /// Appends one ground-truth label to `labels.ndjson`.
    ///
    /// # Errors
    ///
    /// Rejects timestamps lower than the previous label's.
    pub fn write_label(&mut self, label: &Label) -> Result<(), SessionError> {
        check_order(&mut self.last_label_ts, label.ts_us)?;
        serde_json::to_writer(&mut self.labels, label)?;
        self.labels.write_all(b"\n")?;
        self.label_count += 1;
        Ok(())
    }

    /// Flushes and syncs all files, then atomically renames the directory
    /// to its final name, sealing the session.
    ///
    /// # Errors
    ///
    /// Any I/O failure; the directory is left in `.recording` state, so a
    /// failed finalization is indistinguishable from a crash — by design.
    pub fn finalize(self) -> Result<SessionSummary, SessionError> {
        for writer in [self.csi, self.labels] {
            let file = writer.into_inner().map_err(std::io::Error::from)?;
            file.sync_all()?;
        }
        fs::rename(&self.recording_dir, &self.final_dir)?;
        Ok(SessionSummary {
            path: self.final_dir,
            frames: self.frames,
            labels: self.label_count,
        })
    }
}

fn check_order(last: &mut Option<TimestampUs>, ts_us: TimestampUs) -> Result<(), SessionError> {
    if let Some(prev) = *last {
        if ts_us < prev {
            return Err(SessionError::OutOfOrder {
                last: prev,
                got: ts_us,
            });
        }
    }
    *last = Some(ts_us);
    Ok(())
}

fn validate_session_id(id: &str) -> Result<(), SessionError> {
    let valid = !id.is_empty()
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if valid {
        Ok(())
    } else {
        Err(SessionError::InvalidSessionId { id: id.to_owned() })
    }
}

#[cfg(test)]
mod tests {
    use flow_core::{ClassMapping, CsiFrame, DensityClass, NodePlacement, NodeRole};

    use super::*;

    fn meta(session_id: &str) -> SessionMeta {
        SessionMeta {
            session_id: session_id.to_owned(),
            site: "lab-a".into(),
            environment: "test bench".into(),
            wifi_channel: 6,
            nodes: vec![NodePlacement {
                node_id: "rx-1".into(),
                role: NodeRole::Rx,
                position: "desk".into(),
            }],
            firmware_version: "0.1.0".into(),
            software_version: "0.1.0".into(),
            class_mapping: ClassMapping {
                empty: "0".into(),
                low: "1".into(),
                medium: "2".into(),
                saturated: "3+".into(),
            },
        }
    }

    fn frame(ts_us: u64) -> CsiFrame {
        CsiFrame::new(ts_us, "rx-1", -52, 7, vec![1.0, 2.0], vec![0.0, 0.5]).unwrap()
    }

    fn label(ts_us: u64) -> Label {
        Label {
            ts_us,
            class: DensityClass::Low,
            count: Some(1),
        }
    }

    #[test]
    fn full_session_round_trips_through_disk() {
        let root = tempfile::tempdir().unwrap();
        let meta_in = meta("s-001");
        let mut writer = SessionWriter::create(root.path(), &meta_in).unwrap();
        writer.write_frame(&frame(10)).unwrap();
        writer.write_frame(&frame(20)).unwrap();
        writer.write_label(&label(15)).unwrap();
        let summary = writer.finalize().unwrap();

        assert_eq!(summary.frames, 2);
        assert_eq!(summary.labels, 1);
        assert_eq!(summary.path, root.path().join("s-001"));
        assert!(summary.path.is_dir());
        assert!(!root.path().join("s-001.recording").exists());

        let meta_out: SessionMeta =
            serde_json::from_str(&fs::read_to_string(summary.path.join(META_FILE)).unwrap())
                .unwrap();
        assert_eq!(meta_out, meta_in);

        let csi = fs::read_to_string(summary.path.join(CSI_FILE)).unwrap();
        let frames: Vec<CsiFrame> = csi
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert_eq!(frames, [frame(10), frame(20)]);

        let labels = fs::read_to_string(summary.path.join(LABELS_FILE)).unwrap();
        let labels: Vec<Label> = labels
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert_eq!(labels, [label(15)]);
    }

    #[test]
    fn directory_stays_marked_recording_without_finalize() {
        let root = tempfile::tempdir().unwrap();
        let writer = SessionWriter::create(root.path(), &meta("s-002")).unwrap();
        drop(writer);
        assert!(root.path().join("s-002.recording").is_dir());
        assert!(!root.path().join("s-002").exists());
    }

    #[test]
    fn refuses_existing_session() {
        let root = tempfile::tempdir().unwrap();
        let writer = SessionWriter::create(root.path(), &meta("s-003")).unwrap();
        // Same id while recording.
        assert!(matches!(
            SessionWriter::create(root.path(), &meta("s-003")),
            Err(SessionError::AlreadyExists { .. })
        ));
        writer.finalize().unwrap();
        // Same id once finalized: recorded sessions are immutable.
        assert!(matches!(
            SessionWriter::create(root.path(), &meta("s-003")),
            Err(SessionError::AlreadyExists { .. })
        ));
    }

    #[test]
    fn refuses_unsafe_session_ids() {
        let root = tempfile::tempdir().unwrap();
        for id in ["", "a/b", "../escape", "a b", "é"] {
            assert!(
                matches!(
                    SessionWriter::create(root.path(), &meta(id)),
                    Err(SessionError::InvalidSessionId { .. })
                ),
                "id accepted: {id:?}"
            );
        }
    }

    #[test]
    fn refuses_frame_from_undeclared_node() {
        let root = tempfile::tempdir().unwrap();
        let mut writer = SessionWriter::create(root.path(), &meta("s-004")).unwrap();
        let foreign = CsiFrame::new(10, "rx-9", -52, 7, vec![1.0], vec![0.0]).unwrap();
        assert!(matches!(
            writer.write_frame(&foreign),
            Err(SessionError::UnknownNode { node_id }) if node_id == "rx-9"
        ));
    }

    #[test]
    fn refuses_structurally_invalid_frame() {
        let root = tempfile::tempdir().unwrap();
        let mut writer = SessionWriter::create(root.path(), &meta("s-005")).unwrap();
        let mut bad = frame(10);
        bad.len = 99;
        assert!(matches!(
            writer.write_frame(&bad),
            Err(SessionError::Frame(_))
        ));
    }

    #[test]
    fn refuses_out_of_order_timestamps() {
        let root = tempfile::tempdir().unwrap();
        let mut writer = SessionWriter::create(root.path(), &meta("s-006")).unwrap();
        writer.write_frame(&frame(20)).unwrap();
        assert!(matches!(
            writer.write_frame(&frame(10)),
            Err(SessionError::OutOfOrder { last: 20, got: 10 })
        ));
        // Equal timestamps are fine (two nodes, same edge stamp).
        writer.write_frame(&frame(20)).unwrap();

        writer.write_label(&label(5)).unwrap();
        assert!(matches!(
            writer.write_label(&label(4)),
            Err(SessionError::OutOfOrder { last: 5, got: 4 })
        ));
    }

    #[test]
    fn empty_session_finalizes_cleanly() {
        let root = tempfile::tempdir().unwrap();
        let writer = SessionWriter::create(root.path(), &meta("s-007")).unwrap();
        let summary = writer.finalize().unwrap();
        assert_eq!(summary.frames, 0);
        assert_eq!(summary.labels, 0);
        assert!(summary.path.join(CSI_FILE).exists());
    }
}
