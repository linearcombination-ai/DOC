//! doc-lookup: the pure core behind endpoints 3 to 7.
//!
//! Layering: this crate depends on `doc-contract` (types it returns) and `doc-model` (tables, the
//! `Tree` index, the name port). It does no I/O and has no clock: the catalog arrives as bytes
//! (`catalog::parse_*`), the cloned repositories as a [`reader::Workspace`] over `RepoReader`s.
//! Every function takes a [`Mode`](doc_contract::Mode): `AsBuilt` reproduces DOC's FastAPI
//! (including the swallowed-KeyError empty list), `Fixed` isolates failures per repository and
//! returns [`Finding`](doc_contract::Finding)s.

pub mod booklist;
pub mod catalog;
pub mod languages;
pub mod locator;
pub mod pytext;
pub mod reader;
pub mod restypes;
pub mod tables_gen;

pub use doc_contract::Mode;

/// What a use case returns: the value and the findings (empty in `AsBuilt`).
#[derive(Clone, Debug, Default)]
pub struct Outcome<T> {
    pub value: T,
    pub findings: Vec<doc_contract::Finding>,
}

/// A failure DOC swallows into an empty answer (`except Exception: log; return []`). The `AsBuilt`
/// mode returns the empty answer and logs the reason; the reason is what a fault test asserts on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Abort(pub String);
