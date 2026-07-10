//! Density inference for the Mariam Flow edge.
//!
//! Planned scope: feature extraction over sliding windows, 4-class density
//! classification through an ONNX model (via `tract`), conversion of density
//! into a waiting time with Little's Law (W = L / λ), and output smoothing
//! (EMA + hysteresis).
//!
//! Not yet implemented — this crate currently only reserves its place in the
//! workspace. See `docs/architecture.md` for the component's role.
