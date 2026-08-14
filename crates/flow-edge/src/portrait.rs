//! What the appliance can say about a capture it recorded (ADR 0025).
//!
//! Computed in one pass and cached beside the session, the cost being the
//! parsing of tens of megabytes of NDJSON rather than the arithmetic. Nothing
//! here is ever held whole in memory: frames arrive one at a time and land in
//! fixed-size accumulators.

use std::collections::{BTreeMap, VecDeque};
use std::path::{Path, PathBuf};

use flow_core::{CsiFrame, Label, TimestampUs};
use flow_infer::{NODE_FEATURES, node_features};
use flow_ingest::{SessionError, SessionReader};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::config::{DEFAULT_HOP_US, DEFAULT_WINDOW_US};

/// The readable half of a portrait, cached beside its session.
pub const PORTRAIT_FILE: &str = "portrait.json";

/// The heatmap pixels, cached beside the readable half.
///
/// Kept out of the JSON so it stays readable with `curl`, and so the appliance
/// never builds a base64 string of them in memory.
pub const HEATMAP_FILE: &str = "heatmap.bin";

/// Version of the cached payload.
///
/// A cache written by an older build is recomputed rather than read, which is
/// cheaper than migrating a file that can always be produced again.
const PORTRAIT_SCHEMA: u32 = 1;

/// Columns the heatmap is reduced to.
///
/// Chosen against a display rather than against the data: a capture holds far
/// more frames than any screen has pixels.
const MAX_BINS: usize = 720;

/// Bin width the accumulator starts at, before the capture proves longer.
const INITIAL_BIN_US: u64 = 250_000;

/// Silence longer than this counts as a hole rather than as jitter.
const GAP_US: u64 = 1_000_000;

/// Reserved for a bin no frame landed in, so a hole reads as a hole rather
/// than as an amplitude of zero.
const NO_DATA: u8 = 0;

/// Failure to build a portrait.
#[derive(Debug, Error)]
pub enum PortraitError {
    /// The session could not be read.
    #[error(transparent)]
    Session(#[from] SessionError),
    /// The cache could not be written.
    #[error("could not write the portrait: {0}")]
    Cache(String),
}

/// One receiver's account of a capture.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NodePortrait {
    pub node_id: String,
    pub frames: u64,
    /// Frames per second over the span it was heard on.
    pub rate_hz: f64,
    /// Subcarriers per frame, from the first frame the node sent.
    pub subcarriers: usize,
    /// Frames dropped because they disagreed with that width.
    pub inconsistent: u64,
    /// Amplitude the heatmap's scale runs between.
    pub amp_min: f32,
    pub amp_max: f32,
    /// Silences longer than a second, as `[from_us, to_us]`.
    pub gaps: Vec<[TimestampUs; 2]>,
    /// The v1 features over time, one series per name.
    ///
    /// A window too sparse to measure holds `NaN`, which serialises as `null`
    /// — a hole in the series rather than a value of zero, and what training
    /// does with such a window too.
    pub features: BTreeMap<String, Vec<f32>>,
}

/// Everything the appliance can say about one capture.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Portrait {
    pub schema: u32,
    pub session_id: String,
    pub computed_at_us: u64,
    /// Span the capture covers, from its first frame to its last.
    pub started_at_us: TimestampUs,
    pub ended_at_us: TimestampUs,
    /// Width of one heatmap column, in µs.
    pub bin_us: u64,
    /// Columns each node's heatmap holds.
    pub bins: usize,
    /// Geometry the features were computed under.
    pub window_us: u64,
    pub hop_us: u64,
    /// Instants the feature series are sampled at.
    pub feature_ts_us: Vec<TimestampUs>,
    /// Set when a line could not be read, which is what a crashed capture
    /// leaves behind. Everything before it is still described.
    pub truncated: bool,
    pub nodes: Vec<NodePortrait>,
    pub labels: Vec<Label>,
}

/// Reads a cached portrait, or nothing if it is absent or from another build.
#[must_use]
pub fn cached(session_dir: &Path) -> Option<Portrait> {
    let bytes = std::fs::read(session_dir.join(PORTRAIT_FILE)).ok()?;
    let portrait: Portrait = serde_json::from_slice(&bytes).ok()?;
    (portrait.schema == PORTRAIT_SCHEMA).then_some(portrait)
}

/// Where the heatmap pixels of a computed portrait sit.
#[must_use]
pub fn heatmap_path(session_dir: &Path) -> PathBuf {
    session_dir.join(HEATMAP_FILE)
}

