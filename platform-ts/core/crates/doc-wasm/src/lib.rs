//! wasm-bindgen entry point: one function, JSON in, JSON out, with kitchen's names plugged in.

use doc_kitchen::KitchenCanon;
use wasm_bindgen::prelude::*;

/// `op` and `input` are as documented in `doc_books::api`; the return value is `{"ok":..}` or `{"error":..}`.
#[wasm_bindgen]
pub fn call(op: &str, input: &str) -> String {
    match op {
        // P5: the document-job ops. Both take and return the raw JSON text, never panic on input.
        "job_key" => doc_request::key_json(input),
        #[cfg(feature = "jobs")]
        "job_parse" => doc_job::parse_json(input),
        #[cfg(feature = "jobs")]
        "job_assemble" => doc_job::assemble_json(input),
        #[cfg(not(feature = "jobs"))]
        "job_parse" | "job_assemble" => String::from(r#"{"ok":false,"kind":"input","error":"this wasm build has no job ops (it is the light API module)"}"#),
        #[cfg(feature = "fault-injection")]
        "debug_spin" => {
            let mut x: u64 = input.len() as u64 + 1;
            loop {
                x = std::hint::black_box(x.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1));
            }
        }
        _ => doc_books::api::call(op, input, &KitchenCanon),
    }
}

/// The kitchen version the adapter was written against.
#[wasm_bindgen]
pub fn kitchen_version() -> String {
    KitchenCanon::KITCHEN_VERSION.to_string()
}
