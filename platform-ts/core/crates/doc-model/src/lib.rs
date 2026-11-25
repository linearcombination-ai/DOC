//! doc-model: the shared types and tables, with no behaviour that belongs to one stage.
//! Typed books and findings (`model`), the static tables (`tables`), the read-only file
//! `Tree` the readers consume, the name port (`names::NameProvider`) and the Python text
//! semantics (`text`). Dependencies: serde, unicode-general-category. Builds for wasm.

pub mod model;
pub mod names;
pub mod tables;
pub mod text;
pub mod tree;