/// Builds a portrait and caches it beside the session.
///
/// # Errors
///
/// [`PortraitError`] if the session cannot be read or the cache written.
pub fn compute(session_dir: &Path, now_us: u64) -> Result<Portrait, PortraitError> {
    let reader = SessionReader::open(session_dir)?;
    let rx_nodes = reader.rx_node_ids();
    let labels = reader.labels().unwrap_or_default();

    let mut run = Run::new(&rx_nodes);
    let mut truncated = false;
    for outcome in reader.frames()? {
        match outcome {
            Ok(frame) => run.push(&frame),
            // A capture killed mid-write ends in half a line. Everything
            // before it was written and flushed, so it is still worth showing.
            Err(_) => {
                truncated = true;
                break;
            }
        }
    }

    let (nodes, pixels) = run.finish();
    let portrait = Portrait {
        schema: PORTRAIT_SCHEMA,
        session_id: reader.meta().session_id.clone(),
        computed_at_us: now_us,
        started_at_us: run.first_us.unwrap_or(0),
        ended_at_us: run.last_us.unwrap_or(0),
        bin_us: run.bin_us,
        bins: run.bins,
        window_us: DEFAULT_WINDOW_US,
        hop_us: DEFAULT_HOP_US,
        feature_ts_us: run.feature_ts_us,
        truncated,
        nodes,
        labels,
    };

    // The pixels land first: a JSON present without them would have the
    // reader ask for a file that is not there yet.
    std::fs::write(heatmap_path(session_dir), &pixels)
        .and_then(|()| {
            std::fs::write(
                session_dir.join(PORTRAIT_FILE),
                serde_json::to_vec(&portrait)?,
            )
        })
        .map_err(|err| PortraitError::Cache(err.to_string()))?;
    Ok(portrait)
}

/// One pass over a capture's frames.
struct Run {
    bin_us: u64,
    bins: usize,
    first_us: Option<TimestampUs>,
    last_us: Option<TimestampUs>,
    next_emit_us: Option<TimestampUs>,
    feature_ts_us: Vec<TimestampUs>,
    nodes: Vec<NodeRun>,
}

/// What one receiver accumulates.
struct NodeRun {
    node_id: String,
    frames: u64,
    inconsistent: u64,
    subcarriers: usize,
    first_us: Option<TimestampUs>,
    last_us: Option<TimestampUs>,
    prev_us: Option<TimestampUs>,
    gaps: Vec<[TimestampUs; 2]>,
    /// Amplitude sums per `bin × subcarrier`, and frames per bin.
    sums: Vec<f64>,
    counts: Vec<u32>,
    window: VecDeque<CsiFrame>,
    features: Vec<[f64; 7]>,
}

impl Run {
    fn new(rx_nodes: &[String]) -> Self {
        Self {
            bin_us: INITIAL_BIN_US,
            bins: 0,
            first_us: None,
            last_us: None,
            next_emit_us: None,
            feature_ts_us: Vec::new(),
            nodes: rx_nodes.iter().map(|id| NodeRun::new(id.clone())).collect(),
        }
    }

    fn push(&mut self, frame: &CsiFrame) {
        let Some(index) = self.nodes.iter().position(|n| n.node_id == frame.node_id) else {
            return;
        };
        let start = *self.first_us.get_or_insert(frame.ts_us);
        self.last_us = Some(frame.ts_us);
        self.next_emit_us
            .get_or_insert(start.saturating_add(DEFAULT_WINDOW_US));

        let elapsed = frame.ts_us.saturating_sub(start);
        let mut bin = usize::try_from(elapsed / self.bin_us).unwrap_or(usize::MAX);
        while bin >= MAX_BINS {
            self.halve();
            bin = usize::try_from(elapsed / self.bin_us).unwrap_or(usize::MAX);
        }
        self.bins = self.bins.max(bin + 1);

        self.nodes[index].push(frame, bin);
        self.emit_due(frame.ts_us);
    }

    /// Halves the resolution so a long capture stays within `MAX_BINS`.
    ///
    /// Bins hold sums rather than means precisely so that merging two of them
    /// is an addition, with no weighting to get wrong.
    fn halve(&mut self) {
        for node in &mut self.nodes {
            node.halve();
        }
        self.bin_us *= 2;
        self.bins = self.bins.div_ceil(2);
    }

    /// Emits every feature sample the stream has now run past.
    fn emit_due(&mut self, now_us: TimestampUs) {
        while let Some(at) = self.next_emit_us {
            if now_us < at {
                break;
            }
            let from = at.saturating_sub(DEFAULT_WINDOW_US);
            for node in &mut self.nodes {
                node.emit(from, at);
            }
            self.feature_ts_us.push(at);
            self.next_emit_us = Some(at.saturating_add(DEFAULT_HOP_US));
        }
    }

