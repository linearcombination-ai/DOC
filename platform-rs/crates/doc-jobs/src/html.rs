//! HTML output for `POST /documents`: DOC's `compose_document`, `check_content_for_issues`, the title page
//! templates and `enclose_html_content`. DOC renders its Jinja templates (`backend/templates/html/`); the three
//! constructs those files use (`{{ name }}`, `{% if name %}`, `{% endif %}`) are rendered here, from DOC's own
//! template files (read at start, not copied into this prototype).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::model::JobError;

/// The sentence DOC substitutes for a document with no verses in it.
pub const NO_VERSES_NOTICE: &str = "NOTE: There are issues with the requested underlying scripture USFM text that make it unusable by this system until translators fix the issue for the language(s), book(s), and resource(s) combination you have requested.";

#[derive(Clone, Debug)]
pub struct Templates {
    dir: PathBuf,
}

impl Templates {
    pub fn new(dir: PathBuf) -> Templates {
        Templates { dir }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Fail at start, not at the first request, when the template directory is wrong.
    pub fn check(&self) -> Result<(), String> {
        for f in ["header_enclosing.html", "header_compact_enclosing.html", "header_no_css_enclosing.html"] {
            if !self.dir.join(f).is_file() {
                return Err(format!("template {} not found in {}", f, self.dir.display()));
            }
        }
        Ok(())
    }

    fn load(&self, name: &str) -> Result<String, JobError> {
        std::fs::read_to_string(self.dir.join(name)).map_err(|e| JobError::transient("template_io", format!("{}: {e}", self.dir.join(name).display())))
    }

    /// `document_html_header`: the layout picks the template; `generate_docx` picks the CSS-free one.
    pub fn header(&self, layout: &str, generate_docx: bool, title1: &str, title2: &str, title3: &str, timestring: &str) -> Result<String, JobError> {
        if generate_docx {
            return self.load("header_no_css_enclosing.html");
        }
        let name = if matches!(layout, "1c_c" | "2c_sl_sr_c") { "header_compact_enclosing.html" } else { "header_enclosing.html" };
        let t = self.load(name)?;
        let mut vars = HashMap::new();
        vars.insert("timestring", timestring.to_string());
        vars.insert("title1", title1.to_string());
        vars.insert("title2", title2.to_string());
        vars.insert("title3", title3.to_string());
        Ok(render(&t, &vars))
    }
}

pub fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

/// Render the Jinja subset DOC's header templates use. Unknown names render empty (Jinja's default).
/// Titles are HTML-escaped by the caller (a deliberate difference: DOC renders them unescaped).
pub fn render(template: &str, vars: &HashMap<&str, String>) -> String {
    let mut out = String::with_capacity(template.len());
    let mut stack: Vec<bool> = Vec::new();
    let mut rest = template;
    let active = |stack: &Vec<bool>| stack.iter().all(|b| *b);
    while let Some(i) = rest.find(['{']) {
        let (head, tail) = rest.split_at(i);
        if active(&stack) {
            out.push_str(head);
        }
        if let Some(body) = tail.strip_prefix("{{") {
            if let Some(end) = body.find("}}") {
                let name = body[..end].trim();
                if active(&stack) {
                    out.push_str(vars.get(name).map(String::as_str).unwrap_or(""));
                }
                rest = &body[end + 2..];
                continue;
            }
        } else if let Some(body) = tail.strip_prefix("{%") {
            if let Some(end) = body.find("%}") {
                let tag = body[..end].trim();
                if let Some(cond) = tag.strip_prefix("if ").or_else(|| tag.strip_prefix("if")) {
                    stack.push(vars.get(cond.trim()).is_some_and(|v| !v.is_empty()));
                } else if tag == "endif" {
                    stack.pop();
                }
                rest = &body[end + 2..];
                continue;
            }
        }
        if active(&stack) {
            out.push('{');
        }
        rest = &tail[1..];
    }
    if active(&stack) {
        out.push_str(rest);
    }
    out
}

/// `compose_document`: the parts' content, each followed by a rule when the part asks for one.
pub fn compose_document(parts: &Value) -> String {
    let mut out = String::new();
    if let Some(list) = parts.as_array() {
        for p in list {
            if let Some(c) = p.get("content").and_then(Value::as_str) {
                out.push_str(c);
            }
            if p.get("use_section_visual_separator").and_then(Value::as_bool) == Some(true) {
                out.push_str("<hr/>");
            }
        }
    }
    out
}

/// `check_content_for_issues` (only applied when USFM scripture was requested): content without a single
/// verse is replaced by DOC's notice. Returns the content and whether it was replaced (a finding).
pub fn check_content_for_issues(content: String) -> (String, bool) {
    if content.contains("class=\"verse\"") {
        (content, false)
    } else {
        (NO_VERSES_NOTICE.to_string(), true)
    }
}

pub fn enclose(content: &str, header: &str) -> String {
    format!("{header}{content}</body></html>")
}

pub fn templates_dir_default(here: &Path) -> PathBuf {
    here.join("../../../backend/templates/html")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_the_jinja_subset_like_jinja() {
        let mut v = HashMap::new();
        v.insert("title1", "English".to_string());
        v.insert("title3", String::new());
        let t = "<h>{%if title1%}<h2>{{title1}}</h2>{%endif%} {%if title3%}<h3>{{ title3 }}</h3>{% endif %}</h>{ not a tag }";
        assert_eq!(render(t, &v), "<h><h2>English</h2> </h>{ not a tag }");
    }

    #[test]
    fn composes_and_checks() {
        let parts = serde_json::json!([{"content": "<p>a</p>", "use_section_visual_separator": true}, {"content": "<p class=\"verse\">b</p>"}]);
        let c = compose_document(&parts);
        assert_eq!(c, "<p>a</p><hr/><p class=\"verse\">b</p>");
        assert_eq!(check_content_for_issues(c.clone()), (c, false));
        assert_eq!(check_content_for_issues("<p>x</p>".into()).0, NO_VERSES_NOTICE);
    }
}
