//! Request limits and the path-validation defaults the captain approved: language codes are 1 to 35 letters,
//! digits and hyphens; a request names at most 100 resources (the fixed-mode bound on book codes); the body has a
//! size limit; the disk must have room. Violations are returned in FastAPI's 422 `detail` shape.

use serde_json::{json, Value};

pub const MAX_LANG_CODE: usize = 35;

#[derive(Clone, Debug)]
pub struct Limits {
    /// Largest request body the API reads (bytes). A document request is a few hundred bytes.
    pub max_body_bytes: usize,
    /// Most resource requests in one document request.
    pub max_resources: usize,
    /// Free space the output filesystem must keep (bytes); below it new jobs get a typed 503.
    pub min_free_bytes: u64,
}

impl Limits {
    pub fn production() -> Limits {
        Limits { max_body_bytes: 64 * 1024, max_resources: 100, min_free_bytes: 512 * 1024 * 1024 }
    }
}

pub fn lang_code_ok(code: &str) -> bool {
    !code.is_empty() && code.len() <= MAX_LANG_CODE && code.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
}

/// Check the parts of a request the schema validator does not: the language-code format and the number of
/// resources. Returns FastAPI-style `detail` items.
pub fn check_request(raw: &Value, limits: &Limits) -> Vec<Value> {
    let mut out = Vec::new();
    let Some(list) = raw.get("resource_requests").and_then(Value::as_array) else { return out };
    if list.len() > limits.max_resources {
        out.push(json!({"loc": ["body", "resource_requests"], "msg": format!("at most {} resource requests are accepted, got {}", limits.max_resources, list.len()), "type": "value_error.too_many"}));
    }
    for (i, r) in list.iter().enumerate() {
        if let Some(code) = r.get("lang_code").and_then(Value::as_str) {
            if !lang_code_ok(code) {
                out.push(json!({"loc": ["body", "resource_requests", i, "lang_code"], "msg": format!("language code must be 1 to {MAX_LANG_CODE} letters, digits or hyphens"), "type": "value_error.lang_code"}));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lang_codes() {
        assert!(lang_code_ok("en") && lang_code_ok("nan-x-teochi") && lang_code_ok(&"a".repeat(35)));
        assert!(!lang_code_ok("") && !lang_code_ok(&"a".repeat(36)) && !lang_code_ok("en/../x") && !lang_code_ok("en us"));
    }

    #[test]
    fn request_checks() {
        let l = Limits { max_body_bytes: 10, max_resources: 2, min_free_bytes: 0 };
        let raw = json!({"resource_requests": [{"lang_code": "en"}, {"lang_code": "a b"}, {"lang_code": "fr"}]});
        assert_eq!(check_request(&raw, &l).len(), 2);
    }
}
