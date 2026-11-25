//! The "proposed look" structure option (P3b): heading levels and semantic classes chosen to match
//! `proposed-doc-format.docx`, Chapters 1 and 2, as measured from the sample (report section B3; every
//! rule below cites the count it comes from). The interleaving order is NOT touched: it stays whatever the
//! reverse-engineered algorithm produced, so the comparison with the sample shows where the algorithm and the
//! sample differ.
//!
//! The input is the interleaver's parts. A part belongs to the resource named by the last
//! `<h1 class='book-name'>` part; its kind (book intro, chapter intro, notes, commentary, word list, scripture)
//! is read from its content. (A production version would tag parts with a role instead of reading it back;
//! this file is the single place that does the reading, and it is removable: `structure` defaults to
//! `as_built`.)
//!
//! Output classes (the style table maps each to a Word style and a CSS rule):
//! `h1.resource-title` (Heading 1, a `p.line-space` is put before it except the first), `h2.tn-verse-ref`,
//! `h4.tn-note`, `h4.bc-question`, `p.indented` (Indented definition text), `p.tw-list`, `li.num-2`, `li.num-3`,
//! `li.bullet-3`; plain `h2`, `h3`, `h4`, `p` elsewhere.

use doc_model::model::Part;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Res {
    Tn,
    Bc,
    Tw,
    Ulb,
    Other,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    TnNotes,
    TnIntro,
    BcIntro,
    BcChapter,
    TwList,
    Scripture,
    Other,
}

struct El<'a> {
    tag: &'a str,
    attrs: &'a str,
    inner: &'a str,
}

/// Split a fragment into its top-level elements (`h1`..`h6`, `p`, `ul`, `ol`, `div`, `hr`). Text between
/// elements is ignored; the fragments the core produces are well formed.
fn elements(s: &str) -> Vec<El<'_>> {
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(lt) = s[i..].find('<') {
        let start = i + lt;
        let rest = &s[start + 1..];
        let name_end = rest.find(|c: char| !c.is_ascii_alphanumeric()).unwrap_or(rest.len());
        let tag = &rest[..name_end];
        let Some(gt) = rest.find('>') else { break };
        let attrs = rest[name_end..gt].trim_end_matches('/').trim();
        if matches!(tag, "hr" | "br") || rest[..gt].ends_with('/') {
            i = start + 1 + gt + 1;
            if tag == "hr" {
                out.push(El { tag: "hr", attrs: "", inner: "" });
            }
            continue;
        }
        // find the matching close tag, counting nesting of the same tag
        let open = format!("<{tag}");
        let close = format!("</{tag}>");
        let body_start = start + 1 + gt + 1;
        let (mut depth, mut j, mut end) = (1usize, body_start, None);
        while depth > 0 {
            let next_close = s[j..].find(&close).map(|k| j + k);
            let next_open = s[j..].find(&open).map(|k| j + k).filter(|k| s[k + open.len()..].starts_with(|c: char| c == '>' || c == ' '));
            match (next_open, next_close) {
                (Some(o), Some(c)) if o < c => {
                    depth += 1;
                    j = o + open.len();
                }
                (_, Some(c)) => {
                    depth -= 1;
                    j = c + close.len();
                    if depth == 0 {
                        end = Some(c);
                    }
                }
                _ => break,
            }
        }
        let Some(end) = end else { break };
        out.push(El { tag, attrs, inner: &s[body_start..end] });
        i = end + close.len();
    }
    out
}

