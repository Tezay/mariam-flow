//! CSI frame ingestion for the Mariam Flow edge.
//!
//! Implemented: parsing of the text frame format emitted by `esp-csi`-based
//! sensing nodes ([`esp_csi`]), and conversion of raw frames into canonical
//! [`flow_core::CsiFrame`]s ([`RawCsiFrame::to_frame`]).
//!
//! Planned: UDP intake, ring buffering, and immutable on-disk session
//! storage. See `docs/architecture.md` for the component's role.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod convert;
pub mod esp_csi;

pub use esp_csi::{LineFormat, MacAddr, ParseError, RawCsiFrame, parse_line};