    fn finish(&mut self) -> (Vec<NodePortrait>, Vec<u8>) {
        let (bins, span) = (self.bins, self.span());
        let mut pixels = Vec::with_capacity(bins * self.nodes.len() * 64);
        let portraits = self
            .nodes
            .iter_mut()
            .map(|node| node.finish(bins, span, &mut pixels))
            .collect();
        (portraits, pixels)
    }

    /// Seconds the capture spans, or none when it holds a single instant.
    fn span(&self) -> Option<f64> {
        let (first, last) = (self.first_us?, self.last_us?);
        let seconds = last.saturating_sub(first) as f64 / 1e6;
        (seconds > 0.0).then_some(seconds)
    }
}

impl NodeRun {
    fn new(node_id: String) -> Self {
        Self {
            node_id,
            frames: 0,
            inconsistent: 0,
            subcarriers: 0,
            first_us: None,
            last_us: None,
            prev_us: None,
            gaps: Vec::new(),
            sums: Vec::new(),
            counts: Vec::new(),
            window: VecDeque::new(),
            features: Vec::new(),
        }
    }

    fn push(&mut self, frame: &CsiFrame, bin: usize) {
        if self.subcarriers == 0 {
            self.subcarriers = frame.amp.len();
        }
        if frame.amp.len() != self.subcarriers {
            self.inconsistent += 1;
            return;
        }

        self.frames += 1;
        self.first_us.get_or_insert(frame.ts_us);
        self.last_us = Some(frame.ts_us);
        if let Some(prev) = self.prev_us {
            if frame.ts_us.saturating_sub(prev) > GAP_US {
                self.gaps.push([prev, frame.ts_us]);
            }
        }
        self.prev_us = Some(frame.ts_us);

        let width = self.subcarriers;
        if self.counts.len() <= bin {
            self.counts.resize(bin + 1, 0);
            self.sums.resize((bin + 1) * width, 0.0);
        }
        self.counts[bin] += 1;
        for (slot, value) in self.sums[bin * width..(bin + 1) * width]
            .iter_mut()
            .zip(&frame.amp)
        {
            *slot += f64::from(*value);
        }

        self.window.push_back(frame.clone());
    }

    fn halve(&mut self) {
        let width = self.subcarriers.max(1);
        let merged = self.counts.len().div_ceil(2);
        let mut counts = vec![0u32; merged];
        let mut sums = vec![0.0f64; merged * width];
        for (index, count) in self.counts.iter().enumerate() {
            counts[index / 2] += count;
            let (from, to) = (index * width, (index / 2) * width);
            for offset in 0..width.min(self.sums.len().saturating_sub(from)) {
                sums[to + offset] += self.sums[from + offset];
            }
        }
        self.counts = counts;
        self.sums = sums;
    }

    /// Computes one feature sample over `(from, at]`, dropping what fell out.
    fn emit(&mut self, from: TimestampUs, at: TimestampUs) {
        while self.window.front().is_some_and(|f| f.ts_us <= from) {
            self.window.pop_front();
        }
        let duration = at.saturating_sub(from);
        let vector = node_features(self.window.make_contiguous(), duration)
            // An incomplete window is dropped by training too, so reporting
            // NaN keeps the series aligned with what a model would be shown.
            .unwrap_or([f64::NAN; 7]);
        self.features.push(vector);
    }

    fn finish(&mut self, bins: usize, span: Option<f64>, pixels: &mut Vec<u8>) -> NodePortrait {
        let width = self.subcarriers;
        self.counts.resize(bins, 0);
        self.sums.resize(bins * width, 0.0);

        let means: Vec<Option<f32>> = (0..bins)
            .flat_map(|bin| {
                let count = f64::from(self.counts[bin]);
                (0..width).map(move |s| (bin, s, count))
            })
            .map(|(bin, s, count)| {
                (count > 0.0).then(|| (self.sums[bin * width + s] / count) as f32)
            })
            .collect();

        let (low, high) = extremes(&means);
        pixels.extend(means.iter().map(|mean| quantise(*mean, low, high)));

        let seconds = self
            .first_us
            .zip(self.last_us)
            .map(|(a, b)| b.saturating_sub(a) as f64 / 1e6)
            .filter(|s| *s > 0.0)
            .or(span);

        NodePortrait {
            node_id: self.node_id.clone(),
            frames: self.frames,
            rate_hz: seconds.map_or(0.0, |s| self.frames as f64 / s),
            subcarriers: width,
            inconsistent: self.inconsistent,
            amp_min: low,
            amp_max: high,
            gaps: std::mem::take(&mut self.gaps),
            features: series(&self.features),
        }
    }
}

