//! doc-request: DOC's request validation, layout resolution and cache key (C1, C2). What a
//! browser needs for instant validation: no regex engine, no markdown engine, no kitchen.
//! Depends on `doc-model`, serde_json and sha2. Builds for wasm.

pub mod request;
pub use request::*;

/// Validate and key a request without doing any work (what the API runs before it enqueues): JSON text in, JSON text out.
/// `{"ok":true,"key":..,"layout":..}`, `{"ok":false,"kind":"violations","violations":[..]}` or `{"ok":false,"kind":"input","error":..}`.
pub fn key_json(input: &str) -> String {
    let Ok(request) = serde_json::from_str::<serde_json::Value>(input) else {
        return serde_json::json!({"ok": false, "kind": "input", "error": "request is not JSON"}).to_string();
    };
    match validate(&request) {
        Ok(valid) => serde_json::json!({"ok": true, "key": request_key(&valid), "layout": valid.layout.code()}).to_string(),
        Err(violations) => serde_json::json!({"ok": false, "kind": "violations", "violations": violations}).to_string(),
    }
}
