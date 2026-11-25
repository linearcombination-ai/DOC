//! Findings: what the as-built code swallowed, returned to the caller instead.

use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
pub struct Finding {
    pub code: &'static str,
    pub severity: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lang: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repo: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub book: Option<String>,
    pub detail: String,
}

impl Finding {
    pub fn new(code: &'static str, severity: &'static str) -> Finding {
        Finding { code, severity, lang: None, repo: None, book: None, detail: String::new() }
    }
    pub fn detail(mut self, d: impl Into<String>) -> Finding {
        self.detail = d.into();
        self
    }
    pub fn lang(mut self, l: &str) -> Finding {
        self.lang = Some(l.to_string());
        self
    }
    pub fn repo(mut self, r: &str) -> Finding {
        self.repo = Some(r.to_string());
        self
    }
    pub fn book(mut self, b: &str) -> Finding {
        self.book = Some(b.to_string());
        self
    }
}

pub type Findings = Vec<Finding>;
