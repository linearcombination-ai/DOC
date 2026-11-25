//! The base OpenAPI document. Routes are registered into it by the shell's `OpenApiRouter`
//! (each handler carries a `#[utoipa::path]` and is bound to its route by `routes!`).

use utoipa::OpenApi;

use crate::error::{DetailBody, ErrorKind, UnavailableBody, ValidationBody};
use crate::finding::{Finding, FindingCode, Mode, Severity};
use crate::wire::{
    BookEntry, ChaptersInBooks, HealthDetail, HealthResponse, LanguageEntry, ResourceTypeEntry,
    WithFindings,
};

#[derive(OpenApi)]
#[openapi(
    info(
        title = "DOC shell (Rust, P4)",
        description = "Endpoints 1 to 7 of the DOC stack report, behaviour-compatible with DOC's FastAPI in `as_built` mode.",
        version = "0.0.4"
    ),
    components(schemas(
        HealthDetail, HealthResponse, LanguageEntry, BookEntry, ResourceTypeEntry, ChaptersInBooks,
        Finding, FindingCode, Severity, Mode, DetailBody, ValidationBody, UnavailableBody, ErrorKind,
        WithFindings<Vec<BookEntry>>, WithFindings<Vec<ResourceTypeEntry>>
    ))
)]
pub struct ApiDoc;
