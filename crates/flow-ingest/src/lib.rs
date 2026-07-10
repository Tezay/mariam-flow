//! CSI frame ingestion for the Mariam Flow edge.
//!
//! Implemented: parsing of the text frame format emitted by `esp-csi`-based
//! sensing nodes ([`esp_csi`]), conversion of raw frames into canonical
//! [`flow_core::CsiFrame`]s ([`RawCsiFrame::to_frame`]), streaming of
//! frames from any line-based source with loss statistics ([`CsiReader`]),
//! and immutable on-disk session storage ([`SessionWriter`]).
//!
//! Planned: UDP intake and the capture daemon tying these pieces together.
//! See `docs/architecture.md` for the component's role.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod convert;
pub mod esp_csi;
mod reader;
pub mod session;

pub use esp_csi::{LineFormat, MacAddr, ParseError, RawCsiFrame, parse_line};
pub use reader::{CsiReader, StreamStats};
pub use session::{SessionError, SessionSummary, SessionWriter};
