//! Integration tests of the live pipeline, using the committed fixture
//! model (7 features, one RX node). Values predicted by the model are not
//! asserted here — the parity tests own numerical correctness; these
//! tests pin the *plumbing*: emission schedule, eviction, gap handling.

use std::path::PathBuf;

use flow_core::CsiFrame;
use flow_infer::{DensityModel, LiveConfig, LiveError, LivePipeline, WaitConfig};

const SECOND: u64 = 1_000_000;

fn model() -> DensityModel {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/model.onnx");
    DensityModel::load(&path).unwrap()
}

fn config() -> LiveConfig {
    LiveConfig {
        window_us: 5 * SECOND,
        hop_us: SECOND,
        rx_nodes: vec!["rx-1".to_owned()],
        wait: WaitConfig {
            people_per_class: [0.0, 2.0, 12.0, 30.0],
            service_rate_per_min: 4.0,
            smoothing_tau_s: 10.0,
            hysteresis_margin: 0.15,
            min_confidence: 0.0,
        },
    }
}

fn frame(ts_us: u64) -> CsiFrame {
    CsiFrame::new(
        ts_us,
        "rx-1",
        -52,
        7,
        vec![5.0, 4.0, 3.0],
        vec![0.0, 0.1, 0.2],
    )
    .unwrap()
}

#[test]
fn emits_once_per_hop_after_the_first_window() {
    let mut pipeline = LivePipeline::new(model(), config()).unwrap();
    let mut emissions = Vec::new();
    // Two frames per second for 12 s.
    for tick in 0..=24u64 {
        let ts = tick * SECOND / 2;
        if let Some(estimate) = pipeline.push(frame(ts)).unwrap() {
            emissions.push((ts, estimate));
        }
    }
    // First emission at t = 5 s (first full window), then every 1 s: 8 total.
    assert_eq!(emissions.len(), 8);
    assert_eq!(emissions[0].0, 5 * SECOND);
    assert_eq!(emissions[1].0, 6 * SECOND);
    let estimate = emissions[0].1;
    let sum: f32 = estimate.confidence;
    assert!((0.0..=1.0).contains(&sum));
    assert!(estimate.wait_minutes.is_finite());
}

#[test]
fn skips_missed_hops_after_a_gap() {
    let mut pipeline = LivePipeline::new(model(), config()).unwrap();
    let mut count = 0;
    for tick in 0..=12u64 {
        if pipeline.push(frame(tick * SECOND / 2)).unwrap().is_some() {
            count += 1;
        }
    }
    assert_eq!(count, 2); // t = 5 s and 6 s

    // 60 s gap, then traffic resumes: no burst of stale estimates.
    let mut after_gap = Vec::new();
    for tick in 0..=12u64 {
        let ts = 66 * SECOND + tick * SECOND / 2;
        if pipeline.push(frame(ts)).unwrap().is_some() {
            after_gap.push(ts);
        }
    }
    // One estimate per due hop only, emitted while traffic flows.
    assert!(!after_gap.is_empty());
    assert!(after_gap.len() <= 7, "burst after gap: {after_gap:?}");
}

#[test]
fn window_eviction_bounds_the_buffer() {
    let mut pipeline = LivePipeline::new(model(), config()).unwrap();
    for tick in 0..1_000u64 {
        pipeline.push(frame(tick * SECOND / 2)).unwrap();
    }
    let stats = pipeline.stats();
    assert_eq!(stats.frames, 1_000);
    // 500 s of stream at 1 hop/s minus the 5 s warm-up.
    assert_eq!(stats.estimates, 495);
}

#[test]
fn rejects_out_of_order_frames() {
    let mut pipeline = LivePipeline::new(model(), config()).unwrap();
    pipeline.push(frame(10 * SECOND)).unwrap();
    let error = pipeline.push(frame(9 * SECOND)).unwrap_err();
    assert!(matches!(error, LiveError::OutOfOrder { .. }));
}

#[test]
fn rejects_node_count_mismatch_with_model() {
    let mut wrong = config();
    wrong.rx_nodes = vec!["rx-1".to_owned(), "rx-2".to_owned()];
    let error = LivePipeline::new(model(), wrong).unwrap_err();
    assert!(matches!(error, LiveError::FeatureCountMismatch { .. }));
}

#[test]
fn incomplete_windows_are_counted_not_fatal() {
    let mut config = config();
    config.rx_nodes = vec!["rx-1".to_owned()];
    let mut pipeline = LivePipeline::new(model(), config).unwrap();
    // A single frame spanning past the window: due, but only 1 frame in
    // the buffer (below MIN_FRAMES_PER_NODE after eviction).
    pipeline.push(frame(0)).unwrap();
    let result = pipeline.push(frame(6 * SECOND)).unwrap();
    assert!(result.is_none());
    assert_eq!(pipeline.stats().incomplete_windows, 1);
}
