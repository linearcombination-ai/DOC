//! The job endpoints' contract (P6): the document request the UI posts, the task answers, the status bodies, the
//! error shapes and the extended health. Same rule as the rest of this crate: one definition, serde for the bytes
//! and utoipa for the document, and the frontend's types are generated from the document.
//!
//! `POST /documents` and `POST /documents_docx` take the same body, `DocumentRequestBody`, which mirrors
//! DOC's pydantic `DocumentRequest` (`backend/doc/domain/model.py`): two required fields, the rest optional with DOC's
//! defaults applied by the service.

use serde::{Deserialize, Serialize};
use utoipa::{OpenApi, ToSchema};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum AssemblyStrategy {
    Lbo,
    Blo,
    Lvo,
    Bvo,
    Stet,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub enum AssemblyLayout {
    #[serde(rename = "1c")]
    OneColumn,
    #[serde(rename = "1c_c")]
    OneColumnCompact,
    #[serde(rename = "2c_sl_sr")]
    TwoColumnScriptureLeftScriptureRight,
    #[serde(rename = "2c_sl_sr_c")]
    TwoColumnScriptureLeftScriptureRightCompact,
    #[serde(rename = "stet")]
    Stet,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum DocumentRequestSource {
    Ui,
    Test,
    BielUi,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ResourceRequestBody {
    /// 1 to 35 letters, digits or hyphens.
    pub lang_code: String,
    pub resource_type: String,
    pub book_code: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct DocumentRequestBody {
    pub email_address: Option<String>,
    pub assembly_strategy_kind: AssemblyStrategy,
    pub assembly_layout_kind: Option<AssemblyLayout>,
    pub layout_for_print: Option<bool>,
    /// At most 100.
    pub resource_requests: Vec<ResourceRequestBody>,
    pub generate_pdf: Option<bool>,
    pub generate_epub: Option<bool>,
    pub generate_docx: Option<bool>,
    pub limit_words: Option<bool>,
    pub use_chapter_labels: Option<bool>,
    pub use_prince: Option<bool>,
    pub use_two_column_layout_for_tn_notes: Option<bool>,
    pub use_two_column_layout_for_tq_notes: Option<bool>,
    pub use_section_visual_separator: Option<bool>,
    pub show_tn_book_intro: Option<bool>,
    pub show_bc_book_intro: Option<bool>,
    pub show_tn_chapter_intro: Option<bool>,
    pub show_bc_chapter_commentary: Option<bool>,
    pub document_request_source: Option<DocumentRequestSource>,
}

/// `{"task_id": "..."}`; the header `X-Doc-Coalesced: true` marks a request that joined one already running.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct TaskCreated {
    pub task_id: String,
}

/// The failure result added to `FAILURE` (the legacy answer has no `result` there).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct JobFailureBody {
    pub code: String,
    pub message: String,
    pub retryable: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(untagged)]
pub enum TaskResult {
    /// On `SUCCESS`: the document request key (the file name stem).
    Key(String),
    /// On `FAILURE`: why.
    Failure(JobFailureBody),
}

/// `{"state": ...}`: `PENDING`, then the progress strings the UI matches literally (`Receiving request`,
/// `Locating assets`, `Provisioning asset files`, `Parsing asset files`, `Assembling content`,
/// `Converting to Docx`, `Sending email`), then `SUCCESS` (with `result`) or `FAILURE` (with `result`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct TaskStatusBody {
    pub state: String,
    pub result: Option<TaskResult>,
}

/// A typed, retryable (or not) refusal: 501 (a feature not built), 503 (queue down, disk full).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ApiProblem {
    pub detail: String,
    pub code: String,
    pub retryable: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(untagged)]
pub enum LocItem {
    Name(String),
    Index(u32),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct RequestProblem {
    pub loc: Vec<LocItem>,
    pub msg: String,
    #[serde(rename = "type")]
    pub kind: String,
}

/// FastAPI's own 422 shape (`{"detail": [...]}`) for a bad document request.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct RequestValidationBody {
    pub detail: Vec<RequestProblem>,
}

/// `[{"status": "ok", ...}, 200]`: DOC's tuple shape kept; the first element reports far more than DOC's.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct JobsHealthResponse(pub serde_json::Value, pub u16);

mod health_schema {
    use super::*;
    use utoipa::__dev::ComposeSchema;
    use utoipa::openapi::schema::{ArrayBuilder, ArrayItems, ObjectBuilder, Schema, Type};
    use utoipa::openapi::RefOr;

