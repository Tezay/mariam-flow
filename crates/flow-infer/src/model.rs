//! ONNX density classifier, executed with `tract` (pure Rust, no Python
//! runtime in production — ADR 0004).
//!
//! The model artifact is the whole trained pipeline (standardization
//! included) exported as five core ONNX operators (ADR 0006); its input is
//! one feature vector (`[1, n]` float32) and its output the probability
//! distribution over the four density classes (ADR 0002).

use std::path::Path;

use flow_core::DensityClass;
use thiserror::Error;
use tract_onnx::prelude::*;

/// Failure while loading or running the density model.
#[derive(Debug, Error)]
pub enum InferError {
    /// The ONNX file could not be loaded, optimized, or planned.
    #[error("failed to load model: {0}")]
    Load(String),
    /// The model's input is not a concrete `[1, n]` tensor.
    #[error("unsupported model input shape")]
    BadInputShape,
    /// The caller provided the wrong number of features.
    #[error("model expects {expected} features, got {got}")]
    FeatureCount {
        /// Feature count declared by the model.
        expected: usize,
        /// Feature count provided by the caller.
        got: usize,
    },
    /// Inference itself failed.
    #[error("inference failed: {0}")]
    Run(String),
    /// The model's output is not a `[1, 4]` probability tensor.
    #[error("unexpected model output shape")]
    BadOutput,
}

/// The classifier's output: a probability per density class.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Prediction {
    /// Probabilities for `empty`, `low`, `medium`, `saturated`, in class
    /// order. Non-negative, summing to 1.
    pub probabilities: [f32; 4],
}

impl Prediction {
    /// Most likely class (argmax; ties resolve to the lower class).
    #[must_use]
    pub fn class(&self) -> DensityClass {
        let mut best = 0;
        for (index, p) in self.probabilities.iter().enumerate() {
            if *p > self.probabilities[best] {
                best = index;
            }
        }
        DensityClass::ALL[best]
    }

    /// Probability of the most likely class, used to gate the published
    /// estimate (low confidence ⇒ estimate hidden, drift alert).
    #[must_use]
    pub fn confidence(&self) -> f32 {
        let class = self.class();
        self.probabilities[class.as_u8() as usize]
    }

    /// Expected density level `E = Σ pᵢ·i` on the continuous 0–3 scale —
    /// the smooth signal consumed by Little's Law and the output filter.
    #[must_use]
    pub fn expected_level(&self) -> f32 {
        self.probabilities
            .iter()
            .enumerate()
            .map(|(index, p)| index as f32 * p)
            .sum()
    }
}

type Plan = TypedRunnableModel<TypedModel>;

/// A loaded, optimized, runnable density classifier.
pub struct DensityModel {
    plan: Plan,
    n_features: usize,
}

impl DensityModel {
    /// Loads an ONNX model from disk, optimizes it, and prepares an
    /// execution plan.
    ///
    /// # Errors
    ///
    /// [`InferError::Load`] if the file cannot be read or planned;
    /// [`InferError::BadInputShape`] if the model input is not a concrete
    /// `[1, n]` float tensor.
    pub fn load(path: &Path) -> Result<Self, InferError> {
        let plan: Plan = tract_onnx::onnx()
            .model_for_path(path)
            .and_then(|model| model.into_optimized())
            .and_then(|model| model.into_runnable())
            .map_err(|err| InferError::Load(err.to_string()))?;

        let fact = plan
            .model()
            .input_fact(0)
            .map_err(|err| InferError::Load(err.to_string()))?;
        let dims = fact.shape.as_concrete().ok_or(InferError::BadInputShape)?;
        let n_features = match dims {
            [1, n] => *n,
            _ => return Err(InferError::BadInputShape),
        };
        Ok(Self { plan, n_features })
    }

    /// Number of features the model expects per window.
    #[must_use]
    pub fn n_features(&self) -> usize {
        self.n_features
    }

    /// Runs the classifier on one window's feature vector.
    ///
    /// # Errors
    ///
    /// [`InferError::FeatureCount`] on a wrong-sized input,
    /// [`InferError::Run`] / [`InferError::BadOutput`] on execution
    /// failures.
    pub fn predict(&self, features: &[f32]) -> Result<Prediction, InferError> {
        if features.len() != self.n_features {
            return Err(InferError::FeatureCount {
                expected: self.n_features,
                got: features.len(),
            });
        }
        let input = tract_ndarray::Array2::from_shape_vec((1, self.n_features), features.to_vec())
            .map_err(|err| InferError::Run(err.to_string()))?;
        let outputs = self
            .plan
            .run(tvec!(Tensor::from(input).into()))
            .map_err(|err| InferError::Run(err.to_string()))?;
        let view = outputs[0]
            .to_array_view::<f32>()
            .map_err(|err| InferError::Run(err.to_string()))?;
        let flat: Vec<f32> = view.iter().copied().collect();
        let probabilities: [f32; 4] = flat.try_into().map_err(|_| InferError::BadOutput)?;
        Ok(Prediction { probabilities })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn argmax_class_and_confidence() {
        let prediction = Prediction {
            probabilities: [0.1, 0.2, 0.6, 0.1],
        };
        assert_eq!(prediction.class(), DensityClass::Medium);
        assert!((prediction.confidence() - 0.6).abs() < 1e-6);
    }

    #[test]
    fn expected_level_is_the_probability_weighted_mean() {
        let prediction = Prediction {
            probabilities: [0.1, 0.2, 0.6, 0.1],
        };
        // 0·0.1 + 1·0.2 + 2·0.6 + 3·0.1 = 1.7
        assert!((prediction.expected_level() - 1.7).abs() < 1e-6);
    }

    #[test]
    fn ties_resolve_to_the_lower_class() {
        let prediction = Prediction {
            probabilities: [0.4, 0.4, 0.1, 0.1],
        };
        assert_eq!(prediction.class(), DensityClass::Empty);
    }
}
