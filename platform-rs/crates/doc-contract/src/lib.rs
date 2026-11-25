//! doc-contract: what crosses the HTTP boundary, defined once.
//!
//! * [`wire`]: response bodies, shaped exactly as DOC's FastAPI emits them (tuples are arrays).
//! * [`finding`]: the typed findings both modes can return, and [`Mode`].
//! * [`validate`]: the path-parameter validators (`LangCode`, `BookSelection`), used by the
//!   `fixed` mode; the `as_built` mode accepts what FastAPI accepts (any string).
//! * [`error`]: error bodies. FastAPI's own 404/405 shapes are reproduced, and DOC's custom 422
//!   envelope (`app.py` `validation_exception_handler`: `{"status_code":10422,...}`), not
//!   FastAPI's default `{"detail":[...]}`.
//! * [`openapi`]: the base OpenAPI document the shell's routes are registered into.
//!
//! Every type derives `serde::Serialize` (runtime) and `utoipa::ToSchema` (document) from the same
//! definition, so the two cannot disagree; the frontend's types are generated from the document.

pub mod error;
pub mod finding;
pub mod jobs;
pub mod openapi;
pub mod validate;
pub mod wire;

pub use finding::{Finding, FindingCode, Mode, Severity};
