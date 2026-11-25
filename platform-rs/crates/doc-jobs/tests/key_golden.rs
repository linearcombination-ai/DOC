#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! The cache key: DOC's golden hashes (tests/unit/test_document_generator.py) and a differential run
//! against DOC's own `request_key.py` on 300 generated requests (`scripts/key_oracle.py`).

use doc_jobs::pipeline::prepare;
use serde_json::Value;

fn cases() -> Vec<Value> {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/key_cases.json");
    let v: Value = serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap();
    v["cases"].as_array().unwrap().clone()
}

#[test]
fn the_two_pinned_golden_keys() {
    for c in cases().iter().take(2) {
        let p = prepare(&c["request"]).unwrap_or_else(|v| panic!("{v:?}"));
        assert_eq!(p.key, c["key"].as_str().unwrap(), "{}", c["name"]);
    }
    // the literals, as written in DOC's test file
    let p = prepare(&cases()[0]["request"]).unwrap();
    assert_eq!(p.key, "en_ulb_mat_70c7420bb621");
    let p = prepare(&cases()[1]["request"]).unwrap();
    assert_eq!(p.key, "en_ulb-tw_mat_50944a0aa352");
}

#[test]
fn the_interleave_oracle_key_for_the_slice_request() {
    let p = prepare(&cases()[2]["request"]).unwrap();
    assert_eq!(p.key, "en_ulb-tn_jud_d404fded3466");
}

#[test]
fn differential_against_docs_request_key_module() {
    let (mut checked, mut rejected) = (0, 0);
    for c in cases() {
        match prepare(&c["request"]) {
            Ok(p) => {
                assert_eq!(p.key, c["key"].as_str().unwrap(), "{}: {}", c["name"], c["request"]);
                checked += 1;
            }
            Err(_) => rejected += 1,
        }
    }
    println!("checked {checked}, rejected by validation {rejected}");
    assert!(checked >= 100, "the differential run must cover many requests (checked {checked}, rejected {rejected})");
}

#[test]
fn delivery_only_settings_do_not_change_the_key() {
    let base = serde_json::json!({"assembly_strategy_kind": "lbo", "document_request_source": "ui", "resource_requests": [{"lang_code": "en", "resource_type": "ulb", "book_code": "jud"}]});
    let mut other = base.clone();
    other["email_address"] = serde_json::json!("x@y.org");
    other["generate_pdf"] = serde_json::json!(false);
    other["generate_epub"] = serde_json::json!(true);
    assert_eq!(prepare(&base).unwrap().key, prepare(&other).unwrap().key);
    let mut docx = base.clone();
    docx["generate_docx"] = serde_json::json!(true);
    assert_ne!(prepare(&base).unwrap().key, prepare(&docx).unwrap().key, "a docx request must not share a key with the HTML one");
}
