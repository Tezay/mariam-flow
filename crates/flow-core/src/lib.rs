//! Canonical domain types shared across the Mariam Flow edge stack.
//!
//! This crate is the single source of truth for the data model every other
//! component agrees on:
//!
//! - [`CsiFrame`] — one CSI measurement as received from a sensing node,
//!   matching one line of `csi.ndjson` in the canonical session format.
//! - [`DensityClass`] — the frozen 4-class output space of the density
//!   classifier, encoded as integers `0..=3` on disk and on the wire.
//! - [`Label`] — one ground-truth annotation, matching one line of
//!   `labels.ndjson`.
//! - [`SessionMeta`] — capture-session metadata, matching `meta.json`.
//!
//! Serialization is via serde; the JSON field names are part of the frozen
//! session format documented in `docs/architecture.md`. Serde alone does not
//! enforce structural invariants, so data crossing a trust boundary (network,
//! disk) must be checked with [`CsiFrame::validate`] after deserialization.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod density;
mod error;
mod frame;
mod session;

pub use density::DensityClass;
pub use error::{FrameError, InvalidDensityClass};
pub use frame::CsiFrame;
pub use session::{ClassMapping, Label, NodePlacement, NodeRole, SessionMeta};

/// Microsecond-resolution Unix timestamp.
///
/// Timestamps are assigned by the edge aggregator at frame reception, never
/// by the sensing nodes (whose clocks are not trusted).
pub type TimestampUs = u64;
