//! Findings: what the as-built code swallows, returned instead (captain: keep names as written
//! and return findings; nothing is ruled on defect flags until the content team rules).

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// `as_built` reproduces DOC's FastAPI responses including its defects; `fixed` repairs the
/// defects the earlier reports found. Both stay until the content team rules.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    #[default]
    AsBuilt,
    Fixed,
}

impl Mode {
    pub fn as_str(self) -> &'static str {
        match self {
            Mode::AsBuilt => "as_built",
            Mode::Fixed => "fixed",
        }
    }
    pub fn parse(s: &str) -> Option<Mode> {
        match s.trim().to_ascii_lowercase().as_str() {
            "as_built" | "as-built" | "asbuilt" => Some(Mode::AsBuilt),
            "fixed" => Some(Mode::Fixed),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Info,
    Warning,
    Error,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum FindingCode {
    /// A data-API row was skipped (invalid), where DOC discards the whole catalog.
    CatalogRowInvalid,
    /// The data API could not be reached; the last good catalog was served.
    CatalogServedStale,
    /// A repository could not be fetched (clone failed or timed out after retries).
    RepoUnavailable,
    /// A repository refresh failed; the previous copy was served.
    RepoServedStale,
    /// A file in a repository could not be read or decoded.
    RepoUnreadable,
    /// A repository names a book code that is not a canonical book (DOC raises KeyError and
    /// returns an empty list for the whole language).
    UnknownBookCode,
    /// The owner prefix of a repository name was stripped (`update_repo_components`).
    OwnerPrefixStripped,
    /// A book had no name in the resource; kitchen's national name was used.
    NameFromKitchen,
    /// A book had no name anywhere in the requested language; the English name was used.
    NameFellBackToEnglish,
    /// The resource's own name equals the English name in a non-English language.
    NameLooksEnglish,
    /// `front/title.txt` holds more than one line; the first non-empty line was used.
    TitleFileMultiline,
    /// A manifest could not be read or has an unexpected shape.
    ManifestUnreadable,
    /// DOC's name normalisation would have changed the name as written.
    NameNormalisationWouldChange,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Finding {
    pub code: FindingCode,
    pub severity: Severity,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub lang: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub repo: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub book: Option<String>,
    pub detail: String,
}

impl Finding {
    pub fn new(code: FindingCode, severity: Severity, detail: impl Into<String>) -> Finding {
        Finding { code, severity, lang: None, repo: None, book: None, detail: detail.into() }
    }
    pub fn lang(mut self, v: &str) -> Finding {
        self.lang = Some(v.to_string());
        self
    }
    pub fn repo(mut self, v: &str) -> Finding {
        self.repo = Some(v.to_string());
        self
    }
    pub fn book(mut self, v: &str) -> Finding {
        self.book = Some(v.to_string());
        self
    }
}