/// The amplitude range the heatmap's scale runs between.
fn extremes(means: &[Option<f32>]) -> (f32, f32) {
    let mut low = f32::INFINITY;
    let mut high = f32::NEG_INFINITY;
    for value in means.iter().flatten() {
        low = low.min(*value);
        high = high.max(*value);
    }
    if low.is_finite() {
        (low, high)
    } else {
        (0.0, 0.0)
    }
}

/// One amplitude on the byte scale the browser paints from.
fn quantise(mean: Option<f32>, low: f32, high: f32) -> u8 {
    let Some(mean) = mean else {
        return NO_DATA;
    };
    if high <= low {
        return 128;
    }
    let scaled = (mean - low) / (high - low) * 254.0;
    1 + scaled.clamp(0.0, 254.0) as u8
}

/// The feature vectors transposed into one series per name.
fn series(vectors: &[[f64; 7]]) -> BTreeMap<String, Vec<f32>> {
    NODE_FEATURES
        .iter()
        .enumerate()
        .map(|(index, name)| {
            let values = vectors.iter().map(|v| v[index] as f32).collect();
            ((*name).to_owned(), values)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use flow_core::{ClassMapping, DensityClass, NodePlacement, NodeRole, SessionMeta};
    use flow_ingest::SessionWriter;

    use super::*;

    const SECOND: u64 = 1_000_000;

    fn meta(nodes: &[&str]) -> SessionMeta {
        SessionMeta {
            session_id: "kit-20260810T120000Z".into(),
            site: "bench".into(),
            environment: "portrait".into(),
            wifi_channel: 6,
            nodes: nodes
                .iter()
                .map(|id| NodePlacement {
                    node_id: (*id).to_owned(),
                    role: NodeRole::Rx,
                    position: String::new(),
                })
                .collect(),
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

    fn frame(ts_us: u64, node: &str, amplitude: f32) -> CsiFrame {
        CsiFrame::new(
            ts_us,
            node,
            -52,
            7,
            vec![amplitude, amplitude * 2.0],
            vec![0.0, 0.0],
        )
        .unwrap()
    }

    /// A sealed session holding `frames`, ready to be described.
    fn recorded(root: &Path, nodes: &[&str], frames: &[CsiFrame], labels: &[Label]) -> PathBuf {
        let mut writer = SessionWriter::create(root, &meta(nodes)).unwrap();
        for frame in frames {
            writer.write_frame(frame).unwrap();
        }
        for label in labels {
            writer.write_label(label).unwrap();
        }
        writer.finalize().unwrap().path
    }

    /// Frames from one node at `rate_hz` over `seconds`, all of one amplitude.
    fn steady(node: &str, seconds: u64, rate_hz: u64, amplitude: f32) -> Vec<CsiFrame> {
        (0..seconds * rate_hz)
            .map(|step| frame(step * SECOND / rate_hz, node, amplitude))
            .collect()
    }

    #[test]
    fn a_capture_is_described_node_by_node() {
        let dir = tempfile::tempdir().unwrap();
        let session = recorded(
            dir.path(),
            &["rx-1", "rx-2"],
            &steady("rx-1", 20, 10, 4.0),
            &[Label {
                ts_us: 0,
                class: DensityClass::Low,
                count: None,
            }],
        );

        let portrait = compute(&session, 99).unwrap();

        assert_eq!(portrait.nodes.len(), 2, "both declared receivers appear");
        let rx1 = &portrait.nodes[0];
        assert_eq!(rx1.frames, 200);
        assert!((rx1.rate_hz - 10.0).abs() < 0.6, "{}", rx1.rate_hz);
        assert_eq!(rx1.subcarriers, 2);
        // Declared but silent: reported as itself rather than left out, which
        // is the whole point of looking at a capture.
        assert_eq!(portrait.nodes[1].frames, 0);
        assert_eq!(portrait.labels.len(), 1);
        assert!(!portrait.truncated);
    }

    #[test]
    fn a_long_capture_keeps_its_heatmap_within_one_screen() {
        // Bins double in width rather than multiply in number: the browser is
        // handed something it can paint whatever the capture's length.
        let dir = tempfile::tempdir().unwrap();
        let session = recorded(dir.path(), &["rx-1"], &steady("rx-1", 600, 4, 4.0), &[]);

        let portrait = compute(&session, 0).unwrap();

        assert!(portrait.bins <= MAX_BINS, "{} bins", portrait.bins);
        assert!(portrait.bin_us > INITIAL_BIN_US, "resolution never halved");
        let pixels = std::fs::read(heatmap_path(&session)).unwrap();
        assert_eq!(pixels.len(), portrait.bins * portrait.nodes[0].subcarriers);
    }

    #[test]
    fn merging_bins_preserves_the_amplitudes_they_held() {
        // Sums merge by addition and means come out at the end; a bin holding
        // means would need a weighting that this test would catch missing.
        let dir = tempfile::tempdir().unwrap();
        let session = recorded(dir.path(), &["rx-1"], &steady("rx-1", 400, 4, 7.0), &[]);

        let portrait = compute(&session, 0).unwrap();
        let pixels = std::fs::read(heatmap_path(&session)).unwrap();

        // Two subcarriers held one amplitude each throughout, so after any
        // number of merges every pixel must still sit on one end of the scale.
        // A weighting error would land some of them in between.
        assert_eq!(
            (portrait.nodes[0].amp_min, portrait.nodes[0].amp_max),
            (7.0, 14.0)
        );
        let levels: std::collections::BTreeSet<u8> = pixels.iter().copied().collect();
        assert_eq!(levels, [1, 255].into_iter().collect());
    }

    #[test]
    fn a_hole_reads_as_a_hole_rather_than_as_silence_at_zero() {
        let dir = tempfile::tempdir().unwrap();
        let mut frames = steady("rx-1", 3, 10, 4.0);
        frames.extend(
            steady("rx-1", 3, 10, 9.0)
                .into_iter()
                .map(|f| frame(f.ts_us + 30 * SECOND, "rx-1", 9.0)),
        );
        let session = recorded(dir.path(), &["rx-1"], &frames, &[]);

        let portrait = compute(&session, 0).unwrap();
        let pixels = std::fs::read(heatmap_path(&session)).unwrap();

        assert_eq!(portrait.nodes[0].gaps.len(), 1, "the silence is reported");
        assert!(
            pixels.contains(&NO_DATA),
            "empty bins are marked, not painted as zero amplitude"
        );
    }

    #[test]
    fn a_capture_killed_mid_write_is_described_up_to_the_break() {
        let dir = tempfile::tempdir().unwrap();
        let session = recorded(dir.path(), &["rx-1"], &steady("rx-1", 5, 10, 4.0), &[]);
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(session.join("csi.ndjson"))
            .unwrap();
        std::io::Write::write_all(&mut file, br#"{"ts_us":99,"node_"#).unwrap();
        drop(file);

        let portrait = compute(&session, 0).unwrap();

        assert!(portrait.truncated);
        assert_eq!(portrait.nodes[0].frames, 50, "everything before it stands");
    }

    #[test]
    fn the_cache_is_read_back_and_a_foreign_one_is_not() {
        let dir = tempfile::tempdir().unwrap();
        let session = recorded(dir.path(), &["rx-1"], &steady("rx-1", 5, 10, 4.0), &[]);

        let computed = compute(&session, 42).unwrap();
        let read_back = cached(&session).expect("a portrait was just written");

        // Field by field rather than whole: serde_json parses floats through a
        // fast path that can land one ULP away, which no reader of a frame
        // rate can tell and no test should be pinned to.
        assert_eq!(read_back.session_id, computed.session_id);
        assert_eq!(read_back.computed_at_us, 42);
        assert_eq!(read_back.bins, computed.bins);
        assert_eq!(read_back.bin_us, computed.bin_us);
        assert_eq!(read_back.nodes[0].frames, computed.nodes[0].frames);
        assert!(heatmap_path(&session).is_file());

        std::fs::write(session.join(PORTRAIT_FILE), br#"{"schema":99}"#).unwrap();
        assert!(cached(&session).is_none());
    }

    #[test]
    fn features_are_sampled_on_the_geometry_training_uses() {
        let dir = tempfile::tempdir().unwrap();
        let session = recorded(dir.path(), &["rx-1"], &steady("rx-1", 12, 20, 4.0), &[]);

        let portrait = compute(&session, 0).unwrap();

        assert_eq!(portrait.window_us, DEFAULT_WINDOW_US);
        assert_eq!(portrait.hop_us, DEFAULT_HOP_US);
        // First sample once a window has filled, then one per hop.
        assert_eq!(portrait.feature_ts_us.first(), Some(&DEFAULT_WINDOW_US));
        for name in NODE_FEATURES {
            let values = &portrait.nodes[0].features[name];
            assert_eq!(values.len(), portrait.feature_ts_us.len());
        }
        let rate = &portrait.nodes[0].features["frame_rate"];
        assert!((rate[0] - 20.0).abs() < 1.0, "{}", rate[0]);
    }
}
