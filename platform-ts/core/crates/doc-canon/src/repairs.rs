//! DOC-side content repairs (captain 2026-11-25 on D35 and D36: "repair the unambiguous cases AND always
//! return a finding"). Not kitchen gaps: these are policy, kept apart from `kitchen_gaps.rs`.
//!
//! * D35, a verse number glued to the first word (`\v 14Ɖeka`): the word is put back into the verse text
//!   (`VerseModel::text_doc`, label `label_doc`) and a finding names every such verse ([`d35_findings`]).
//!   A lone ASCII letter after the number (`5a`) is a legitimate segment, not a glue, and is not reported.
//! * D36, a chapter with no `\v` whose verses are bare numbers (`1Nya alai ... 2Laban ...`): verses are
//!   inferred only when the pattern is unambiguous, i.e. the first text starts with `1` and the numbers `2, 3, ...`
//!   follow in order as tokens glued to the next word, at least two verses in all ([`repair_bare_verses`]).
//!   Every inferred verse returns a finding; an ambiguous chapter keeps its text as written and returns one finding.

use crate::usfm::UsfmModel;
use doc_model::model::{Finding, FindingCode, Severity};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RepairReport {
    /// (chapter, verse) inferred by the D36 rule
    pub inferred: Vec<(u32, u32)>,
    /// chapters that look like bare-number verses but whose pattern is ambiguous
    pub ambiguous: Vec<u32>,
}


/// Insert `\v N ` where a run of text starts with the bare number that follows the last verse (1 at the
/// start of a chapter): the number must be glued to the next word (`52Dopm`), and once a run has started the
/// following numbers `N+1, N+2, ...` that are glued the same way open further verses. A run that starts with a
/// glued number that is not the expected one is ambiguous: text kept as written, one finding for the chapter.
pub fn repair_bare_verses(source: &str) -> (String, RepairReport) {
    let mut report = RepairReport::default();
    let mut out = String::with_capacity(source.len() + 64);
    let mut chapter: u32 = 0;
    let mut expected: u32 = 1;
    let mut ambiguous_here = false;
    let mut seen_verse = false;
    // only a chapter with no `\v` at all is D36; a chapter that has verse markers is left as written
    let mut chapters_with_v: std::collections::BTreeSet<usize> = std::collections::BTreeSet::new();
    {
        let mut idx = 0usize;
        for line in source.lines() {
            let t = line.trim_start();
            if t.starts_with("\\c ") { idx += 1; }
            if t.starts_with("\\v ") { chapters_with_v.insert(idx); }
        }
    }
    let mut chapter_idx = 0usize;
    for line in source.split_inclusive('\n') {
        let t = line.trim_start();
        if let Some(rest) = t.strip_prefix("\\c ") {
            chapter = rest.trim().parse().unwrap_or(0);
            chapter_idx += 1;
            expected = 1;
            ambiguous_here = false;
            seen_verse = false;
            out.push_str(line);
            continue;
        }
        if let Some(rest) = t.strip_prefix("\\v ") {
            let digits: String = rest.trim_start().chars().take_while(|c| c.is_ascii_digit()).collect();
            // `3-4`: continue after the end of the range
            let after = &rest.trim_start()[digits.len()..];
            let last = if let Some(r) = after.strip_prefix('-') { r.chars().take_while(|c| c.is_ascii_digit()).collect::<String>().parse().unwrap_or(0) } else { 0 };
            expected = digits.parse::<u32>().map_or(expected, |n| n.max(last) + 1);
            seen_verse = true;
            out.push_str(line);
            continue;
        }
        if chapter == 0 || chapters_with_v.contains(&chapter_idx) {
            out.push_str(line);
            continue;
        }
        // split a leading marker from the text
        let (prefix, text) = if t.starts_with('\\') {
            let end = t.find(|c: char| c.is_whitespace()).unwrap_or(t.len());
            (&line[..line.len() - t.len() + end], &t[end..])
        } else {
            ("", line)
        };
        if text.trim().is_empty() {
            out.push_str(line);
            continue;
        }
        out.push_str(prefix);
        let chars: Vec<char> = text.chars().collect();
        let first = chars.iter().position(|c| !c.is_whitespace()).unwrap_or(0);
        let mut in_run = seen_verse;
        let mut i = 0;
        let mut at_boundary = true;
        while i < chars.len() {
            let c = chars[i];
            if c.is_ascii_digit() && at_boundary && (in_run || i == first) {
                let mut j = i;
                while j < chars.len() && chars[j].is_ascii_digit() {
                    j += 1;
                }
                let num: u32 = chars[i..j].iter().collect::<String>().parse().unwrap_or(0);
                let glued = j < chars.len() && !chars[j].is_whitespace() && !chars[j].is_ascii_digit();
                if glued && num == expected {
                    if !out.is_empty() && !out.ends_with('\n') {
                        out.push('\n');
                    }
                    out.push_str(&format!("\\v {num} "));
                    report.inferred.push((chapter, num));
                    expected += 1;
                    in_run = true;
                    seen_verse = true;
                    i = j;
                    at_boundary = false;
                    continue;
                } else if glued && i == first && !in_run && !ambiguous_here {
                    ambiguous_here = true;
                    report.ambiguous.push(chapter);
                }
            }
            at_boundary = !c.is_alphanumeric();
            out.push(c);
            i += 1;
        }
    }
    (out, report)
}

/// D35 findings, one per verse whose designator carries more than a number or range and more than one
/// character after it (a lone ASCII letter is a segment such as `5a`).
pub fn d35_findings(model: &UsfmModel, lang: &str) -> Vec<Finding> {
    let mut out = Vec::new();
    for c in &model.chapters {
        for v in c.verses.iter().filter(|v| v.glued) {
            let rest = v.label.strip_prefix(&v.label_doc).unwrap_or("");
            if rest.chars().count() == 1 && rest.chars().all(|x| x.is_ascii_alphabetic()) {
                continue;
            }
            out.push(
                Finding::new(FindingCode::UsfmVerseNumberGlued, Severity::Warning)
                    .lang(lang)
                    .chapter(c.number)
                    .detail(format!("verse {}: the word `{rest}` was put back into the verse text", v.label_doc)),
            );
        }
    }
    out
}

/// The findings for a [`RepairReport`]: one per inferred verse, one per ambiguous chapter.
pub fn d36_findings(report: &RepairReport, lang: &str) -> Vec<Finding> {
    let mut out: Vec<Finding> = report
        .inferred
        .iter()
        .map(|(c, v)| Finding::new(FindingCode::UsfmVerseInferred, Severity::Warning).lang(lang).chapter(*c).detail(format!("verse {v} inferred from a bare number")))
        .collect();
    out.extend(report.ambiguous.iter().map(|c| {
        Finding::new(FindingCode::UsfmVersesAmbiguous, Severity::Warning).lang(lang).chapter(*c).detail("numbers that may be verse numbers, pattern ambiguous: text kept as written")
    }));
    out
}

/// Parse with both repairs: returns the model of the repaired text and all findings (D35 per verse, D36 per
/// inferred verse or ambiguous chapter).
pub fn parse_book_repaired(source: &str, lang: &str) -> (UsfmModel, Vec<Finding>) {
    let (text, report) = repair_bare_verses(source);
    let model = crate::usfm::parse_book(&text);
    let mut f = d35_findings(&model, lang);
    f.extend(d36_findings(&report, lang));
    (model, f)
}
