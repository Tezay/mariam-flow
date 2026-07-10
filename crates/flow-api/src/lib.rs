//! Local REST API for the Mariam Flow edge.
//!
//! Planned scope: an `axum` HTTP server exposing the live estimate
//! (density class, waiting time, confidence), session management, and
//! control endpoints; plus the outbound push of aggregated estimates to the
//! backend. Raw CSI is never exposed by this API.
//!
//! Not yet implemented — this crate currently only reserves its place in the
//! workspace. See `docs/architecture.md` for the component's role.
