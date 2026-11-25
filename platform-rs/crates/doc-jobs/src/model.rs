//! The job vocabulary: DOC's exact status strings, the failure result, and the error taxonomy.

use serde::{Deserialize, Serialize};

/// Celery's terminal and not-yet-started states, spelled as Celery spells them. The UI
/// (`GenerateDocument.svelte`) matches `SUCCESS` and `FAILURE` literally.
pub const PENDING: &str = "PENDING";
pub const SUCCESS: &str = "SUCCESS";
pub const FAILURE: &str = "FAILURE";

/// The progress strings of the docx path, in the order `generate_docx_document` sets them
/// (`backend/doc/domain/document_generator.py:352,207,222,234,368,382`). The UI draws its progress bar
/// from these strings (`frontend/src/routes/settings/TaskStatus.svelte:7-40`), so they are spelled
/// exactly. "Sending email" is set only when an address was given and sending is enabled.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Progress {
    ReceivingRequest,
    LocatingAssets,
    ProvisioningAssetFiles,
    ParsingAssetFiles,
    AssemblingContent,
    ConvertingToDocx,
    /// Set when an email address was given and the document is sent.
    SendingEmail,
    /// Set when the HTML is converted to PDF (DOC's string).
    ConvertingToPdf,
}

impl Progress {
    pub const fn as_str(self) -> &'static str {
        match self {
            Progress::ReceivingRequest => "Receiving request",
            Progress::LocatingAssets => "Locating assets",
            Progress::ProvisioningAssetFiles => "Provisioning asset files",
            Progress::ParsingAssetFiles => "Parsing asset files",
            Progress::AssemblingContent => "Assembling content",
            Progress::ConvertingToDocx => "Converting to Docx",
            Progress::SendingEmail => "Sending email",
            Progress::ConvertingToPdf => "Converting to PDF",
        }
    }
}

/// The order the strings appear in for a request that is not a cache hit.
pub const DOCX_SEQUENCE: [Progress; 6] = [
    Progress::ReceivingRequest,
    Progress::LocatingAssets,
    Progress::ProvisioningAssetFiles,
    Progress::ParsingAssetFiles,
    Progress::AssemblingContent,
    Progress::ConvertingToDocx,
];

/// The extended failure result added to `FAILURE` (legacy shows no `result` for it).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobFailure {
    pub code: String,
    pub message: String,
    pub retryable: bool,
}

/// What a stage reports. Transient errors are retried with backoff and jitter; permanent ones end
/// the job at once. A job whose attempts run out ends as `FAILURE` with `retryable: true`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum JobError {
    Transient { code: &'static str, message: String },
    Permanent { code: &'static str, message: String },
}

impl JobError {
    pub fn transient(code: &'static str, message: impl Into<String>) -> JobError {
        JobError::Transient { code, message: message.into() }
    }
    pub fn permanent(code: &'static str, message: impl Into<String>) -> JobError {
        JobError::Permanent { code, message: message.into() }
    }
    pub fn retryable(&self) -> bool {
        matches!(self, JobError::Transient { .. })
    }
    pub fn failure(&self) -> JobFailure {
        match self {
            JobError::Transient { code, message } => JobFailure { code: (*code).into(), message: message.clone(), retryable: true },
            JobError::Permanent { code, message } => JobFailure { code: (*code).into(), message: message.clone(), retryable: false },
        }
    }
}

impl std::fmt::Display for JobError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JobError::Transient { code, message } => write!(f, "transient {code}: {message}"),
            JobError::Permanent { code, message } => write!(f, "permanent {code}: {message}"),
        }
    }
}

/// What `GET /task_status/{id}` reads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JobView {
    pub task_id: String,
    /// `PENDING` until a worker picks the job up, then the last progress string, then
    /// `SUCCESS` or `FAILURE`.
    pub state: String,
    /// The document request key on success.
    pub result: Option<String>,
    pub failure: Option<JobFailure>,
    pub attempt: u32,
    /// A transient failure is waiting for its next attempt.
    pub retrying: bool,
}

impl JobView {
    /// The legacy body: `{state}` while running, `{state, result}` on success; the extended
    /// failure result is added only to `FAILURE`, so a client that reads `state` is unaffected.
    pub fn body(&self) -> serde_json::Value {
        use serde_json::json;
        if self.state == SUCCESS {
            return json!({"state": SUCCESS, "result": self.result});
        }
        if self.state == FAILURE {
            return match &self.failure {
                Some(f) => json!({"state": FAILURE, "result": f}),
                None => json!({"state": FAILURE}),
            };
        }
        json!({"state": self.state})
    }
}