    fn t(ty: Type) -> Schema {
        ObjectBuilder::new().schema_type(ty).build().into()
    }
    fn obj(desc: &str) -> Schema {
        ObjectBuilder::new().schema_type(Type::Object).description(Some(desc)).build().into()
    }
    impl ComposeSchema for JobsHealthResponse {
        fn compose(_g: Vec<RefOr<Schema>>) -> RefOr<Schema> {
            let strings: Schema = ArrayBuilder::new().items(ArrayItems::RefOrSchema(Box::new(RefOr::T(t(Type::String))))).build().into();
            let procs: Schema = ArrayBuilder::new().items(ArrayItems::RefOrSchema(Box::new(RefOr::T(obj("one process: id, role, age_ms, alive, docx pool, catalog"))))).build().into();
            let detail: Schema = ObjectBuilder::new()
                .schema_type(Type::Object)
                .property("status", t(Type::String))
                .property("ready", t(Type::Boolean))
                .property("problems", strings)
                .property("queue", obj("ok, store, ping_ms, queued, running, delayed, oldest_entry_age_ms, lag_s, stalled_recovered_total"))
                .property("redis", obj("ok, ping_ms"))
                .property("processes", procs)
                .property("catalog", obj("last_success_unix_s, age_s"))
                .property("disk", obj("free_mb, min_free_mb, ok"))
                .property("uptime_s", t(Type::Integer))
                .required("status")
                .required("ready")
                .required("problems")
                .build()
                .into();
            let code: Schema = ObjectBuilder::new().schema_type(Type::Integer).minimum(Some(100)).maximum(Some(599)).build().into();
            RefOr::T(ArrayBuilder::new().prefix_items(vec![detail, code]).items(ArrayItems::False).min_items(Some(2)).max_items(Some(2)).build().into())
        }
    }
    impl ToSchema for JobsHealthResponse {}
}

#[utoipa::path(
    post, path = "/documents_docx", operation_id = "generate_docx_document", request_body = DocumentRequestBody,
    responses(
        (status = 200, description = "The task id; poll /task_status/{task_id}", body = TaskCreated, headers(("X-Request-Id" = String, description = "The request id"), ("X-Doc-Coalesced" = Option<String>, description = "true when an identical request was already running"))),
        (status = 413, description = "The body is larger than the limit"),
        (status = 422, description = "The request is not valid", body = RequestValidationBody),
        (status = 503, description = "Queue unavailable or disk space low; retry (see Retry-After)", body = ApiProblem),
    )
)]
pub fn post_documents_docx() {}

#[utoipa::path(
    post, path = "/documents", operation_id = "generate_document", request_body = DocumentRequestBody,
    responses(
        (status = 200, description = "The task id (HTML output)", body = TaskCreated),
        (status = 413, description = "The body is larger than the limit"),
        (status = 422, description = "The request is not valid", body = RequestValidationBody),
        (status = 501, description = "PDF output is not implemented (generate_pdf=true)", body = ApiProblem),
        (status = 503, description = "Queue unavailable or disk space low; retry", body = ApiProblem),
    )
)]
pub fn post_documents() {}

#[utoipa::path(
    get, path = "/task_status/{task_id}", operation_id = "task_status",
    params(("task_id" = String, Path, description = "The id from POST /documents(_docx)")),
    responses(
        (status = 200, description = "The task's state; an unknown id is PENDING, as with Celery", body = TaskStatusBody),
        (status = 503, description = "Queue unavailable; keep polling", body = ApiProblem),
    )
)]
pub fn get_task_status() {}

#[utoipa::path(
    get, path = "/health/status", operation_id = "health_status",
    responses((status = 200, description = "Liveness (status) and readiness (ready, problems)", body = JobsHealthResponse))
)]
pub fn get_health_status() {}

#[utoipa::path(
    get, path = "/metrics", operation_id = "metrics",
    responses((status = 200, description = "Prometheus text exposition 0.0.4", content_type = "text/plain", body = String))
)]
pub fn get_metrics() {}

#[derive(OpenApi)]
#[openapi(
    paths(post_documents_docx, post_documents, get_task_status, get_health_status, get_metrics),
    components(schemas(
        AssemblyStrategy, AssemblyLayout, DocumentRequestSource, ResourceRequestBody, DocumentRequestBody, TaskCreated, JobFailureBody, TaskResult,
        TaskStatusBody, ApiProblem, LocItem, RequestProblem, RequestValidationBody, JobsHealthResponse
    ))
)]
pub struct JobsApiDoc;
