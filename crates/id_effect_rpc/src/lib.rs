//! RPC-style HTTP helpers for [`id_effect`](https://docs.rs/id_effect) Axum hosts — **Phase D**
//! (`@effect/rpc`-shaped boundaries).
//!
//! ## Contents
//!
//! - [`RpcError`] — JSON envelope + status + [`IntoResponse`](axum::response::IntoResponse)
//! - [`correlation`] — `x-correlation-id` propagation
//! - [`span`] — `tracing` span helpers compatible with OpenTelemetry layers
//! - [`parse_rpc_envelopes_par`](envelope::parse_rpc_envelopes_par) — parallel JSON parse of
//!   multiple [`RpcEnvelope`] wire bodies (rayon)
//!
//! Pair with **`id_effect_axum::json`** (`decode_json_schema`, `JsonSchemaError`) for
//! schema-validated JSON bodies at the wire edge (see the mdBook *Axum host* chapter).
//!
//! ## Book
//!
//! See the mdBook chapter *RPC boundaries with id_effect* (`ch07-12-rpc-boundaries.md`).

#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![allow(clippy::missing_panics_doc)]

pub mod correlation;
mod envelope;
pub mod error;
pub mod span;

pub use envelope::{RpcEnvelope, RpcErrorCode, parse_rpc_envelopes_par};
pub use error::RpcError;
