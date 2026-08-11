//! The mandatory Python↔Rust parity test (ADR 0004).
//!
//! `ml/flow_ml/export.py` trains the fixture model, exports it to ONNX,
//! and records sklearn's probabilities for a set of inputs. This test runs
//! the same inputs through `tract` and requires identical outputs within
//! the recorded tolerance. Regenerate the fixtures with:
//! `cd ml && uv run python -m flow_ml.export ../crates/flow-infer/tests/fixtures`

use std::fs;
use std::path::PathBuf;

use flow_infer::DensityModel;
use serde::Deserialize;

#[derive(Deserialize)]
struct Fixture {
    n_features: usize,
    tolerance: f32,
    inputs: Vec<Vec<f32>>,
    expected_probabilities: Vec<Vec<f32>>,
}

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn load_fixture() -> Fixture {
    let text = fs::read_to_string(fixtures_dir().join("parity.json")).unwrap();
    serde_json::from_str(&text).unwrap()
}

#[test]
fn tract_reproduces_sklearn_probabilities() {
    let fixture = load_fixture();
    let model = DensityModel::load(&fixtures_dir().join("model.onnx")).unwrap();
    assert_eq!(model.n_features(), fixture.n_features);

    let mut worst: f32 = 0.0;
    for (input, expected) in fixture.inputs.iter().zip(&fixture.expected_probabilities) {
        let prediction = model.predict(input).unwrap();
        let sum: f32 = prediction.probabilities.iter().sum();
        assert!(
            (sum - 1.0).abs() < 1e-5,
            "probabilities must sum to 1, got {sum}"
        );
        for (got, want) in prediction.probabilities.iter().zip(expected) {
            worst = worst.max((got - want).abs());
        }
    }
    assert!(
        worst <= fixture.tolerance,
        "parity violated: max |Δp| = {worst}, tolerance = {}",
        fixture.tolerance
    );
}

#[test]
fn wrong_feature_count_is_rejected() {
    let model = DensityModel::load(&fixtures_dir().join("model.onnx")).unwrap();
    let error = model.predict(&[0.0, 1.0]).unwrap_err();
    assert!(error.to_string().contains("expects"));
}

#[test]
fn a_model_that_never_saw_every_class_is_refused_on_load() {
    let error = DensityModel::load(&fixtures_dir().join("model-incomplete.onnx")).unwrap_err();

    assert!(
        error.to_string().contains("output"),
        "the refusal must name the output, got {error}"
    );
}