fn strip_tags(s: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn heading_level(tag: &str) -> Option<u32> {
    let b = tag.as_bytes();
    if b.len() == 2 && b[0] == b'h' && (b'1'..=b'6').contains(&b[1]) {
        Some(u32::from(b[1] - b'0'))
    } else {
        None
    }
}

fn kind_of(res: Res, content: &str, seen_scripture: bool) -> Kind {
    match res {
        Res::Ulb => Kind::Scripture,
        Res::Tw => Kind::TwList,
        Res::Tn => {
            if content.contains("<h4>") && elements(content).first().is_some_and(|e| e.tag == "div") {
                Kind::TnNotes
            } else {
                Kind::TnIntro
            }
        }
        Res::Bc => {
            if content.contains("<h2>") && seen_scripture {
                Kind::BcChapter
            } else {
                Kind::BcIntro
            }
        }
        Res::Other => Kind::Other,
    }
}

/// Look level for a DOC heading level, per kind. Evidence (sample vs the algorithm's output, matched by heading
/// text, report B3): TN notes `h4`->H2 (42 of 57), `h5`->H4 (136 of 136); TN chapter intro `h2`->H2, `h4`->H3, `h5`->H4;
/// TN book intro identity; BC book intro `h3`->H2, `h4`->H3, `h5`->H3; BC chapter identity.
fn level_map(kind: Kind, doc_level: u32, chapter_intro: bool) -> u32 {
    match (kind, doc_level) {
        (Kind::TnNotes, 4) => 2,
        (Kind::TnNotes, 5) => 4,
        (Kind::TnIntro, 4) if chapter_intro => 3,
        (Kind::TnIntro, 5) if chapter_intro => 4,
        (Kind::BcIntro, 3) => 2,
        (Kind::BcIntro, 4 | 5) => 3,
        (_, l) => l,
    }
}

fn is_ref_line(text: &str) -> bool {
    text.starts_with('[') && text.ends_with(']') && text.len() < 40
}

fn render(kind: Kind, content: &str, chapter_intro: bool) -> String {
    let mut out = String::new();
    fn walk(kind: Kind, els: &[El<'_>], chapter_intro: bool, out: &mut String, after_h4: &mut bool) {
        let mut idx = 0;
        while idx < els.len() {
            let e = &els[idx];
            idx += 1;
            if e.tag == "div" {
                if kind == Kind::TwList {
                    out.push_str(&format!("<p class=\"tw-list\">{}</p>\n", e.inner.trim()));
                } else {
                    walk(kind, &elements(e.inner), chapter_intro, out, after_h4);
                }
            } else if let Some(l) = heading_level(e.tag) {
                let nl = level_map(kind, l, chapter_intro);
                let cls = match (kind, nl) {
                    (Kind::TnNotes, 2) => " class=\"tn-verse-ref\"",
                    (Kind::TnNotes, 4) => " class=\"tn-note\"",
                    (Kind::BcChapter, 4) => " class=\"bc-question\"",
                    _ => "",
                };
                // BC: the bracketed reference paragraph after a question moves into the heading (sample, em space)
                let mut inner = e.inner.trim().to_string();
                if kind == Kind::BcChapter && nl == 4 {
                    if let Some(n) = els.get(idx) {
                        if n.tag == "p" && is_ref_line(&strip_tags(n.inner)) {
                            inner = format!("{inner}\u{2003}{}", strip_tags(n.inner));
                            idx += 1;
                        }
                    }
                }
                // keep the id attribute DOC puts on headings (cross-reference targets)
                let id = e.attrs.split_whitespace().find(|a| a.starts_with("id=")).map(|a| format!(" {a}")).unwrap_or_default();
                out.push_str(&format!("<h{nl}{cls}{id}>{inner}</h{nl}>\n"));
                *after_h4 = nl == 4;
            } else if e.tag == "p" {
                let see = strip_tags(e.inner).starts_with("See");
                let cls = if *after_h4 && !see { " class=\"indented\"" } else { "" };
                out.push_str(&format!("<p{cls}>{}</p>\n", e.inner.trim()));
            } else if e.tag == "ul" || e.tag == "ol" {
                let li_class = match (kind, e.tag) {
                    (Kind::TnIntro, "ol") => "num-2",
                    (Kind::BcIntro, "ol") => "num-3",
                    (_, "ol") => "num-2",
                    _ => "bullet-3",
                };
                out.push_str(&format!("<{}>\n", e.tag));
                for li in elements(e.inner).iter().filter(|x| x.tag == "li") {
                    out.push_str(&format!("<li class=\"{li_class}\">{}</li>\n", strip_li(li.inner)));
                }
                out.push_str(&format!("</{}>\n", e.tag));
                *after_h4 = false;
            }
        }
    }
    fn strip_li(s: &str) -> String {
        // `<li><p>text</p></li>` (loose list) to the text
        let t = s.trim();
        match elements(t).as_slice() {
            [p] if p.tag == "p" => p.inner.trim().to_string(),
            _ => t.to_string(),
        }
    }
    let els = elements(content);
    let mut after_h4 = false;
    walk(kind, &els, chapter_intro, &mut out, &mut after_h4);
    out
}

/// Rewrite the parts in place into the proposed look.
pub fn restructure(parts: &mut Vec<Part>) {
    let mut res = Res::Other;
    let mut seen_scripture = false;
    let mut first_h1 = true;
    let mut out: Vec<Part> = Vec::with_capacity(parts.len() + 8);
    let mut tn_intro_seen_after_scripture = false;
    for p in parts.drain(..) {
        let c = p.content.trim();
        if let Some(title) = c.strip_prefix("<h1 class='book-name'>").and_then(|t| t.strip_suffix("</h1>")) {
            res = match title {
                "Translation Notes" => Res::Tn,
                "Bible Commentary" => Res::Bc,
                "Translation Words" => Res::Tw,
                "Unlocked Literal Bible" => Res::Ulb,
                _ => Res::Other,
            };
            if res == Res::Ulb {
                seen_scripture = true;
                tn_intro_seen_after_scripture = false;
            }
            if !first_h1 {
                out.push(Part { content: "<p class=\"line-space\"></p>".to_string(), ..p.clone() });
            }
            first_h1 = false;
            out.push(Part { content: format!("<h1 class=\"resource-title\">{title}</h1>"), ..p });
            continue;
        }
        if c == "&nbsp;" {
            continue; // DOC's spacer part: the line-space paragraph before each h1 replaces it
        }
        let kind = kind_of(res, c, seen_scripture);
        let chapter_intro = res == Res::Tn && seen_scripture_or_chapter(&out) && !tn_intro_seen_after_scripture;
        if kind == Kind::TnIntro && chapter_intro {
            tn_intro_seen_after_scripture = false;
        }
        let content = match kind {
            Kind::Scripture => p.content.clone(),
            Kind::Other => p.content.clone(),
            k => render(k, c, kind == Kind::TnIntro && is_chapter_intro(c)),
        };
        out.push(Part { content, ..p });
    }
    *parts = out;
}

fn seen_scripture_or_chapter(_out: &[Part]) -> bool {
    true
}

/// A TN chapter intro starts with `<h2>… General Notes`; the book intro with `<h2>Introduction to …`.
fn is_chapter_intro(content: &str) -> bool {
    elements(content).first().is_some_and(|e| e.tag == "h2" && strip_tags(e.inner).contains("General Notes"))
}
