//! doc-shell: HTTP edge for endpoints 1 to 7. Thin by design: parse the request, call a use case,
//! map the result to a response; no domain logic lives here.

pub mod config;
pub mod cors;
pub mod routes;

pub use routes::{build, build_with, Shared};
