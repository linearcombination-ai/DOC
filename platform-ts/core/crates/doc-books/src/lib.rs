//! doc-books: the pure core for DOC waves 1-3 (endpoints 1-7). No I/O, no clock, no network, no
//! kitchen types (names arrive through `doc_model::names::NameProvider`). The shell reaches it
//! through [`api::call`], one JSON-in JSON-out function, so the same core runs native or as wasm.

pub mod api;
pub mod bookname;
pub mod books;
pub mod catalog;
pub mod findings;
pub mod flags;
pub mod locator;
pub mod manifest;
pub mod names;
pub mod pystr;
#[allow(clippy::all)]
pub mod tables_gen;
