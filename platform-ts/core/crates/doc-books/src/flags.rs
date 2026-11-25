//! Defect flags. `as_built` reproduces DOC's FastAPI byte for byte; `fixed` turns on the fixes for
//! the defects the earlier reports found. Nothing is ruled on until the content team rules, so
//! each fix is its own flag and the shell can set any combination per request.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct Flags {
    /// D-list-1: an unknown book code (59 languages) empties the whole list in DOC (swallowed KeyError).
    pub skip_unknown_book_codes: bool,
    /// D-list-2: DOC applies `update_repo_components` (owner prefixes) in the document path only.
    pub owner_prefix_in_list_path: bool,
    /// D-list-3: one unreadable/missing repository empties the whole list in DOC.
    pub tolerate_repo_errors: bool,
    /// D-catalog-1: one invalid catalog row empties the whole catalog in DOC (and is cached).
    pub skip_bad_catalog_rows: bool,
    /// D-catalog-2: an empty non-primary (bg) catalog empties `/language_codes_and_names` in DOC.
    pub tolerate_empty_nonprimary: bool,
    /// Names: use kitchen's localized name where the resource gives none (DOC shows English).
    pub kitchen_names: bool,
    /// DOC's `add_data_not_supplied_by_data_api` appends to the cached catalog in place, so
    /// `/language_codes_and_names` changes after any book request until the cache expires.
    pub augment_leak: bool,
}

impl Flags {
    pub const fn as_built() -> Flags {
        Flags {
            skip_unknown_book_codes: false,
            owner_prefix_in_list_path: false,
            tolerate_repo_errors: false,
            skip_bad_catalog_rows: false,
            tolerate_empty_nonprimary: false,
            kitchen_names: false,
            augment_leak: true,
        }
    }
    pub const fn fixed() -> Flags {
        Flags {
            skip_unknown_book_codes: true,
            owner_prefix_in_list_path: true,
            tolerate_repo_errors: true,
            skip_bad_catalog_rows: true,
            tolerate_empty_nonprimary: true,
            kitchen_names: true,
            augment_leak: false,
        }
    }
}

impl Default for Flags {
    fn default() -> Flags {
        Flags::as_built()
    }
}
