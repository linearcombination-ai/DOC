#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! Drift test: every documented response of every route is validated against the OpenAPI document
//! the same binary serves. If a handler's output and its `#[utoipa::path]` annotation (or the
//! contract types behind it) disagree, this fails. It closes the one gap the type system leaves:
//! the annotation is written next to the handler, not derived from its return type.

mod support;

use doc_contract::Mode;
use serde_json::{json, Value};
use support::*;

fn validate(doc: &Value, schema_name_or_inline: &Value, instance: &Value, what: &str) {
    // wrap the response schema so `$ref: #/components/...` resolves against the document
    let mut root = json!({ "components": doc["components"].clone() });
    for (k, v) in schema_name_or_inline.as_object().cloned().unwrap_or_default() {
        root[k] = v;
    }
    let validator = jsonschema::validator_for(&root).unwrap_or_else(|e| panic!("{what}: schema does not compile: {e}"));
    let errors: Vec<String> = validator.iter_errors(instance).map(|e| e.to_string()).collect();
    assert!(errors.is_empty(), "{what}: response does not match its documented schema: {errors:?}\ninstance: {instance}");
}

fn documented(doc: &Value, path: &str, status: u16) -> Value {
    doc["paths"][path]["get"]["responses"][status.to_string()]["content"]["application/json"]["schema"].clone()
}

#[tokio::test]
async fn every_documented_response_matches_what_the_handlers_return() {
    let env = start(vec![Step::Ok(two_book_catalog())], Knobs::default()).await;
    let doc: Value = env.client.get(format!("{}/openapi.json", env.base)).send().await.unwrap().json().await.unwrap();
    assert_eq!(doc["openapi"], "3.1.0");
    let cases: Vec<(&str, String)> = vec![
        ("/health/status", "/health/status".into()),
        ("/chapters_in_books", "/chapters_in_books".into()),
        ("/language_codes_and_names", "/language_codes_and_names".into()),
        ("/book_codes_for_lang/{lang_code}", "/book_codes_for_lang/xx".into()),
        ("/book_codes_for_lang_from_usfm_only/{lang_code}", "/book_codes_for_lang_from_usfm_only/xx".into()),
        ("/shared_book_codes/{lang0_code}/{lang1_code}", "/shared_book_codes/xx/xx".into()),
        ("/resource_types/{lang_code}/{book_codes}", "/resource_types/xx/all".into()),
        ("/findings/book_codes_for_lang/{lang_code}", "/findings/book_codes_for_lang/xx".into()),
        ("/findings/book_codes_for_lang_from_usfm_only/{lang_code}", "/findings/book_codes_for_lang_from_usfm_only/xx".into()),
        ("/findings/shared_book_codes/{lang0_code}/{lang1_code}", "/findings/shared_book_codes/xx/xx".into()),
        ("/findings/resource_types/{lang_code}/{book_codes}", "/findings/resource_types/xx/all".into()),
    ];
    let mut checked = 0;
    for mode in [Mode::AsBuilt, Mode::Fixed] {
        for (template, concrete) in &cases {
            let (status, body, _) = env.get(concrete, mode).await;
            assert_eq!(status, 200, "{concrete} {mode:?}: {body}");
            let schema = documented(&doc, template, 200);
            assert!(schema.is_object(), "{template} documents no 200 body");
            validate(&doc, &schema, &body, &format!("{mode:?} {concrete}"));
            checked += 1;
        }
    }
    // the error shapes: 422 (fixed mode), 503
    let (status, body, _) = env.get("/book_codes_for_lang/a%20b", Mode::Fixed).await;
    assert_eq!(status, 422);
    validate(&doc, &documented(&doc, "/book_codes_for_lang/{lang_code}", 422), &body, "422 body");
    assert_eq!(body["status_code"], 10422, "DOC's custom envelope, not FastAPI's detail array");
    let (status, body, _) = env.get("/resource_types/xx/mat,notabook", Mode::Fixed).await;
    assert_eq!(status, 422, "{body}");
    env.api.set_script(vec![Step::Status(500)]);
    let env2 = start(vec![Step::Status(500)], Knobs::default()).await;
    let (status, body, _) = env2.get("/book_codes_for_lang/xx", Mode::Fixed).await;
    assert_eq!(status, 503);
    validate(&doc, &documented(&doc, "/book_codes_for_lang/{lang_code}", 503), &body, "503 body");
    assert_eq!(checked, 22);
}

#[tokio::test]
async fn route_table_matches_the_document() {
    let env = start(vec![Step::Ok(two_book_catalog())], Knobs::default()).await;
    let doc: Value = env.client.get(format!("{}/openapi.json", env.base)).send().await.unwrap().json().await.unwrap();
    let paths: Vec<&String> = doc["paths"].as_object().unwrap().keys().collect();
    assert_eq!(paths.len(), 11);
    for p in paths {
        assert!(doc["paths"][p.as_str()]["get"].is_object(), "{p} documents a GET");
    }
}
