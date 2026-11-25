//! doc-request: DOC's request validation, layout resolution and cache key (C1, C2). What a
//! browser needs for instant validation: no regex engine, no markdown engine, no kitchen.
//! Depends on `doc-model`, serde_json and sha2. Builds for wasm.

pub mod request;
pub use request::*;
