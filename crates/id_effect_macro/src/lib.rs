//! Declarative macros for the `effect` crate.
//!
//! Rust does not allow `macro_rules!` and `#[proc_macro]` in the same crate. This crate holds the
//! declarative macros; procedural `effect!` lives in the **`effect-proc-macro`** crate.
//!
//! Intra-doc links to `effect` types use fully qualified paths; this crate does not depend on
//! `effect`, so rustdoc cannot resolve them. Suppress the lint here only.
//!
//! For optional runtime batch mapping with rayon, see [`map_slice_par`](batch_par::map_slice_par).
#![allow(rustdoc::broken_intra_doc_links)]
#![deny(missing_docs)]

pub mod batch_par;
pub mod context;
pub mod layer;
pub mod pipe;
pub mod service;

pub use batch_par::map_slice_par;
