//! Error bodies.
//!
//! `NotFound` and `MethodNotAllowed` reproduce Starlette's `{"detail": ...}`. `Validation`
//! reproduces DOC's custom 422 handler (`backend/doc/entrypoints/app.py`):
//! `{"status_code": 10422, "message": "...", "data": null}`. `Unavailable` is new (fixed mode): a
//! transient dependency failure that outlived the retry ceilings; `retryable` tells the caller.

use serde::Serialize;
use utoipa::ToSchema;

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct DetailBody {
    pub detail: String,
}

/// DOC's 422 envelope. `status_code` is the application code 10422, not the HTTP status.
#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct ValidationBody {
    pub status_code: u32,
    pub message: String,
    #[schema(value_type = Option<String>)]
    pub data: Option<String>,
}

impl ValidationBody {
    pub fn new(message: String) -> ValidationBody {
        ValidationBody { status_code: 10422, message, data: None }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ErrorKind {
    CatalogUnavailable,
    CatalogMalformed,
    DeadlineExceeded,
}

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct UnavailableBody {
    pub kind: ErrorKind,
    pub message: String,
    pub retryable: bool,
}
