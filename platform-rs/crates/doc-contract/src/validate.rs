//! Path-parameter validators. Used by the `fixed` mode; FastAPI accepts any `str`, so the
//! `as_built` mode does not call them (the parity run records what DOC does with odd values).
//!
//! The messages follow Pydantic's `str(RequestValidationError)` after DOC's handler flattens
//! newlines, so a client that shows `message` shows a familiar shape.

use thiserror::Error;

use crate::error::ValidationBody;

#[derive(Debug, Error, PartialEq, Eq)]
#[error("1 validation error for Request {loc} Value error, {msg} [type=value_error, input_value='{input}', input_type=str]")]
pub struct ValidationError {
    pub loc: String,
    pub msg: String,
    pub input: String,
}

impl ValidationError {
    pub fn body(&self) -> ValidationBody {
        ValidationBody::new(self.to_string())
    }
}

/// A language code as the catalog writes them: letters, digits and hyphens, at most 35 characters
/// (`nse-MZ-nsenga`, `tzh-x-oxchuctzeltal`, `es-419`).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct LangCode(String);

impl LangCode {
    pub fn parse(raw: &str, param: &str) -> Result<LangCode, ValidationError> {
        let ok = !raw.is_empty()
            && raw.len() <= 35
            && raw.chars().all(|c| c.is_ascii_alphanumeric() || c == '-');
        if ok {
            Ok(LangCode(raw.to_string()))
        } else {
            Err(ValidationError {
                loc: format!("path.{param}"),
                msg: "language code must be 1 to 35 letters, digits or hyphens".into(),
                input: raw.chars().take(64).collect(),
            })
        }
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// `all`, or a comma-separated list of book codes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BookSelection {
    All,
    Codes(Vec<String>),
}

impl BookSelection {
    pub fn parse(raw: &str, param: &str, is_book: impl Fn(&str) -> bool) -> Result<BookSelection, ValidationError> {
        let err = |msg: String| ValidationError {
            loc: format!("path.{param}"),
            msg,
            input: raw.chars().take(64).collect(),
        };
        if raw == "all" {
            return Ok(BookSelection::All);
        }
        let codes: Vec<String> = raw.split(',').map(str::to_string).collect();
        if codes.len() > 100 {
            return Err(err("at most 100 book codes".into()));
        }
        if let Some(bad) = codes.iter().find(|c| !is_book(c)) {
            return Err(err(format!("unknown book code {bad:?}")));
        }
        Ok(BookSelection::Codes(codes))
    }
}
