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

mod clock;
mod convert;
pub mod esp_csi;
mod reader;
pub mod session;
mod source;
mod timeline;
pub mod udp;

pub use esp_csi::{LineFormat, MacAddr, ParseError, RawCsiFrame, parse_line};
pub use reader::{CsiReader, StreamStats};
pub use session::{SessionError, SessionSummary, SessionWriter};
pub use source::{FrameSource, SourceConfig, SourceError, UDP_SCHEME};
pub use timeline::Timeline;
pub use udp::{SenderKey, UdpSource, UdpStats, parse_node_mapping};
