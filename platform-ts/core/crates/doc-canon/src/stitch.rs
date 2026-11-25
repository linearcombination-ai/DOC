//! Writer-repository stitching through kitchen's `bttwriter` (P3).
//!
//! DOC assembles a per-book Writer repository with `combine_usfm_files` (`parsing.py:1388-1426`):
//! `\ide UTF-8`, `\h <front/title.txt, normalised>`, then for each chapter directory (sorted by
//! name, `front`, `00` and dot-directories left out) `\c N`, optionally `\cl`, and each chunk file
//! (`.usfm` or `.txt`, sorted by path) with a leading duplicate `\c` removed and a `\p` repair for
//! verse 1 chunks, joined by ` \n`. kitchen's `bttwriter::stitch` writes `\id`, `\ide`, `\h`,
//! `\toc1`-`\toc3` and one `\mt` per title line, then each chapter's `\c`, `\cl` and trimmed chunks.
//! This function is the adapter: it hands the files over and returns kitchen's USFM.

use crate::kitchen_gaps::{self, Gaps};
use bttwriter::stitch;
use mise::books::BookKey;

/// One book's files as `(relative path with '/', bytes)` to its USFM. `book_code` is the three
/// letter code (upper-cased); `fallback_title` stands in when `front/title.txt` is empty.
pub fn stitch_writer_book(files: &[(String, Vec<u8>)], book_code: &str, fallback_title: &str) -> String {
    stitch_writer_book_with(files, book_code, fallback_title, &Gaps::default())
}

/// [`stitch_writer_book`] with the kitchen-gap workarounds chosen by `gaps` (G1: chapter titles that
/// contain a backslash marker are cleaned before kitchen writes them as `\cl`).
pub fn stitch_writer_book_with(files: &[(String, Vec<u8>)], book_code: &str, fallback_title: &str, gaps: &Gaps) -> String {
    let cleaned;
    let files: &[(String, Vec<u8>)] = if gaps.cl_marker {
        // KITCHEN-GAP[G1]
        cleaned = kitchen_gaps::sanitize_chapter_titles(files).0;
        &cleaned
    } else {
        files
    };
    let mut code = [b' '; 3];
    for (i, b) in book_code.to_uppercase().bytes().take(3).enumerate() {
        code[i] = b;
    }
    stitch(files.iter().map(|(p, b)| (p.as_str(), b.as_slice())), BookKey::new(code), fallback_title)
}
