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
use tract::prelude::*;

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

/// A loaded, optimized, runnable density classifier.
pub struct DensityModel {
    runnable: Runnable,
    n_features: usize,
}

impl core::fmt::Debug for DensityModel {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("DensityModel")
            .field("n_features", &self.n_features)
            .finish_non_exhaustive()
    }
}

impl DensityModel {
    /// Loads an ONNX model from disk, optimizes it, and prepares an
    /// execution plan.
    ///
    /// # Errors
    ///
    /// [`InferError::Load`] if the file cannot be read or planned;
    /// [`InferError::BadInputShape`] if the model input is not a concrete
    /// `[1, n]` float tensor; [`InferError::BadOutput`] if it does not answer
    /// one probability per density class.
    pub fn load(path: &Path) -> Result<Self, InferError> {
        let runnable = tract::onnx()
            .and_then(|onnx| onnx.load(path))
            .and_then(|model| model.into_model())
            .and_then(|model| model.into_runnable())
            .map_err(|err| InferError::Load(err.to_string()))?;

        let input = runnable
            .input_fact(0)
            .map_err(|err| InferError::Load(err.to_string()))?;
        let n_features = match concrete_shape(&input).as_deref() {
            Some(&[1, n]) => n,
            _ => return Err(InferError::BadInputShape),
        };

        // Checked here rather than at the first prediction: by then a caller
        // has accepted the model and put it in service.
        let output = runnable
            .output_fact(0)
            .map_err(|err| InferError::Load(err.to_string()))?;
        match concrete_shape(&output).as_deref() {
            Some(&[1, n]) if n == DensityClass::ALL.len() => {}
            _ => return Err(InferError::BadOutput),
        }

        Ok(Self {
            runnable,
            n_features,
        })
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
        let input = Tensor::from_slice(&[1, self.n_features], features)
            .map_err(|err| InferError::Run(err.to_string()))?;
        let outputs = self
            .runnable
            .run([input])
            .map_err(|err| InferError::Run(err.to_string()))?;
        let probabilities: [f32; 4] = outputs
            .first()
            .ok_or(InferError::BadOutput)?
            .as_slice::<f32>()
            .map_err(|err| InferError::Run(err.to_string()))?
            .try_into()
            .map_err(|_| InferError::BadOutput)?;
        Ok(Prediction { probabilities })
    }
}

fn concrete_shape(fact: &Fact) -> Option<Vec<usize>> {
    fact.dims()
        .ok()?
        .map(|dim| usize::try_from(dim.to_int64().ok()?).ok())
        .collect()
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
